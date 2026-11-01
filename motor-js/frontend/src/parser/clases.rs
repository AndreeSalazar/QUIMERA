//! `class`, convertida en una función constructora y su prototipo (como hace Babel):
//!
//! ```text
//! class B extends A { x = 1; constructor(v) { super(v); } m() { return super.m(); } static s() {} }
//! ```
//! es, más o menos,
//! ```text
//! (() => {
//!   var padre = A, campos = function () { this.x = 1; };
//!   function B(v) { __quimera_super(padre, this, [v], campos); }
//!   __quimera_heredar(B, padre);
//!   B.prototype.m = function m() { return padre.prototype.m.call(this); };
//!   B.s = function s() {};
//!   __quimera_no_enumerar(B.prototype);
//!   return B;
//! })()
//! ```
//!
//! Falta: `get`/`set` (la máquina aún no tiene accesores), métodos `async` y generadores, bloques
//! `static {}`. Los campos `#privados` son propiedades normales con ese nombre.

use super::Parser;
use super::azucar::{Firma, Piezas, indefinido, lista_de, llamar, miembro, sin_esparcir};
use crate::ast::{Clave, Decl, Expr, Funcion, Sent};
use crate::fuente::{Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// La clase que se está analizando (para `super`).
pub(super) struct InfoClase {
    /// El temporal con la clase padre, si hay `extends`.
    pub padre: Option<String>,
    /// El temporal con la función que inicia los campos.
    pub campos: String,
    /// Dentro de un método `static`.
    pub estatico: bool,
}

/// Un método o campo de la clase.
enum Miembro {
    Metodo { estatico: bool, clave: Clave, funcion: Funcion },
    Campo { estatico: bool, clave: Clave, valor: Option<Expr> },
}

fn este_campo(clave: Clave, valor: Option<Expr>, dueno: Expr) -> Sent {
    let destino = match clave {
        Clave::Nombre(n) => Expr::Miembro(Box::new(dueno), n),
        Clave::Calculada(k) => Expr::Indice(Box::new(dueno), Box::new(k)),
    };
    Sent::Expr(Expr::Asignar("=", Box::new(destino), Box::new(valor.unwrap_or_else(indefinido))))
}

impl Parser {
    /// `class [Nombre] [extends Padre] { ... }`: el nombre y la expresión que crea la clase.
    pub(super) fn clase(&mut self) -> Resultado<(Option<String>, Expr)> {
        let pos = self.pos();
        self.esperar("class")?;
        let nombre = if matches!(self.ver(), Tipo::Ident(_)) { Some(self.ident()?) } else { None };
        self.abrir_temporales();
        let padre_expr = if self.tomar("extends") { Some(self.llamada_o_miembro()?) } else { None };
        let padre = padre_expr.as_ref().map(|_| self.temporal());
        let campos = self.temporal();
        self.clases.push(InfoClase { padre: padre.clone(), campos: campos.clone(), estatico: false });
        let miembros = self.cuerpo_de_clase();
        self.clases.pop();
        let mut miembros = miembros?;
        let interno = nombre.clone().unwrap_or_else(|| self.temporal());
        let mut cuerpo = Vec::new();
        if let (Some(p), Some(e)) = (&padre, padre_expr) {
            cuerpo.push(Sent::Declarar(Decl::Var, vec![(p.clone(), Some(e))]));
        }
        // Los campos de instancia: una función que los pone en `this`.
        let mut iniciar = Vec::new();
        miembros.retain(|m| match m {
            Miembro::Campo { estatico: false, clave, valor } => {
                iniciar.push(este_campo(clave.clone(), valor.clone(), Expr::Este));
                false
            }
            _ => true,
        });
        let hay_campos = !iniciar.is_empty();
        let f_campos = Funcion { nombre: None, params: Vec::new(), cuerpo: iniciar, flecha: false, pos };
        cuerpo.push(Sent::Declarar(Decl::Var, vec![(campos.clone(), Some(Expr::Funcion(Box::new(f_campos))))]));
        let constructor = self.constructor(&mut miembros, &padre, &campos, hay_campos, &interno, pos);
        cuerpo.push(Sent::Funcion(constructor));
        if let Some(p) = &padre {
            cuerpo.push(Sent::Expr(llamar(
                "__quimera_heredar",
                vec![Expr::Ident(interno.clone()), Expr::Ident(p.clone())],
            )));
        }
        for m in miembros {
            let (estatico, clave, valor) = match m {
                Miembro::Metodo { estatico, clave, funcion } => {
                    (estatico, clave, Some(Expr::Funcion(Box::new(funcion))))
                }
                Miembro::Campo { estatico, clave, valor } => (estatico, clave, valor),
            };
            let dueno = if estatico {
                Expr::Ident(interno.clone())
            } else {
                miembro(Expr::Ident(interno.clone()), "prototype")
            };
            cuerpo.push(este_campo(clave, valor, dueno));
        }
        cuerpo.push(Sent::Expr(llamar(
            "__quimera_no_enumerar",
            vec![miembro(Expr::Ident(interno.clone()), "prototype")],
        )));
        cuerpo.push(Sent::Retornar(Some(Expr::Ident(interno))));
        self.cerrar_temporales(&mut cuerpo);
        let iife = Funcion { nombre: None, params: Vec::new(), cuerpo, flecha: true, pos };
        Ok((nombre, Expr::Llamar(Box::new(Expr::Funcion(Box::new(iife))), Vec::new())))
    }

    /// La función constructora: la de `constructor(...)` o una por defecto.
    fn constructor(
        &mut self,
        miembros: &mut Vec<Miembro>,
        padre: &Option<String>,
        campos: &str,
        hay_campos: bool,
        interno: &str,
        pos: crate::fuente::Pos,
    ) -> Funcion {
        let i = miembros.iter().position(
            |m| matches!(m, Miembro::Metodo { estatico: false, clave: Clave::Nombre(n), .. } if n == "constructor"),
        );
        let mut f = match i.map(|i| miembros.remove(i)) {
            Some(Miembro::Metodo { funcion, .. }) => funcion,
            _ => {
                let cuerpo = match padre {
                    // `constructor(...args) { super(...args); }`
                    Some(p) => vec![Sent::Expr(llamar(
                        "__quimera_super",
                        vec![
                            Expr::Ident(p.clone()),
                            Expr::Este,
                            Expr::Ident(String::from("arguments")),
                            Expr::Ident(String::from(campos)),
                        ],
                    ))],
                    None => Vec::new(),
                };
                Funcion { nombre: None, params: Vec::new(), cuerpo, flecha: false, pos }
            }
        };
        if padre.is_none() && hay_campos {
            let llamada = Expr::Llamar(Box::new(miembro(Expr::Ident(String::from(campos)), "call")), vec![Expr::Este]);
            f.cuerpo.insert(0, Sent::Expr(llamada));
        }
        f.nombre = Some(String::from(interno));
        f
    }

    fn cuerpo_de_clase(&mut self) -> Resultado<Vec<Miembro>> {
        self.esperar("{")?;
        let mut miembros = Vec::new();
        while !self.tomar("}") {
            if self.tomar(";") {
                continue;
            }
            let pos = self.pos();
            let modificador = |p: &Self| !(p.es_en(1, "(") || p.es_en(1, "=") || p.es_en(1, ";") || p.es_en(1, "}"));
            let estatico = self.es_palabra("static") && modificador(self);
            if estatico {
                self.avanzar();
            }
            if (self.es_palabra("get") || self.es_palabra("set")) && modificador(self) {
                return error(pos, "get/set en clases todavía no están en QUIMERA (la máquina aún no tiene accesores)");
            }
            if (self.es_palabra("async") && modificador(self)) || self.es("*") {
                return error(pos, "los métodos async y los generadores todavía no están en QUIMERA");
            }
            if estatico && self.es("{") {
                return error(pos, "los bloques static {} todavía no están en QUIMERA");
            }
            let clave = self.clave_de_propiedad()?;
            if self.es("(") {
                if let Some(c) = self.clases.last_mut() {
                    c.estatico = estatico;
                }
                let nombre = if let Clave::Nombre(n) = &clave { Some(n.clone()) } else { None };
                let funcion = self.funcion_de_metodo(nombre, pos)?;
                miembros.push(Miembro::Metodo { estatico, clave, funcion });
            } else {
                let valor = if self.tomar("=") {
                    // Un campo se evalúa como si estuviera en un método (su `this` es la instancia).
                    self.abrir_temporales();
                    let v = self.asignacion();
                    let mut decl = Vec::new();
                    self.cerrar_temporales(&mut decl);
                    let v = v?;
                    Some(if decl.is_empty() { v } else { self.envolver_en_funcion(decl, v, pos) })
                } else {
                    None
                };
                self.fin_sentencia()?;
                miembros.push(Miembro::Campo { estatico, clave, valor });
            }
        }
        Ok(miembros)
    }

    /// Un valor que necesita temporales: `(function () { var t...; return v; }).call(this)`.
    fn envolver_en_funcion(&mut self, mut decl: Vec<Sent>, v: Expr, pos: crate::fuente::Pos) -> Expr {
        decl.push(Sent::Retornar(Some(v)));
        let f = Funcion { nombre: None, params: Vec::new(), cuerpo: decl, flecha: false, pos };
        Expr::Llamar(Box::new(miembro(Expr::Funcion(Box::new(f)), "call")), vec![Expr::Este])
    }

    /// Los parámetros y el cuerpo de un método (o de una función con nombre ya leído).
    pub(super) fn funcion_de_metodo(&mut self, nombre: Option<String>, pos: crate::fuente::Pos) -> Resultado<Funcion> {
        self.abrir_temporales();
        let r = self.firma().and_then(|f| Ok((f, self.bloque()?)));
        let (mut firma, mut cuerpo): (Firma, Vec<Sent>) = match r {
            Ok(x) => x,
            Err(e) => {
                self.temporales.pop();
                return Err(e);
            }
        };
        Self::prologo(&mut firma, &mut cuerpo);
        self.cerrar_temporales(&mut cuerpo);
        Ok(Funcion { nombre, params: firma.params, cuerpo, flecha: false, pos })
    }

    /// `super(...)`, `super.m(...)`, `super.x`, `super[k]` dentro de una clase con `extends`.
    pub(super) fn super_(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        self.esperar("super")?;
        let Some(info) = self.clases.last() else { return error(pos, "super fuera de una clase") };
        let Some(padre) = info.padre.clone() else { return error(pos, "super en una clase sin extends") };
        let (campos, estatico) = (info.campos.clone(), info.estatico);
        if self.es("(") {
            let piezas = self.piezas_entre("(", ")")?;
            let args = vec![Expr::Ident(padre), Expr::Este, lista_de(piezas), Expr::Ident(campos)];
            return Ok(llamar("__quimera_super", args));
        }
        let base = if estatico { Expr::Ident(padre) } else { miembro(Expr::Ident(padre), "prototype") };
        let objetivo = if self.tomar(".") {
            Expr::Miembro(Box::new(base), self.nombre_de_propiedad()?)
        } else {
            self.esperar("[")?;
            let k = self.expresion()?;
            self.esperar("]")?;
            Expr::Indice(Box::new(base), Box::new(k))
        };
        if !self.es("(") {
            return Ok(objetivo);
        }
        // Un método del padre con `this` = esta instancia.
        Ok(match sin_esparcir(self.piezas_entre("(", ")")?) {
            Ok(mut args) => {
                args.insert(0, Expr::Este);
                Expr::Llamar(Box::new(miembro(objetivo, "call")), args)
            }
            Err(piezas) => Expr::Llamar(Box::new(miembro(objetivo, "apply")), vec![Expr::Este, lista_de(piezas)]),
        })
    }

    /// Argumentos o elementos entre `abre` y `cierra`, con `...spread`.
    pub(super) fn piezas_entre(&mut self, abre: &str, cierra: &str) -> Resultado<Piezas> {
        self.esperar(abre)?;
        let mut p = Piezas { exprs: Vec::new(), esparcidas: Vec::new() };
        while !self.tomar(cierra) {
            if self.tomar("...") {
                p.esparcidas.push(p.exprs.len());
            }
            p.exprs.push(self.asignacion()?);
            if !self.es(cierra) {
                self.esperar(",")?;
            }
        }
        Ok(p)
    }
}
