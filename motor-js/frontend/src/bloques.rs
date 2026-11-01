//! Ámbitos de bloque: `let`, `const` y `catch (e)` que tapan una variable de fuera con el
//! mismo nombre.
//!
//! El compilador reparte las variables por función (`ambitos.rs`): dos `n` de la misma función
//! serían la misma. Esta pasada, antes de compilar, renombra la de dentro y sus usos dentro de su
//! bloque (como hace Babel con `let`):
//!
//! ```text
//! let n = 1; { const n = 2; f(n); } g(n);   →   let n = 1; { const n·1 = 2; f(n·1); } g(n);
//! ```
//!
//! El nombre nuevo lleva U+0001 (`n\u{1}1`): no choca con nada que se pueda escribir. Una
//! función de dentro que declara su propia `n` la tapa a su vez, y ahí no se renombra.
//!
//! Falta: una variable nueva por vuelta en `for (let i...)` cuando la capturan funciones (hoy
//! todas las vueltas comparten la misma).

use crate::ast::{Clave, Decl, Expr, Funcion, Sent};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Los ámbitos abiertos, de fuera hacia dentro: nombre → cómo se llama de verdad. `funcion`
/// marca dónde empieza la función actual (más allá, una colisión no importa).
struct Renombrar {
    ambitos: Vec<BTreeMap<String, String>>,
    funcion: usize,
    contador: usize,
}

/// Renombra las declaraciones de bloque que tapan a otra de la misma función.
pub fn renombrar(programa: &mut [Sent]) {
    let mut r = Renombrar { ambitos: Vec::new(), funcion: 0, contador: 0 };
    let mut nivel = BTreeMap::new();
    for n in nombres_de_funcion(programa, &[]) {
        nivel.insert(n.clone(), n);
    }
    r.ambitos.push(nivel);
    r.sentencias(programa);
}

/// Los nombres de nivel de función: parámetros, `var` (también dentro de bloques), funciones
/// declaradas, y los `let`/`const` del propio cuerpo.
fn nombres_de_funcion(cuerpo: &[Sent], params: &[String]) -> Vec<String> {
    let mut v: Vec<String> = params.to_vec();
    crate::ambitos::declaraciones_var(cuerpo, &mut v);
    for s in cuerpo {
        match s {
            Sent::Declarar(_, lista) => v.extend(lista.iter().map(|(n, _)| n.clone())),
            Sent::Funcion(f) => v.extend(f.nombre.clone()),
            _ => {}
        }
    }
    v
}

/// Los `let`/`const` declarados directamente en un bloque (sin entrar en otros bloques).
fn de_bloque(cuerpo: &[Sent]) -> Vec<String> {
    let mut v = Vec::new();
    for s in cuerpo {
        if let Sent::Declarar(Decl::Let | Decl::Const, lista) = s {
            v.extend(lista.iter().map(|(n, _)| n.clone()));
        }
    }
    v
}

impl Renombrar {
    fn buscar(&self, n: &str) -> Option<&String> {
        self.ambitos.iter().rev().find_map(|a| a.get(n))
    }

    /// Abre un ámbito de bloque con estos nombres; los que ya existen en la función se renombran.
    fn abrir(&mut self, nombres: Vec<String>) {
        let mut a = BTreeMap::new();
        for n in nombres {
            let choca = self.ambitos[self.funcion..].iter().any(|x| x.contains_key(&n));
            let nuevo = if choca {
                self.contador += 1;
                format!("{n}\u{1}{}", self.contador)
            } else {
                n.clone()
            };
            a.insert(n, nuevo);
        }
        self.ambitos.push(a);
    }

    fn nombre(&self, n: &mut String) {
        if let Some(nuevo) = self.buscar(n)
            && nuevo != n
        {
            *n = nuevo.clone();
        }
    }

    fn bloque(&mut self, cuerpo: &mut [Sent]) {
        self.abrir(de_bloque(cuerpo));
        self.sentencias(cuerpo);
        self.ambitos.pop();
    }

    fn sentencias(&mut self, v: &mut [Sent]) {
        for s in v {
            self.sentencia(s);
        }
    }

    fn sentencia(&mut self, s: &mut Sent) {
        match s {
            Sent::Declarar(_, lista) => {
                for (n, e) in lista {
                    if let Some(e) = e {
                        self.expr(e);
                    }
                    self.nombre(n);
                }
            }
            Sent::Funcion(f) => {
                if let Some(n) = &mut f.nombre {
                    self.nombre(n);
                }
                self.funcion(f);
            }
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
                let nombres = match inicio.as_deref() {
                    Some(Sent::Declarar(Decl::Let | Decl::Const, l)) => l.iter().map(|(n, _)| n.clone()).collect(),
                    _ => Vec::new(),
                };
                self.abrir(nombres);
                if let Some(i) = inicio {
                    self.sentencia(i);
                }
                cond.iter_mut().chain(paso.iter_mut()).for_each(|e| self.expr(e));
                self.sentencia(cuerpo);
                self.ambitos.pop();
            }
            Sent::ParaEn { decl, nombre, objeto, cuerpo, .. } => {
                self.expr(objeto);
                let propia = matches!(decl, Some(Decl::Let | Decl::Const));
                self.abrir(if propia { alloc::vec![nombre.clone()] } else { Vec::new() });
                self.nombre(nombre);
                self.sentencia(cuerpo);
                self.ambitos.pop();
            }
            Sent::Intentar { cuerpo, captura, final_ } => {
                self.bloque(cuerpo);
                if let Some((nombre, b)) = captura {
                    self.abrir(nombre.iter().cloned().collect());
                    if let Some(n) = nombre {
                        self.nombre(n);
                    }
                    self.bloque(b);
                    self.ambitos.pop();
                }
                if let Some(f) = final_ {
                    self.bloque(f);
                }
            }
            Sent::Elegir(x, casos) => {
                self.expr(x);
                let nombres = casos.iter().flat_map(|(_, c)| de_bloque(c)).collect();
                self.abrir(nombres);
                for (prueba, cuerpo) in casos {
                    if let Some(p) = prueba {
                        self.expr(p);
                    }
                    self.sentencias(cuerpo);
                }
                self.ambitos.pop();
            }
            Sent::Bloque(b) => self.bloque(b),
            Sent::Retornar(None) | Sent::Romper | Sent::Continuar | Sent::Vacio => {}
        }
    }

    /// Una función: sus propios nombres tapan a los de fuera (sin renombrar).
    fn funcion(&mut self, f: &mut Funcion) {
        let params: Vec<String> = f.params.iter().map(|p| p.nombre.clone()).collect();
        let mut nivel: BTreeMap<String, String> = BTreeMap::new();
        for n in nombres_de_funcion(&f.cuerpo, &params) {
            nivel.insert(n.clone(), n);
        }
        if let Some(n) = &f.nombre {
            nivel.entry(n.clone()).or_insert_with(|| n.clone());
        }
        if !f.flecha {
            nivel.insert(String::from("arguments"), String::from("arguments"));
        }
        let antes = core::mem::replace(&mut self.funcion, self.ambitos.len());
        self.ambitos.push(nivel);
        for p in &mut f.params {
            if let Some(d) = &mut p.defecto {
                self.expr(d);
            }
        }
        self.sentencias(&mut f.cuerpo);
        self.ambitos.pop();
        self.funcion = antes;
    }

    fn expr(&mut self, e: &mut Expr) {
        match e {
            Expr::Ident(n) => self.nombre(n),
            Expr::Funcion(f) => self.funcion(f),
            Expr::Arreglo(v) => v.iter_mut().flatten().for_each(|x| self.expr(x)),
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
                args.iter_mut().for_each(|a| self.expr(a));
            }
            Expr::Secuencia(v) => v.iter_mut().for_each(|x| self.expr(x)),
            Expr::Este | Expr::Numero(_) | Expr::Cadena(_) | Expr::Regex(..) | Expr::Bool(_) | Expr::Nulo => {}
        }
    }
}
