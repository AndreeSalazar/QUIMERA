//! Análisis de ámbitos: qué variables tiene cada función y cuáles capturan sus funciones internas.
//!
//! Una variable que ninguna función interna usa vive en un registro (lo más rápido). Una
//! capturada tiene que sobrevivir a su función, así que vive en el entorno. Es la misma
//! división que hace SpiderMonkey entre *bindings* normales y *aliased*.

use crate::ast::{Clave, Decl, Expr, Funcion, Sent};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

/// Lo que se sabe de una función.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    /// Parámetros y luego variables y funciones declaradas, en orden y sin repetir.
    pub locales: Vec<String>,
    /// Las que alguna función interna usa.
    pub capturadas: BTreeSet<String>,
}

/// Información de cada función, por su dirección en el árbol (el programa usa la clave 0).
pub type Ambitos = BTreeMap<usize, Info>;

/// Clave de una función: su dirección. El árbol no se mueve entre el análisis y la compilación.
pub fn clave(f: &Funcion) -> usize {
    f as *const Funcion as usize
}

/// Analiza el programa entero.
pub fn analizar(programa: &[Sent]) -> Ambitos {
    let mut a = Analisis { pila: Vec::new(), out: BTreeMap::new() };
    a.out.insert(0, Info::default());
    // El programa no tiene locales: lo que declara es global.
    a.pila.push((0, BTreeSet::new(), false));
    a.sentencias(programa);
    a.out
}

fn anadir(v: &mut Vec<String>, n: &str) {
    if !v.iter().any(|x| x == n) {
        v.push(String::from(n));
    }
}

/// Nombres declarados en un cuerpo (sin entrar en las funciones internas).
pub fn declaraciones(cuerpo: &[Sent], v: &mut Vec<String>) {
    for s in cuerpo {
        declaraciones_de(s, v, false);
    }
}

/// Solo los nombres declarados con `var` (los que el programa crea como globales al empezar).
pub fn declaraciones_var(cuerpo: &[Sent], v: &mut Vec<String>) {
    for s in cuerpo {
        declaraciones_de(s, v, true);
    }
}

fn varias(cuerpo: &[Sent], v: &mut Vec<String>, solo_var: bool) {
    for s in cuerpo {
        declaraciones_de(s, v, solo_var);
    }
}

fn declaraciones_de(s: &Sent, v: &mut Vec<String>, solo_var: bool) {
    match s {
        Sent::Declarar(d, lista) if !solo_var || *d == Decl::Var => lista.iter().for_each(|(n, _)| anadir(v, n)),
        Sent::Funcion(f) if !solo_var => {
            if let Some(n) = &f.nombre {
                anadir(v, n);
            }
        }
        Sent::Si(_, a, b) => {
            declaraciones_de(a, v, solo_var);
            if let Some(b) = b {
                declaraciones_de(b, v, solo_var);
            }
        }
        Sent::Mientras(_, c) | Sent::Hacer(c, _) => declaraciones_de(c, v, solo_var),
        Sent::Para { inicio, cuerpo, .. } => {
            if let Some(i) = inicio {
                declaraciones_de(i, v, solo_var);
            }
            declaraciones_de(cuerpo, v, solo_var);
        }
        Sent::ParaEn { decl, nombre, cuerpo, .. } => {
            if decl.is_some_and(|d| !solo_var || d == Decl::Var) {
                anadir(v, nombre);
            }
            declaraciones_de(cuerpo, v, solo_var);
        }
        Sent::Intentar { cuerpo, captura, final_ } => {
            varias(cuerpo, v, solo_var);
            if let Some((nombre, bloque)) = captura {
                if let (Some(n), false) = (nombre, solo_var) {
                    anadir(v, n);
                }
                varias(bloque, v, solo_var);
            }
            if let Some(f) = final_ {
                varias(f, v, solo_var);
            }
        }
        Sent::Elegir(_, casos) => casos.iter().for_each(|(_, c)| varias(c, v, solo_var)),
        Sent::Bloque(b) => varias(b, v, solo_var),
        _ => {}
    }
}

struct Analisis {
    /// Funciones abiertas, de fuera hacia dentro: (clave, nombres locales).
    /// El `bool` dice si es una flecha (no tiene `arguments` propio).
    pila: Vec<(usize, BTreeSet<String>, bool)>,
    out: Ambitos,
}

impl Analisis {
    fn funcion(&mut self, f: &Funcion, es_expresion: bool) {
        let mut locales: Vec<String> = Vec::new();
        for p in &f.params {
            anadir(&mut locales, &p.nombre);
        }
        declaraciones(&f.cuerpo, &mut locales);
        // `var f = function g() { g() }`: dentro, `g` es la propia función.
        if let (true, Some(n)) = (es_expresion, &f.nombre) {
            anadir(&mut locales, n);
        }
        let conjunto = locales.iter().cloned().collect();
        self.out.insert(clave(f), Info { locales, capturadas: BTreeSet::new() });
        self.pila.push((clave(f), conjunto, f.flecha));
        for p in &f.params {
            if let Some(d) = &p.defecto {
                self.expr(d);
            }
        }
        self.sentencias(&f.cuerpo);
        self.pila.pop();
    }

    /// Un uso de `nombre`: si es de una función de fuera, esa función lo tiene capturado.
    fn referencia(&mut self, nombre: &str) {
        if nombre == "arguments" {
            self.declarar_arguments();
        }
        let Some(((_, propios, _), fuera)) = self.pila.split_last() else { return };
        if propios.contains(nombre) {
            return;
        }
        if let Some((k, _, _)) = fuera.iter().rev().find(|(_, nombres, _)| nombres.contains(nombre))
            && let Some(info) = self.out.get_mut(k)
        {
            info.capturadas.insert(String::from(nombre));
        }
    }

    /// `arguments` es una variable local de la función normal (no flecha) más cercana, que
    /// solo existe si se usa: así las demás funciones no pagan nada.
    fn declarar_arguments(&mut self) {
        let Some((k, nombres, _)) = self.pila.iter_mut().rev().find(|(_, _, flecha)| !flecha) else { return };
        if *k == 0 || nombres.contains("arguments") {
            return;
        }
        nombres.insert(String::from("arguments"));
        if let Some(info) = self.out.get_mut(k) {
            anadir(&mut info.locales, "arguments");
        }
    }

    fn sentencias(&mut self, v: &[Sent]) {
        for s in v {
            self.sentencia(s);
        }
    }

    fn sentencia(&mut self, s: &Sent) {
        match s {
            Sent::Declarar(_, lista) => lista.iter().filter_map(|(_, e)| e.as_ref()).for_each(|e| self.expr(e)),
            Sent::Funcion(f) => self.funcion(f, false),
            Sent::Expr(e) | Sent::Lanzar(e) | Sent::Retornar(Some(e)) => self.expr(e),
            Sent::Si(c, a, b) => {
                self.expr(c);
                self.sentencia(a);
                if let Some(b) = b {
                    self.sentencia(b);
                }
            }
            Sent::Mientras(c, cuerpo) | Sent::Hacer(cuerpo, c) => {
                self.expr(c);
                self.sentencia(cuerpo);
            }
            Sent::Para { inicio, cond, paso, cuerpo } => {
                if let Some(i) = inicio {
                    self.sentencia(i);
                }
                cond.iter().chain(paso.iter()).for_each(|e| self.expr(e));
                self.sentencia(cuerpo);
            }
            Sent::ParaEn { nombre, objeto, cuerpo, .. } => {
                self.referencia(nombre);
                self.expr(objeto);
                self.sentencia(cuerpo);
            }
            Sent::Intentar { cuerpo, captura, final_ } => {
                self.sentencias(cuerpo);
                if let Some((_, b)) = captura {
                    self.sentencias(b);
                }
                if let Some(f) = final_ {
                    self.sentencias(f);
                }
            }
            Sent::Elegir(x, casos) => {
                self.expr(x);
                for (prueba, cuerpo) in casos {
                    if let Some(p) = prueba {
                        self.expr(p);
                    }
                    self.sentencias(cuerpo);
                }
            }
            Sent::Bloque(b) => self.sentencias(b),
            Sent::Retornar(None) | Sent::Romper | Sent::Continuar | Sent::Vacio => {}
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident(n) => self.referencia(n),
            Expr::Funcion(f) => self.funcion(f, true),
            Expr::Arreglo(v) => v.iter().flatten().for_each(|x| self.expr(x)),
            Expr::Objeto(props) => {
                for (k, v) in props {
                    if let Clave::Calculada(k) = k {
                        self.expr(k);
                    }
                    self.expr(v);
                }
            }
            Expr::Unaria(_, a) | Expr::Miembro(a, _) => self.expr(a),
            Expr::Actualizar { destino, .. } => self.expr(destino),
            Expr::Binaria(_, a, b) | Expr::Logica(_, a, b) | Expr::Asignar(_, a, b) | Expr::Indice(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            Expr::Condicional(c, a, b) => {
                self.expr(c);
                self.expr(a);
                self.expr(b);
            }
            Expr::Llamar(f, args) | Expr::Nuevo(f, args) => {
                self.expr(f);
                args.iter().for_each(|a| self.expr(a));
            }
            Expr::Secuencia(v) => v.iter().for_each(|x| self.expr(x)),
            Expr::Numero(_) | Expr::Cadena(_) | Expr::Regex(..) | Expr::Bool(_) | Expr::Nulo | Expr::Este => {}
        }
    }
}
