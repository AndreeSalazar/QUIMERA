//! Compilador: árbol de sintaxis → bytecode de registros (Stencil).
//!
//! Cada función se compila con su contexto: dónde vive cada variable (registro o hueco del
//! entorno), los registros temporales y los saltos pendientes de los bucles. Las variables
//! sin capturar se leen y escriben en su registro sin copias: `x = x + 1` es una sola
//! instrucción `rX = rX Suma r1`.

mod control;
mod expresiones;
mod sentencias;

use crate::ambitos::{self, Ambitos};
use crate::ast::{Expr, Funcion as FuncionAst, Param, Sent};
use crate::bytecode::{Constante, Funcion, Instr, Reg, Stencil};
use crate::fuente::{Pos, Resultado, error};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Compila un programa ya analizado.
pub fn compilar(programa: &[Sent]) -> Resultado<Stencil> {
    let ambitos = ambitos::analizar(programa);
    let mut c = Compilador {
        ambitos: &ambitos,
        atomos: Vec::new(),
        indice: BTreeMap::new(),
        funciones: Vec::new(),
        pila: Vec::new(),
    };
    c.funciones.push(None);
    let programa_fn = c.cuerpo(
        &[],
        programa,
        0,
        Opciones {
            flecha: false,
            nombre: None,
            propio: None,
            global: true,
            generador: false,
            pos: Pos { linea: 1, columna: 1 },
        },
    )?;
    c.funciones[0] = Some(programa_fn);
    let funciones = c.funciones.into_iter().map(Option::unwrap_or_default).collect();
    Ok(Stencil { funciones, atomos: c.atomos })
}

/// Dónde vive una variable dentro de su función.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lugar {
    Reg(Reg),
    Hueco(u16),
}

/// Dónde está una variable vista desde la función actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destino {
    Local(Reg),
    Entorno { saltos: u16, hueco: u16 },
    Global(u32),
}

#[derive(Default)]
struct Bucle {
    romper: Vec<usize>,
    continuar: Vec<usize>,
    es_switch: bool,
    /// Regiones `try` abiertas al empezar el bucle: `break` cierra las de dentro.
    regiones: usize,
    /// Sus etiquetas (`fuera: for ...`), para `break fuera` y `continue fuera`.
    etiquetas: Vec<String>,
}

/// Un `try` abierto mientras se compila su cuerpo (o su `catch`).
#[derive(Clone)]
struct Region {
    /// Si hay un manejador activo que `FinIntentar` debe quitar al salir.
    manejador: bool,
    /// El `finally` que hay que ejecutar al salir.
    final_: Option<Vec<Sent>>,
}

#[derive(Default)]
struct Ctx {
    /// Modo estricto (lo hereda cada función de dentro).
    estricta: bool,
    codigo: Vec<Instr>,
    constantes: Vec<Constante>,
    lugares: BTreeMap<String, Lugar>,
    /// Primer registro temporal: los anteriores son parámetros y variables.
    primer_temporal: Reg,
    tope: Reg,
    maximo: Reg,
    entorno: u16,
    bucles: Vec<Bucle>,
    /// Etiquetas que se ponen al siguiente bucle que se abra (`fuera: for ...`).
    etiquetas_pendientes: Vec<String>,
    regiones: Vec<Region>,
    argumentos: Option<Reg>,
    /// El registro donde está `this` desde que empieza la función (si la función lo usa).
    este: Option<Reg>,
}

#[derive(Clone, Copy)]
struct Opciones<'a> {
    flecha: bool,
    nombre: Option<&'a str>,
    /// Nombre propio de una expresión de función (`function g() {}` dentro de una expresión).
    propio: Option<&'a str>,
    global: bool,
    generador: bool,
    /// Dónde empieza en el texto (para las trazas de los errores).
    pos: Pos,
}

struct Compilador<'a> {
    ambitos: &'a Ambitos,
    atomos: Vec<String>,
    indice: BTreeMap<String, u32>,
    /// Funciones terminadas; `None` mientras se compila (su índice ya está reservado).
    funciones: Vec<Option<Funcion>>,
    pila: Vec<Ctx>,
}

impl Compilador<'_> {
    fn ctx(&mut self) -> &mut Ctx {
        let n = self.pila.len() - 1;
        &mut self.pila[n]
    }

    fn atomo(&mut self, s: &str) -> u32 {
        if let Some(&n) = self.indice.get(s) {
            return n;
        }
        let n = self.atomos.len() as u32;
        self.atomos.push(String::from(s));
        self.indice.insert(String::from(s), n);
        n
    }

    fn emitir(&mut self, i: Instr) -> usize {
        let c = self.ctx();
        c.codigo.push(i);
        c.codigo.len() - 1
    }

    fn pc(&mut self) -> u32 {
        self.ctx().codigo.len() as u32
    }

    /// Completa el destino de un salto ya emitido.
    fn parchear(&mut self, en: usize, a: u32) {
        match &mut self.ctx().codigo[en] {
            Instr::Saltar { destino }
            | Instr::SaltarSi { destino, .. }
            | Instr::SaltarSiNo { destino, .. }
            | Instr::SaltarSiDefinido { destino, .. }
            | Instr::SaltarSiNoCompara { destino, .. }
            | Instr::SaltarSiCompara { destino, .. } => *destino = a,
            Instr::Intentar { captura } => *captura = a,
            _ => {}
        }
    }

    fn temporal(&mut self) -> Resultado<Reg> {
        let c = self.ctx();
        if c.tope == Reg::MAX {
            return error(Pos::default(), "la función necesita más de 65535 registros");
        }
        let r = c.tope;
        c.tope += 1;
        c.maximo = c.maximo.max(c.tope);
        Ok(r)
    }

    fn marca(&mut self) -> Reg {
        self.ctx().tope
    }

    fn soltar(&mut self, hasta: Reg) {
        self.ctx().tope = hasta;
    }

    fn es_temporal(&mut self, r: Reg) -> bool {
        r >= self.ctx().primer_temporal
    }

    fn constante(&mut self, k: Constante) -> u32 {
        let c = self.ctx();
        if let Some(i) = c.constantes.iter().position(|x| *x == k) {
            return i as u32;
        }
        c.constantes.push(k);
        (c.constantes.len() - 1) as u32
    }

    fn numero(&mut self, n: f64, dst: Reg) {
        let entero = n as i32;
        // Un entero exacto (y no -0) va en la instrucción; el resto, a la tabla de constantes.
        if entero as f64 == n && !(n == 0.0 && n.is_sign_negative()) {
            self.emitir(Instr::Entero { dst, valor: entero });
        } else {
            let k = self.constante(Constante::Numero(n));
            self.emitir(Instr::Constante { dst, k });
        }
    }

    /// Busca una variable desde la función actual hacia fuera.
    fn resolver(&mut self, nombre: &str) -> Destino {
        let actual = self.pila.len() - 1;
        if let Some(l) = self.pila[actual].lugares.get(nombre) {
            return match *l {
                Lugar::Reg(r) => Destino::Local(r),
                Lugar::Hueco(hueco) => Destino::Entorno { saltos: 0, hueco },
            };
        }
        let mut saltos = u16::from(self.pila[actual].entorno > 0);
        for ctx in self.pila[..actual].iter().rev() {
            if let Some(Lugar::Hueco(hueco)) = ctx.lugares.get(nombre) {
                return Destino::Entorno { saltos, hueco: *hueco };
            }
            saltos += u16::from(ctx.entorno > 0);
        }
        Destino::Global(self.atomo(nombre))
    }

    fn cargar(&mut self, nombre: &str, dst: Reg) {
        match self.resolver(nombre) {
            Destino::Local(r) if r == dst => {}
            Destino::Local(r) => {
                self.emitir(Instr::Mover { dst, src: r });
            }
            Destino::Entorno { saltos, hueco } => {
                self.emitir(Instr::LeerEntorno { dst, saltos, hueco });
            }
            Destino::Global(nombre) => {
                self.emitir(Instr::LeerGlobal { dst, nombre });
            }
        }
    }

    fn guardar(&mut self, nombre: &str, src: Reg) {
        match self.resolver(nombre) {
            Destino::Local(r) if r == src => {}
            Destino::Local(r) => {
                self.emitir(Instr::Mover { dst: r, src });
            }
            Destino::Entorno { saltos, hueco } => {
                self.emitir(Instr::EscribirEntorno { saltos, hueco, src });
            }
            Destino::Global(nombre) => {
                self.emitir(Instr::EscribirGlobal { nombre, src });
            }
        }
    }

    /// Compila una función interna y devuelve su índice en el Stencil.
    fn funcion(&mut self, f: &FuncionAst, es_expresion: bool) -> Resultado<u32> {
        let indice = self.funciones.len();
        self.funciones.push(None);
        let nombre = f.nombre.as_deref();
        let propio = if es_expresion { nombre } else { None };
        let opciones = Opciones { flecha: f.flecha, nombre, propio, global: false, generador: f.generador, pos: f.pos };
        let compilada = self.cuerpo(&f.params, &f.cuerpo, ambitos::clave(f), opciones)?;
        self.funciones[indice] = Some(compilada);
        Ok(indice as u32)
    }

    /// Compila parámetros y cuerpo en un contexto nuevo.
    fn cuerpo(&mut self, params: &[Param], cuerpo: &[Sent], clave: usize, op: Opciones) -> Resultado<Funcion> {
        let info = self.ambitos.get(&clave).cloned().unwrap_or_default();
        let estricta = self.pila.last().is_some_and(|c| c.estricta) || usa_estricto(cuerpo);
        self.pila.push(Ctx { estricta, ..Ctx::default() });
        self.preparar_locales(params, &info, op)?;
        self.valores_por_defecto(params)?;
        if op.global {
            let mut vars = Vec::new();
            ambitos::declaraciones_var(cuerpo, &mut vars);
            for v in vars {
                let nombre = self.atomo(&v);
                self.emitir(Instr::DeclararGlobal { nombre });
            }
        }
        self.izar_funciones(cuerpo)?;
        for s in cuerpo {
            self.sentencia(s)?;
        }
        let t = self.temporal()?;
        self.emitir(Instr::Indefinido { dst: t });
        self.emitir(Instr::Retornar { src: t });
        let ctx = self.pila.pop().unwrap_or_default();
        let nombre = op.nombre.map(|n| self.atomo(n));
        Ok(Funcion {
            nombre,
            params: params.len() as u16,
            registros: ctx.maximo,
            entorno: ctx.entorno,
            flecha: op.flecha,
            argumentos: ctx.argumentos,
            generador: op.generador,
            estricta,
            linea: op.pos.linea,
            columna: op.pos.columna,
            codigo: ctx.codigo,
            constantes: ctx.constantes,
        })
    }

    /// Reparte parámetros y variables entre registros y huecos del entorno.
    fn preparar_locales(&mut self, params: &[Param], info: &ambitos::Info, op: Opciones) -> Resultado<()> {
        if op.global {
            return Ok(());
        }
        let mut huecos = 0u16;
        let mut capturados_en_registro = Vec::new();
        for (i, nombre) in info.locales.iter().enumerate() {
            let capturada = info.capturadas.contains(nombre);
            let lugar = if capturada {
                huecos += 1;
                Lugar::Hueco(huecos - 1)
            } else {
                Lugar::Reg(0) // se asigna abajo
            };
            if i < params.len() && capturada {
                capturados_en_registro.push((i as Reg, huecos - 1));
            }
            self.ctx().lugares.insert(nombre.clone(), lugar);
        }
        // Registros: primero los parámetros (llegan en r0, r1...), luego las demás variables.
        let n = params.len() as Reg;
        let c = self.ctx();
        c.tope = n;
        for (i, p) in params.iter().enumerate() {
            if let Some(Lugar::Reg(r)) = c.lugares.get_mut(&p.nombre) {
                *r = i as Reg;
            }
        }
        for nombre in info.locales.iter().skip(params.len()) {
            if let Some(Lugar::Reg(r)) = c.lugares.get_mut(nombre) {
                *r = c.tope;
                c.tope += 1;
            }
        }
        // `arguments`: la máquina lo deja en un registro; si está capturado, se copia al entorno.
        let arguments_capturado = match c.lugares.get("arguments") {
            _ if params.iter().any(|p| p.nombre == "arguments") => None,
            Some(Lugar::Reg(r)) => {
                c.argumentos = Some(*r);
                None
            }
            Some(Lugar::Hueco(h)) => {
                c.argumentos = Some(c.tope);
                c.tope += 1;
                Some((c.tope - 1, *h))
            }
            None => None,
        };
        // `this` no cambia dentro de una función: se carga una vez y cada `this.x` usa ese registro.
        if info.usa_este {
            c.este = Some(c.tope);
            c.tope += 1;
        }
        c.primer_temporal = c.tope;
        c.maximo = c.tope;
        c.entorno = huecos;
        if huecos > 0 {
            self.emitir(Instr::CrearEntorno { huecos });
        }
        if let Some(r) = self.ctx().este {
            self.emitir(Instr::Este { dst: r });
        }
        if let Some((reg, hueco)) = arguments_capturado {
            self.emitir(Instr::EscribirEntorno { saltos: 0, hueco, src: reg });
        }
        for (reg, hueco) in capturados_en_registro {
            self.emitir(Instr::EscribirEntorno { saltos: 0, hueco, src: reg });
        }
        if let Some(n) = op.propio {
            let t = self.temporal()?;
            self.emitir(Instr::FuncionActual { dst: t });
            self.guardar(n, t);
            self.soltar(t);
        }
        Ok(())
    }

    /// `function f(a = 1)`: si `a` llega como `undefined`, toma su valor por defecto.
    fn valores_por_defecto(&mut self, params: &[Param]) -> Resultado<()> {
        for p in params {
            let Some(defecto) = &p.defecto else { continue };
            let marca = self.marca();
            let (v, indef) = (self.temporal()?, self.temporal()?);
            self.cargar(&p.nombre, v);
            self.emitir(Instr::Indefinido { dst: indef });
            self.emitir(Instr::Binaria { op: crate::bytecode::OpBin::Estricto, dst: indef, a: v, b: indef });
            let salto = self.emitir(Instr::SaltarSiNo { c: indef, destino: 0 });
            self.expr(defecto, v)?;
            self.guardar(&p.nombre, v);
            let aqui = self.pc();
            self.parchear(salto, aqui);
            self.soltar(marca);
        }
        Ok(())
    }

    /// Las declaraciones `function f() {}` existen desde el principio de su función.
    fn izar_funciones(&mut self, cuerpo: &[Sent]) -> Resultado<()> {
        let mut funciones = Vec::new();
        recoger_funciones(cuerpo, &mut funciones);
        for f in funciones {
            let indice = self.funcion(f, false)?;
            let nombre = f.nombre.as_deref().unwrap_or("");
            // Si su variable vive en un registro, el cierre se crea ahí mismo.
            if let Destino::Local(r) = self.resolver(nombre) {
                self.emitir(Instr::Cierre { dst: r, funcion: indice });
                continue;
            }
            let t = self.temporal()?;
            self.emitir(Instr::Cierre { dst: t, funcion: indice });
            self.guardar(nombre, t);
            self.soltar(t);
        }
        Ok(())
    }
}

/// Declaraciones de función de un cuerpo, también dentro de bloques (no dentro de otras funciones).
fn recoger_funciones<'a>(cuerpo: &'a [Sent], v: &mut Vec<&'a FuncionAst>) {
    for s in cuerpo {
        match s {
            Sent::Funcion(f) => v.push(f),
            Sent::Bloque(b) => recoger_funciones(b, v),
            Sent::Si(_, a, b) => {
                recoger_funciones(core::slice::from_ref(a.as_ref()), v);
                if let Some(b) = b {
                    recoger_funciones(core::slice::from_ref(b.as_ref()), v);
                }
            }
            Sent::Mientras(_, c)
            | Sent::Hacer(c, _)
            | Sent::Para { cuerpo: c, .. }
            | Sent::ParaEn { cuerpo: c, .. } => {
                recoger_funciones(core::slice::from_ref(c.as_ref()), v);
            }
            Sent::Intentar { cuerpo, captura, final_ } => {
                recoger_funciones(cuerpo, v);
                if let Some((_, b)) = captura {
                    recoger_funciones(b, v);
                }
                if let Some(f) = final_ {
                    recoger_funciones(f, v);
                }
            }
            Sent::Elegir(_, casos) => casos.iter().for_each(|(_, c)| recoger_funciones(c, v)),
            _ => {}
        }
    }
}

/// Una expresión que solo escribe su destino en su última instrucción, después de leer todo
/// lo que necesita: se puede compilar directamente sobre el registro de una variable.
/// Si el cuerpo empieza con la directiva `"use strict"`.
fn usa_estricto(cuerpo: &[Sent]) -> bool {
    cuerpo
        .iter()
        .map_while(|s| match s {
            Sent::Expr(Expr::Cadena(t)) => Some(t),
            _ => None,
        })
        .any(|t| t == "use strict")
}

fn segura_sobre_variable(e: &Expr) -> bool {
    !matches!(
        e,
        Expr::Logica(..)
            | Expr::Condicional(..)
            | Expr::Objeto(_)
            | Expr::Asignar(..)
            | Expr::Actualizar { .. }
            | Expr::Secuencia(_)
    )
}
