//! Compilación de expresiones. `expr(e, dst)` deja el valor en `dst`; `operando(e)` devuelve
//! un registro con el valor (el de la propia variable si ya está en uno: sin copiar).

use super::{Compilador, Destino, segura_sobre_variable};
use crate::ast::{Clave, Expr};
use crate::bytecode::{Constante, Instr, OpBin, OpUn, Reg};
use crate::fuente::{Pos, Resultado, error};

fn op_unaria(op: &str) -> Option<OpUn> {
    Some(match op {
        "-" => OpUn::Negar,
        "+" => OpUn::ANumero,
        "!" => OpUn::No,
        "~" => OpUn::NoBits,
        "typeof" => OpUn::Tipo,
        "void" => OpUn::Vacio,
        _ => return None,
    })
}

/// Un número literal que cabe exacto en un `i32` (y no es `-0`): puede ir dentro de la instrucción.
fn entero_literal(e: &Expr) -> Option<i32> {
    let Expr::Numero(n) = e else { return None };
    let i = *n as i32;
    (i as f64 == *n && !(*n == 0.0 && n.is_sign_negative())).then_some(i)
}

/// `+=` → `+`, `**=` → `**`...
/// Las operaciones que pueden llevar un entero literal dentro de la instrucción (`BinariaInm`).
fn con_inmediato(op: OpBin) -> bool {
    matches!(
        op,
        OpBin::Suma
            | OpBin::Resta
            | OpBin::Mul
            | OpBin::YBits
            | OpBin::OBits
            | OpBin::XorBits
            | OpBin::Izq
            | OpBin::Der
            | OpBin::DerSinSigno
    )
}

fn op_compuesta(op: &str) -> Option<OpBin> {
    OpBin::desde(op.strip_suffix('=')?)
}

impl Compilador<'_> {
    /// Un registro con el valor de `e`.
    pub(super) fn operando(&mut self, e: &Expr) -> Resultado<Reg> {
        if let Expr::Ident(n) = e
            && let Destino::Local(r) = self.resolver(n)
        {
            return Ok(r);
        }
        if let (Expr::Este, Some(r)) = (e, self.ctx().este) {
            return Ok(r);
        }
        let t = self.temporal()?;
        self.expr(e, t)?;
        Ok(t)
    }

    /// Deja el valor de `e` en `dst`.
    pub(super) fn expr(&mut self, e: &Expr, dst: Reg) -> Resultado<()> {
        match e {
            Expr::Numero(n) => self.numero(*n, dst),
            Expr::Cadena(s) => {
                let a = self.atomo(s);
                let k = self.constante(Constante::Cadena(a));
                self.emitir(Instr::Constante { dst, k });
            }
            Expr::Regex(patron, banderas) => {
                let (patron, banderas) = (self.atomo(patron), self.atomo(banderas));
                self.emitir(Instr::CrearRegex { dst, patron, banderas });
            }
            Expr::Bool(valor) => _ = self.emitir(Instr::Booleano { dst, valor: *valor }),
            Expr::Nulo => _ = self.emitir(Instr::Nulo { dst }),
            Expr::Este => match self.ctx().este {
                Some(r) if r == dst => {}
                Some(r) => _ = self.emitir(Instr::Mover { dst, src: r }),
                None => _ = self.emitir(Instr::Este { dst }),
            },
            Expr::Ident(n) if n == "undefined" && matches!(self.resolver(n), Destino::Global(_)) => {
                self.emitir(Instr::Indefinido { dst });
            }
            Expr::Ident(n) => self.cargar(n, dst),
            Expr::Arreglo(v) => self.arreglo(v, dst)?,
            Expr::Objeto(props) => self.objeto(props, dst)?,
            Expr::Funcion(f) => {
                let funcion = self.funcion(f, true)?;
                self.emitir(Instr::Cierre { dst, funcion });
            }
            Expr::Unaria(op, a) => self.unaria(op, a, dst)?,
            Expr::Actualizar { op, prefijo, destino } => self.actualizar(op, *prefijo, destino, dst)?,
            Expr::Binaria(op, a, b) => {
                let op = OpBin::desde(op).ok_or_else(|| sin_operador(op))?;
                // `a + 3`, `x & 0xff`, `h >> 8`: el entero va dentro de la instrucción.
                if let (true, Some(valor)) = (con_inmediato(op), entero_literal(b)) {
                    let ra = self.operando(a)?;
                    self.emitir(Instr::BinariaInm { op, dst, a: ra, valor });
                    return Ok(());
                }
                let (ra, rb) = (self.operando(a)?, self.operando(b)?);
                self.emitir(Instr::Binaria { op, dst, a: ra, b: rb });
            }
            Expr::Logica(op, a, b) => self.logica(op, a, b, dst)?,
            Expr::Asignar(op, destino, valor) => self.asignar(op, destino, valor, dst)?,
            Expr::Condicional(c, a, b) => self.condicional(c, a, b, dst)?,
            Expr::Llamar(f, args) => self.llamar(f, args, dst)?,
            Expr::Nuevo(f, args) => {
                let rf = self.operando(f)?;
                let (desde, n) = self.argumentos(args)?;
                self.emitir(Instr::Construir { dst, f: rf, args: desde, n });
            }
            Expr::Miembro(obj, nombre) => {
                let o = self.operando(obj)?;
                let nombre = self.atomo(nombre);
                self.emitir(Instr::LeerProp { dst, obj: o, nombre });
            }
            Expr::Indice(obj, i) => {
                let (o, ri) = (self.operando(obj)?, self.operando(i)?);
                self.emitir(Instr::LeerIndice { dst, obj: o, indice: ri });
            }
            Expr::Secuencia(v) => {
                for (i, x) in v.iter().enumerate() {
                    if i + 1 == v.len() {
                        self.expr(x, dst)?;
                    } else {
                        let t = self.temporal()?;
                        self.expr(x, t)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn arreglo(&mut self, v: &[Option<Expr>], dst: Reg) -> Resultado<()> {
        let desde = self.consecutivos(v.len())?;
        for (i, x) in v.iter().enumerate() {
            let r = desde + i as Reg;
            match x {
                Some(e) => self.expr(e, r)?,
                None => _ = self.emitir(Instr::Hueco { dst: r }),
            }
        }
        let n = u16::try_from(v.len()).or_else(|_| error(Pos::default(), "arreglo literal demasiado largo"))?;
        self.emitir(Instr::CrearArreglo { dst, desde, n });
        Ok(())
    }

    /// Reserva `n` registros seguidos *antes* de calcular nada: los temporales de cada
    /// elemento van después, así que los elementos quedan juntos.
    fn consecutivos(&mut self, n: usize) -> Resultado<Reg> {
        let desde = self.marca();
        for _ in 0..n {
            self.temporal()?;
        }
        Ok(desde)
    }

    fn objeto(&mut self, props: &[(Clave, Expr)], dst: Reg) -> Resultado<()> {
        // Sobre una variable se construye aparte: `x = {a: x}` debe leer la `x` vieja.
        let o = if self.es_temporal(dst) { dst } else { self.temporal()? };
        self.emitir(Instr::CrearObjeto { dst: o });
        for (clave, valor) in props {
            let marca = self.marca();
            match clave {
                Clave::Nombre(n) => {
                    let v = self.operando(valor)?;
                    let nombre = self.atomo(n);
                    self.emitir(Instr::EscribirProp { obj: o, nombre, src: v });
                }
                Clave::Calculada(k) => {
                    let (rk, v) = (self.operando(k)?, self.operando(valor)?);
                    self.emitir(Instr::EscribirIndice { obj: o, indice: rk, src: v });
                }
            }
            self.soltar(marca);
        }
        if o != dst {
            self.emitir(Instr::Mover { dst, src: o });
        }
        Ok(())
    }

    fn unaria(&mut self, op: &str, a: &Expr, dst: Reg) -> Resultado<()> {
        match (op, a) {
            ("-", Expr::Numero(n)) => self.numero(-n, dst),
            ("delete", Expr::Miembro(obj, nombre)) => {
                let o = self.operando(obj)?;
                let nombre = self.atomo(nombre);
                self.emitir(Instr::BorrarProp { dst, obj: o, nombre });
            }
            ("delete", Expr::Indice(obj, i)) => {
                let (o, ri) = (self.operando(obj)?, self.operando(i)?);
                self.emitir(Instr::BorrarIndice { dst, obj: o, indice: ri });
            }
            ("delete", _) => _ = self.emitir(Instr::Booleano { dst, valor: true }),
            ("typeof", Expr::Ident(n)) if matches!(self.resolver(n), Destino::Global(_)) => {
                let nombre = self.atomo(n);
                self.emitir(Instr::TipoGlobal { dst, nombre });
            }
            _ => {
                let op = op_unaria(op).ok_or_else(|| sin_operador(op))?;
                let ra = self.operando(a)?;
                self.emitir(Instr::Unaria { op, dst, a: ra });
            }
        }
        Ok(())
    }

    fn logica(&mut self, op: &str, a: &Expr, b: &Expr, dst: Reg) -> Resultado<()> {
        self.expr(a, dst)?;
        let salto = match op {
            "&&" => self.emitir(Instr::SaltarSiNo { c: dst, destino: 0 }),
            "||" => self.emitir(Instr::SaltarSi { c: dst, destino: 0 }),
            _ => self.emitir(Instr::SaltarSiDefinido { c: dst, destino: 0 }),
        };
        self.expr(b, dst)?;
        let fin = self.pc();
        self.parchear(salto, fin);
        Ok(())
    }

    fn condicional(&mut self, c: &Expr, a: &Expr, b: &Expr, dst: Reg) -> Resultado<()> {
        let rc = self.operando(c)?;
        let a_otro = self.emitir(Instr::SaltarSiNo { c: rc, destino: 0 });
        self.expr(a, dst)?;
        let a_fin = self.emitir(Instr::Saltar { destino: 0 });
        let otro = self.pc();
        self.parchear(a_otro, otro);
        self.expr(b, dst)?;
        let fin = self.pc();
        self.parchear(a_fin, fin);
        Ok(())
    }

    /// Argumentos en registros seguidos; devuelve el primero y cuántos son.
    fn argumentos(&mut self, args: &[Expr]) -> Resultado<(Reg, u16)> {
        let desde = self.consecutivos(args.len())?;
        for (i, a) in args.iter().enumerate() {
            self.expr(a, desde + i as Reg)?;
        }
        let n = u16::try_from(args.len()).or_else(|_| error(Pos::default(), "demasiados argumentos"))?;
        Ok((desde, n))
    }

    /// `f(x)`, `obj.m(x)` (con `this = obj`) u `obj[k](x)`.
    fn llamar(&mut self, f: &Expr, args: &[Expr], dst: Reg) -> Resultado<()> {
        let (rf, este) = match f {
            Expr::Miembro(obj, nombre) => {
                let este = self.operando(obj)?;
                let rf = self.temporal()?;
                let nombre = self.atomo(nombre);
                self.emitir(Instr::LeerProp { dst: rf, obj: este, nombre });
                (rf, este)
            }
            Expr::Indice(obj, i) => {
                let (este, ri) = (self.operando(obj)?, self.operando(i)?);
                let rf = self.temporal()?;
                self.emitir(Instr::LeerIndice { dst: rf, obj: este, indice: ri });
                (rf, este)
            }
            _ => {
                let rf = self.operando(f)?;
                let este = self.temporal()?;
                self.emitir(Instr::Indefinido { dst: este });
                (rf, este)
            }
        };
        let (desde, n) = self.argumentos(args)?;
        self.emitir(Instr::Llamar { dst, f: rf, este, args: desde, n });
        Ok(())
    }

    /// `x = v`, `x += v`, `x &&= v`, `o.p = v`, `o[k] = v`. El valor queda también en `dst`.
    fn asignar(&mut self, op: &str, destino: &Expr, valor: &Expr, dst: Reg) -> Resultado<()> {
        match destino {
            Expr::Ident(n) => self.asignar_nombre(op, n, valor, dst),
            Expr::Miembro(obj, nombre) => {
                let o = self.operando(obj)?;
                let nombre = self.atomo(nombre);
                let leer = Instr::LeerProp { dst, obj: o, nombre };
                self.combinar(op, Some(leer), valor, dst)?;
                self.emitir(Instr::EscribirProp { obj: o, nombre, src: dst });
                Ok(())
            }
            Expr::Indice(obj, i) => {
                let (o, ri) = (self.operando(obj)?, self.operando(i)?);
                let leer = Instr::LeerIndice { dst, obj: o, indice: ri };
                self.combinar(op, Some(leer), valor, dst)?;
                self.emitir(Instr::EscribirIndice { obj: o, indice: ri, src: dst });
                Ok(())
            }
            _ => error(Pos::default(), "no se puede asignar a esto"),
        }
    }

    /// Calcula en `dst` el valor nuevo: `v` para `=`, o `actual op v`. `leer` trae el valor
    /// actual a `dst` (`None` si ya está ahí).
    fn combinar(&mut self, op: &str, leer: Option<Instr>, valor: &Expr, dst: Reg) -> Resultado<()> {
        if op == "=" {
            return self.expr(valor, dst);
        }
        if let Some(leer) = leer {
            self.emitir(leer);
        }
        if let Some(logico) = op.strip_suffix('=').filter(|o| matches!(*o, "&&" | "||" | "??")) {
            let salto = match logico {
                "&&" => self.emitir(Instr::SaltarSiNo { c: dst, destino: 0 }),
                "||" => self.emitir(Instr::SaltarSi { c: dst, destino: 0 }),
                _ => self.emitir(Instr::SaltarSiDefinido { c: dst, destino: 0 }),
            };
            self.expr(valor, dst)?;
            let fin = self.pc();
            self.parchear(salto, fin);
            return Ok(());
        }
        let binop = op_compuesta(op).ok_or_else(|| sin_operador(op))?;
        if let (true, Some(v)) = (con_inmediato(binop), entero_literal(valor)) {
            self.emitir(Instr::BinariaInm { op: binop, dst, a: dst, valor: v });
            return Ok(());
        }
        let rv = self.operando(valor)?;
        self.emitir(Instr::Binaria { op: binop, dst, a: dst, b: rv });
        Ok(())
    }

    fn asignar_nombre(&mut self, op: &str, n: &str, valor: &Expr, dst: Reg) -> Resultado<()> {
        match self.resolver(n) {
            // En su registro y sin copias: `x += 1` es `rX = rX Suma r1`.
            Destino::Local(r) if op == "=" && segura_sobre_variable(valor) => self.expr(valor, r)?,
            Destino::Local(r) if op_compuesta(op).is_some() => {
                let op = op_compuesta(op).ok_or_else(|| sin_operador(op))?;
                match (op, entero_literal(valor)) {
                    (op, Some(v)) if con_inmediato(op) => {
                        _ = self.emitir(Instr::BinariaInm { op, dst: r, a: r, valor: v })
                    }
                    _ => {
                        let rv = self.operando(valor)?;
                        self.emitir(Instr::Binaria { op, dst: r, a: r, b: rv });
                    }
                }
            }
            _ => {
                let t = self.temporal()?;
                if op != "=" {
                    self.cargar(n, t);
                }
                self.combinar(op, None, valor, t)?;
                self.guardar(n, t);
                self.emitir(Instr::Mover { dst, src: t });
                return Ok(());
            }
        }
        if let Destino::Local(r) = self.resolver(n)
            && r != dst
        {
            self.emitir(Instr::Mover { dst, src: r });
        }
        Ok(())
    }

    /// `++x` / `x--` sobre una variable o una propiedad.
    fn actualizar(&mut self, op: &str, prefijo: bool, destino: &Expr, dst: Reg) -> Resultado<()> {
        // `++x` es `x - (-1)` y `--x` es `x - 1`: la resta siempre convierte a número (con
        // `+` y un 1, `"5"` daría `"51"`) y en coma flotante el resultado es idéntico a `x + 1`.
        let paso = if op == "++" { -1 } else { 1 };
        if let Expr::Ident(n) = destino
            && let Destino::Local(r) = self.resolver(n)
        {
            if prefijo {
                self.emitir(Instr::BinariaInm { op: OpBin::Resta, dst: r, a: r, valor: paso });
                if r != dst {
                    self.emitir(Instr::Mover { dst, src: r });
                }
            } else {
                let viejo = if dst == r { self.temporal()? } else { dst };
                self.emitir(Instr::Unaria { op: OpUn::ANumero, dst: viejo, a: r });
                self.emitir(Instr::BinariaInm { op: OpBin::Resta, dst: r, a: viejo, valor: paso });
                if viejo != dst {
                    self.emitir(Instr::Mover { dst, src: viejo });
                }
            }
            return Ok(());
        }
        // Propiedad o variable fuera de registro: leer, convertir, restar, escribir.
        let viejo = self.temporal()?;
        let nuevo = self.temporal()?;
        let escribir = self.leer_destino(destino, viejo)?;
        self.emitir(Instr::Unaria { op: OpUn::ANumero, dst: viejo, a: viejo });
        self.emitir(Instr::BinariaInm { op: OpBin::Resta, dst: nuevo, a: viejo, valor: paso });
        self.escribir_destino(escribir, nuevo);
        self.emitir(Instr::Mover { dst, src: if prefijo { nuevo } else { viejo } });
        Ok(())
    }

    /// Lee un destino de asignación en `dst` y devuelve cómo escribirlo después.
    fn leer_destino(&mut self, destino: &Expr, dst: Reg) -> Resultado<Escritura> {
        Ok(match destino {
            Expr::Ident(n) => {
                self.cargar(n, dst);
                Escritura::Nombre(alloc::string::String::from(n.as_str()))
            }
            Expr::Miembro(obj, nombre) => {
                let o = self.operando(obj)?;
                let nombre = self.atomo(nombre);
                self.emitir(Instr::LeerProp { dst, obj: o, nombre });
                Escritura::Prop(o, nombre)
            }
            Expr::Indice(obj, i) => {
                let (o, ri) = (self.operando(obj)?, self.operando(i)?);
                self.emitir(Instr::LeerIndice { dst, obj: o, indice: ri });
                Escritura::Indice(o, ri)
            }
            _ => return error(Pos::default(), "++/-- necesita una variable o una propiedad"),
        })
    }

    fn escribir_destino(&mut self, e: Escritura, src: Reg) {
        match e {
            Escritura::Nombre(n) => self.guardar(&n, src),
            Escritura::Prop(obj, nombre) => _ = self.emitir(Instr::EscribirProp { obj, nombre, src }),
            Escritura::Indice(obj, indice) => _ = self.emitir(Instr::EscribirIndice { obj, indice, src }),
        }
    }
}

/// Cómo escribir de vuelta un destino ya leído.
enum Escritura {
    Nombre(alloc::string::String),
    Prop(Reg, u32),
    Indice(Reg, Reg),
}

fn sin_operador(op: &str) -> crate::fuente::Error {
    crate::fuente::Error { pos: Pos::default(), mensaje: alloc::format!("operador {op} sin instrucción") }
}
