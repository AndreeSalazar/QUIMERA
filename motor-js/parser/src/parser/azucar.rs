//! Sintaxis moderna convertida en la que el compilador ya entiende (como hace Babel), sin tocar
//! el compilador ni la máquina:
//!
//! - **Desestructuración**: `let {a, b: [c] = []} = o` → `let t = o, a = t.a, u = t.b, ...`.
//! - **Parámetros** desestructurados y `...rest`: un nombre temporal y un prólogo en el cuerpo.
//! - **`...spread`** en llamadas, `new`, arreglos y objetos: `concat`, `apply` y
//!   `Object.assign`, con ayudantes del preludio (`__quimera_lista`, `__quimera_metodo`...).
//!
//! Los temporales se llaman `\u{1}t3`: desde JavaScript no se pueden escribir, y las ranuras con
//! U+0001 no salen en `for-in`.

use super::Parser;
use crate::ast::{Clave, Decl, Expr, Param, Sent};
use crate::fuente::{Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Un patrón de desestructuración.
#[derive(Debug, Clone)]
pub(super) enum Patron {
    Nombre(String),
    /// En una asignación (`[this.a, o[k]] = ...`): cualquier destino.
    Destino(Expr),
    /// `{clave: patrón = defecto, ...resto}`.
    Objeto(Vec<(Clave, Patron, Option<Expr>)>, Option<String>),
    /// `[patrón = defecto, , ...resto]` (un hueco es `None`).
    Arreglo(Vec<Option<(Patron, Option<Expr>)>>, Option<Box<Patron>>),
}

/// Argumentos o elementos, y cuáles llevan `...` delante. Casi nunca hay ninguno: así se leen
/// directamente en su lista final, sin copiarlos (el parser es la parte caliente del frontend).
pub(super) struct Piezas {
    pub exprs: Vec<Expr>,
    pub esparcidas: Vec<usize>,
}

/// Los parámetros de una función, y lo que su cuerpo debe hacer al empezar.
pub(super) struct Firma {
    pub params: Vec<Param>,
    /// Las desestructuraciones de los parámetros.
    pub prologo: Vec<Sent>,
    /// `...nombre`, y cuántos parámetros hay antes.
    pub resto: Option<(String, usize)>,
}

/// `void 0`.
pub(super) fn indefinido() -> Expr {
    Expr::Unaria("void", Box::new(Expr::Numero(0.0)))
}

pub(super) fn llamar(f: &str, args: Vec<Expr>) -> Expr {
    Expr::Llamar(Box::new(Expr::Ident(String::from(f))), args)
}

pub(super) fn miembro(e: Expr, nombre: &str) -> Expr {
    Expr::Miembro(Box::new(e), String::from(nombre))
}

/// `[a, b, ...c, d]` como una expresión que da el arreglo: `[a, b].concat(lista(c), [d])`.
pub(super) fn lista_de(piezas: Piezas) -> Expr {
    let mut segmentos: Vec<Expr> = Vec::new();
    let mut normales: Vec<Option<Expr>> = Vec::new();
    for (i, e) in piezas.exprs.into_iter().enumerate() {
        if piezas.esparcidas.contains(&i) {
            if !normales.is_empty() || segmentos.is_empty() {
                segmentos.push(Expr::Arreglo(core::mem::take(&mut normales)));
            }
            segmentos.push(llamar("__quimera_lista", vec![e]));
        } else {
            normales.push(Some(e));
        }
    }
    if !normales.is_empty() {
        segmentos.push(Expr::Arreglo(normales));
    }
    let mut it = segmentos.into_iter();
    let primero = it.next().unwrap_or(Expr::Arreglo(Vec::new()));
    let resto: Vec<Expr> = it.collect();
    if resto.is_empty() { primero } else { Expr::Llamar(Box::new(miembro(primero, "concat")), resto) }
}

/// Una llamada con `...spread`: el `this` de `o.m(...)` se conserva.
pub(super) fn llamada_esparcida(f: Expr, piezas: Piezas) -> Expr {
    let lista = lista_de(piezas);
    match f {
        Expr::Miembro(o, n) => llamar("__quimera_metodo", vec![*o, Expr::Cadena(n), lista]),
        Expr::Indice(o, k) => llamar("__quimera_metodo", vec![*o, *k, lista]),
        f => Expr::Llamar(Box::new(miembro(f, "apply")), vec![indefinido(), lista]),
    }
}

/// Las piezas sin `...spread`, si no hay ninguna.
pub(super) fn sin_esparcir(piezas: Piezas) -> Result<Vec<Expr>, Piezas> {
    if piezas.esparcidas.is_empty() { Ok(piezas.exprs) } else { Err(piezas) }
}

/// `{a: 1, ...o, b}` → `Object.assign({a: 1}, o, {b: b})`.
pub(super) fn objeto_esparcido(segmentos: Vec<Result<Vec<(Clave, Expr)>, Expr>>) -> Expr {
    let mut args = Vec::new();
    for s in segmentos {
        match s {
            Ok(props) => args.push(Expr::Objeto(props)),
            Err(e) => {
                if args.is_empty() {
                    args.push(Expr::Objeto(Vec::new()));
                }
                args.push(e);
            }
        }
    }
    Expr::Llamar(Box::new(miembro(Expr::Ident(String::from("Object")), "assign")), args)
}

/// Las parejas de una desestructuración como declaración (todos sus destinos son nombres).
pub(super) fn nombres(pares: Vec<(Expr, Expr)>) -> Vec<(String, Option<Expr>)> {
    pares
        .into_iter()
        .filter_map(|(d, e)| match d {
            Expr::Ident(n) => Some((n, Some(e))),
            _ => None,
        })
        .collect()
}

/// Un patrón de asignación a partir de lo ya leído como expresión (`[a, b]`, `{a, b: c}`).
pub(super) fn expr_a_patron(e: Expr) -> Option<Patron> {
    let con_defecto = |e: Expr| match e {
        Expr::Asignar("=", d, v) => Some((expr_a_patron(*d)?, Some(*v))),
        otro => Some((expr_a_patron(otro)?, None)),
    };
    Some(match e {
        Expr::Ident(n) => Patron::Nombre(n),
        Expr::Miembro(..) | Expr::Indice(..) => Patron::Destino(e),
        Expr::Arreglo(v) => {
            let mut elementos = Vec::new();
            for x in v {
                elementos.push(match x {
                    None => None,
                    Some(x) => Some(con_defecto(x)?),
                });
            }
            Patron::Arreglo(elementos, None)
        }
        Expr::Objeto(props) => {
            let mut v = Vec::new();
            for (k, x) in props {
                let (p, d) = con_defecto(x)?;
                v.push((k, p, d));
            }
            Patron::Objeto(v, None)
        }
        // `{a, ...resto} = o`: el objeto con `...` ya se leyó como `Object.assign({a}, resto)`.
        Expr::Llamar(f, mut args) if es_metodo(&f, "Object", "assign") && args.len() >= 2 => {
            let Some(Expr::Ident(resto)) = args.pop() else { return None };
            let mut v = Vec::new();
            for a in args {
                let Expr::Objeto(props) = a else { return None };
                for (k, x) in props {
                    let (p, d) = con_defecto(x)?;
                    v.push((k, p, d));
                }
            }
            Patron::Objeto(v, Some(resto))
        }
        // `[a, ...resto] = x`: leído como `[a].concat(__quimera_lista(resto))`.
        Expr::Llamar(f, mut args) if args.len() == 1 => {
            let Expr::Miembro(base, metodo) = *f else { return None };
            let (Expr::Arreglo(v), "concat") = (*base, metodo.as_str()) else { return None };
            let Some(Expr::Llamar(g, mut dentro)) = args.pop() else { return None };
            if !matches!(&*g, Expr::Ident(n) if n == "__quimera_lista") || dentro.len() != 1 {
                return None;
            }
            let resto = expr_a_patron(dentro.pop()?)?;
            let mut elementos = Vec::new();
            for x in v {
                elementos.push(match x {
                    None => None,
                    Some(x) => Some(con_defecto(x)?),
                });
            }
            Patron::Arreglo(elementos, Some(Box::new(resto)))
        }
        _ => return None,
    })
}

/// Si `f` es `objeto.metodo` (con `objeto` un nombre).
fn es_metodo(f: &Expr, objeto: &str, metodo: &str) -> bool {
    matches!(f, Expr::Miembro(o, m) if m == metodo && matches!(&**o, Expr::Ident(n) if n == objeto))
}

/// `Object.defineProperty(dueno, clave, {get: f})` (o `set`): un accesor de un objeto o clase.
/// Con `enumerable`, añade `enumerable: true` (objetos literales y exportaciones; en las clases
/// no lo son, como en los navegadores).
pub(super) fn definir_accesor(
    dueno: Expr,
    clave: Clave,
    es_get: bool,
    f: crate::ast::Funcion,
    enumerable: bool,
) -> Expr {
    let k = match clave {
        Clave::Nombre(n) => Expr::Cadena(n),
        Clave::Calculada(e) => e,
    };
    let campo = String::from(if es_get { "get" } else { "set" });
    let mut campos = vec![(Clave::Nombre(campo), Expr::Funcion(Box::new(f)))];
    if enumerable {
        campos.push((Clave::Nombre(String::from("enumerable")), Expr::Bool(true)));
    }
    let desc = Expr::Objeto(campos);
    let define = miembro(Expr::Ident(String::from("Object")), "defineProperty");
    Expr::Llamar(Box::new(define), vec![dueno, k, desc])
}

/// `valor === undefined ? defecto : valor`.
fn con_defecto(valor: Expr, defecto: Option<Expr>) -> Expr {
    match defecto {
        None => valor,
        Some(d) => Expr::Condicional(
            Box::new(Expr::Binaria("===", Box::new(valor.clone()), Box::new(indefinido()))),
            Box::new(d),
            Box::new(valor),
        ),
    }
}

impl Parser {
    /// Un nombre temporal nuevo (lo declara quien lo usa).
    pub(super) fn temporal(&mut self) -> String {
        self.contador_temporales += 1;
        format!("\u{1}t{}", self.contador_temporales)
    }

    /// Un temporal declarado con `var` al principio de la función actual.
    pub(super) fn temporal_var(&mut self) -> String {
        let t = self.temporal();
        if let Some(marco) = self.temporales.last_mut() {
            marco.push(t.clone());
        }
        t
    }

    /// Empieza el cuerpo de una función (sus temporales `var` irán al principio).
    pub(super) fn abrir_temporales(&mut self) {
        self.temporales.push(Vec::new());
    }

    /// Termina el cuerpo: declara sus temporales al principio.
    pub(super) fn cerrar_temporales(&mut self, cuerpo: &mut Vec<Sent>) {
        if let Some(t) = self.temporales.pop().filter(|t| !t.is_empty()) {
            cuerpo.insert(0, Sent::Declarar(Decl::Var, t.into_iter().map(|n| (n, None)).collect()));
        }
    }

    /// Un patrón: un nombre, `{...}` o `[...]`.
    pub(super) fn patron(&mut self) -> Resultado<Patron> {
        if self.tomar("[") {
            let mut elementos = Vec::new();
            let mut resto = None;
            while !self.tomar("]") {
                if self.tomar(",") {
                    elementos.push(None);
                    continue;
                }
                if self.tomar("...") {
                    resto = Some(Box::new(self.patron()?));
                    self.esperar("]")?;
                    break;
                }
                let p = self.patron()?;
                let d = if self.tomar("=") { Some(self.asignacion()?) } else { None };
                elementos.push(Some((p, d)));
                if !self.es("]") {
                    self.esperar(",")?;
                }
            }
            return Ok(Patron::Arreglo(elementos, resto));
        }
        if self.tomar("{") {
            let mut props = Vec::new();
            let mut resto = None;
            while !self.tomar("}") {
                if self.tomar("...") {
                    resto = Some(self.ident()?);
                    self.esperar("}")?;
                    break;
                }
                let pos = self.pos();
                let abreviable = matches!(self.ver(), Tipo::Ident(_));
                let clave = self.clave_de_propiedad()?;
                let p = if self.tomar(":") {
                    self.patron()?
                } else {
                    match (&clave, abreviable) {
                        (Clave::Nombre(n), true) => Patron::Nombre(n.clone()),
                        _ => return error(pos, "se esperaba `:` en el patrón"),
                    }
                };
                let d = if self.tomar("=") { Some(self.asignacion()?) } else { None };
                props.push((clave, p, d));
                if !self.es("}") {
                    self.esperar(",")?;
                }
            }
            return Ok(Patron::Objeto(props, resto));
        }
        Ok(Patron::Nombre(self.ident()?))
    }

    /// Las asignaciones (nombre, valor) que hace `patrón = fuente`, en orden. `fuente` debe
    /// poder leerse varias veces (un nombre). Con `inline`, los temporales van en la propia lista
    /// (declaraciones); si no, se declaran con `var` en la función.
    pub(super) fn desplegar(&mut self, p: Patron, fuente: Expr, inline: bool, salida: &mut Vec<(Expr, Expr)>) {
        let sub = |yo: &mut Self, p: Patron, valor: Expr, salida: &mut Vec<(Expr, Expr)>| match p {
            Patron::Nombre(n) => salida.push((Expr::Ident(n), valor)),
            Patron::Destino(d) => salida.push((d, valor)),
            otro => {
                let t = if inline { yo.temporal() } else { yo.temporal_var() };
                salida.push((Expr::Ident(t.clone()), valor));
                yo.desplegar(otro, Expr::Ident(t), inline, salida);
            }
        };
        match p {
            Patron::Nombre(n) => salida.push((Expr::Ident(n), fuente)),
            Patron::Destino(d) => salida.push((d, fuente)),
            Patron::Objeto(props, resto) => {
                let mut usadas = Vec::new();
                for (clave, p, d) in props {
                    let valor = match clave {
                        Clave::Nombre(n) => {
                            usadas.push(Expr::Cadena(n.clone()));
                            Expr::Miembro(Box::new(fuente.clone()), n)
                        }
                        Clave::Calculada(k) => Expr::Indice(Box::new(fuente.clone()), Box::new(k)),
                    };
                    sub(self, p, con_defecto(valor, d), salida);
                }
                if let Some(r) = resto {
                    let usadas = Expr::Arreglo(usadas.into_iter().map(Some).collect());
                    salida.push((Expr::Ident(r), llamar("__quimera_resto", vec![fuente, usadas])));
                }
            }
            Patron::Arreglo(elementos, resto) => {
                let t = if inline { self.temporal() } else { self.temporal_var() };
                salida.push((Expr::Ident(t.clone()), llamar("__quimera_lista", vec![fuente])));
                let n = elementos.len();
                for (i, e) in elementos.into_iter().enumerate() {
                    if let Some((p, d)) = e {
                        let valor = Expr::Indice(Box::new(Expr::Ident(t.clone())), Box::new(Expr::Numero(i as f64)));
                        sub(self, p, con_defecto(valor, d), salida);
                    }
                }
                if let Some(r) = resto {
                    let valor = Expr::Llamar(Box::new(miembro(Expr::Ident(t), "slice")), vec![Expr::Numero(n as f64)]);
                    sub(self, *r, valor, salida);
                }
            }
        }
    }

    /// `patrón = valor` como expresión (`[a, b] = [b, a]`): asignaciones en secuencia, y vale `valor`.
    pub(super) fn asignar_patron(&mut self, p: Patron, valor: Expr) -> Expr {
        let t = self.temporal_var();
        let mut pares = vec![(Expr::Ident(t.clone()), valor)];
        self.desplegar(p, Expr::Ident(t.clone()), false, &mut pares);
        let mut v: Vec<Expr> = pares.into_iter().map(|(d, e)| Expr::Asignar("=", Box::new(d), Box::new(e))).collect();
        v.push(Expr::Ident(t));
        Expr::Secuencia(v)
    }

    /// `(a, {b}, ...c)`: los parámetros, con los desestructurados como temporales.
    pub(super) fn firma(&mut self) -> Resultado<Firma> {
        self.esperar("(")?;
        let mut f = Firma { params: Vec::new(), prologo: Vec::new(), resto: None };
        while !self.tomar(")") {
            if self.tomar("...") {
                let nombre = self.ident()?;
                f.resto = Some((nombre, f.params.len()));
                self.esperar(")")?;
                break;
            }
            let p = self.patron()?;
            let defecto = if self.tomar("=") { Some(self.asignacion()?) } else { None };
            let nombre = match p {
                Patron::Nombre(n) => n,
                otro => {
                    let t = self.temporal();
                    let mut pares = Vec::new();
                    self.desplegar(otro, Expr::Ident(t.clone()), true, &mut pares);
                    f.prologo.push(Sent::Declarar(Decl::Var, nombres(pares)));
                    t
                }
            };
            f.params.push(Param { nombre, defecto });
            if !self.es(")") {
                self.esperar(",")?;
            }
        }
        Ok(f)
    }

    /// Pone al principio del cuerpo el `...rest` (con `arguments`) y las desestructuraciones.
    pub(super) fn prologo(f: &mut Firma, cuerpo: &mut Vec<Sent>) {
        let mut antes = Vec::new();
        if let Some((nombre, i)) = f.resto.take() {
            let slice = miembro(miembro(miembro(Expr::Ident(String::from("Array")), "prototype"), "slice"), "call");
            let valor =
                Expr::Llamar(Box::new(slice), vec![Expr::Ident(String::from("arguments")), Expr::Numero(i as f64)]);
            antes.push(Sent::Declarar(Decl::Var, vec![(nombre, Some(valor))]));
        }
        antes.append(&mut f.prologo);
        if !antes.is_empty() {
            cuerpo.splice(0..0, antes);
        }
    }

    /// Una clave de propiedad: nombre, palabra, cadena, número o `[calculada]`.
    pub(super) fn clave_de_propiedad(&mut self) -> Resultado<Clave> {
        let pos = self.pos();
        Ok(match self.avanzar().tipo {
            Tipo::Ident(n) | Tipo::Cadena(n) => Clave::Nombre(n),
            Tipo::Clave(k) => Clave::Nombre(String::from(k)),
            Tipo::Numero(n) => Clave::Nombre(super::numero_a_clave(n)),
            Tipo::Signo("#") => Clave::Nombre(format!("#{}", self.ident()?)),
            Tipo::Signo("[") => {
                let e = self.asignacion()?;
                self.esperar("]")?;
                Clave::Calculada(e)
            }
            otro => return error(pos, format!("se esperaba una clave de propiedad y hay {}", super::describir(&otro))),
        })
    }
}
