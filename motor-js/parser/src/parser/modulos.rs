//! Módulos (`import` / `export`), convertidos en una función que el cargador llama:
//!
//! ```text
//! __quimera_funcion = function (__exportar, __importar, __meta) {
//!   "use strict";
//!   return __quimera_async(function* () {
//!     const m1 = await __importar("./a.js"), x = m1.x;          // los import, primero
//!     Object.defineProperty(__exportar, "y", {get: () => y});   // los export, vivos
//!     ...el cuerpo...
//!   }, this, arguments);
//! };
//! ```
//!
//! - Es `async`: los módulos de los que depende se esperan en orden, y vale `await` fuera de
//!   una función (el «top-level await»).
//! - Lo importado se copia al importar (`x = m1.x`): si el otro módulo cambia `x` después, este
//!   no lo ve (en los navegadores sí). Lo exportado es un getter: quien importa lee el valor de
//!   ese momento.
//! - `import(x)` es `__importar(x)` dentro de un módulo y `__quimera_importar_dinamico(x)` fuera.

use super::Parser;
use super::azucar::{definir_accesor, llamar, miembro};
use super::clases::envolver_async;
use crate::ast::{Clave, Decl, Expr, Funcion, Param, Sent};
use crate::fuente::{Pos, Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Lo que va antes del cuerpo: las importaciones y las definiciones de lo exportado.
#[derive(Default)]
pub(super) struct Modulo {
    importaciones: Vec<Sent>,
    /// (nombre exportado, expresión que lo da).
    exportaciones: Vec<(String, Expr)>,
    /// `export * from`: módulos (temporales) cuyas exportaciones se reexportan.
    reexportar: Vec<String>,
}

/// Una función flecha sin parámetros que devuelve `e` (el getter de una exportación).
fn flecha(e: Expr, pos: Pos) -> Funcion {
    Funcion {
        nombre: None,
        params: Vec::new(),
        cuerpo: vec![Sent::Retornar(Some(e))],
        flecha: true,
        pos,
        generador: false,
    }
}

impl Parser {
    /// Un texto entre comillas (el origen de un `import`).
    fn cadena_modulo(&mut self) -> Resultado<String> {
        match self.avanzar().tipo {
            Tipo::Cadena(s) => Ok(s),
            otro => error(
                self.pos(),
                alloc::format!("se esperaba el nombre de un módulo y hay {}", super::describir(&otro)),
            ),
        }
    }

    /// Salta `with { type: "json" }` (o `assert {...}`) tras el origen.
    fn atributos_de_importacion(&mut self) -> Resultado<()> {
        if (self.es("with") || self.es_palabra("assert")) && self.es_en(1, "{") {
            self.avanzar();
            let fin = self.cierre_desde(0).unwrap_or(0);
            for _ in 0..=fin {
                self.avanzar();
            }
        }
        Ok(())
    }

    /// `const t = await __importar("origen")`; devuelve el temporal.
    fn importar_origen(&mut self, origen: String, m: &mut Modulo) -> String {
        let t = self.temporal();
        let pedir = llamar("__importar", vec![Expr::Cadena(origen)]);
        let valor = self.esperar_expr(pedir);
        m.importaciones.push(Sent::Declarar(Decl::Const, vec![(t.clone(), Some(valor))]));
        t
    }

    /// `{ a, b as c, default as d }`: (nombre en el otro módulo, nombre aquí).
    fn lista_de_nombres(&mut self) -> Resultado<Vec<(String, String)>> {
        self.esperar("{")?;
        let mut v = Vec::new();
        while !self.tomar("}") {
            let fuera = match self.avanzar().tipo {
                Tipo::Ident(n) | Tipo::Cadena(n) => n,
                Tipo::Clave(k) => String::from(k),
                otro => {
                    return error(
                        self.pos(),
                        alloc::format!("se esperaba un nombre y hay {}", super::describir(&otro)),
                    );
                }
            };
            let dentro = if self.es_palabra("as") {
                self.avanzar();
                match self.avanzar().tipo {
                    Tipo::Ident(n) | Tipo::Cadena(n) => n,
                    Tipo::Clave(k) => String::from(k),
                    otro => {
                        return error(
                            self.pos(),
                            alloc::format!("se esperaba un nombre y hay {}", super::describir(&otro)),
                        );
                    }
                }
            } else {
                fuera.clone()
            };
            v.push((fuera, dentro));
            if !self.es("}") {
                self.esperar(",")?;
            }
        }
        Ok(v)
    }

    /// `import ...;` (ya se sabe que no es `import(` ni `import.meta`).
    pub(super) fn importacion(&mut self, m: &mut Modulo) -> Resultado<()> {
        self.esperar("import")?;
        if let Tipo::Cadena(_) = self.ver() {
            let origen = self.cadena_modulo()?;
            self.atributos_de_importacion()?;
            self.fin_sentencia()?;
            self.importar_origen(origen, m);
            return Ok(());
        }
        let mut por_defecto = None;
        let mut espacio = None;
        let mut nombres = Vec::new();
        if matches!(self.ver(), Tipo::Ident(_)) && !self.es_palabra("from")
            || matches!(self.ver(), Tipo::Ident(n) if n == "from") && self.es_en(1, ",")
        {
            por_defecto = Some(self.ident()?);
            self.tomar(",");
        }
        if self.tomar("*") {
            if !self.es_palabra("as") {
                return error(self.pos(), "se esperaba `as` tras `import *`");
            }
            self.avanzar();
            espacio = Some(self.ident()?);
        } else if self.es("{") {
            nombres = self.lista_de_nombres()?;
        }
        if !self.es_palabra("from") {
            return error(self.pos(), "se esperaba `from` en el import");
        }
        self.avanzar();
        let origen = self.cadena_modulo()?;
        self.atributos_de_importacion()?;
        self.fin_sentencia()?;
        let t = self.importar_origen(origen, m);
        let mut lista = Vec::new();
        if let Some(d) = por_defecto {
            lista.push((d, Some(miembro(Expr::Ident(t.clone()), "default"))));
        }
        if let Some(n) = espacio {
            lista.push((n, Some(Expr::Ident(t.clone()))));
        }
        for (fuera, dentro) in nombres {
            lista.push((dentro, Some(Expr::Miembro(Box::new(Expr::Ident(t.clone())), fuera))));
        }
        if !lista.is_empty() {
            m.importaciones.push(Sent::Declarar(Decl::Const, lista));
        }
        Ok(())
    }

    /// `export ...`: devuelve la sentencia que queda en el cuerpo (si queda alguna).
    pub(super) fn exportacion(&mut self, m: &mut Modulo) -> Resultado<Option<Sent>> {
        self.esperar("export")?;
        if self.tomar("default") {
            return self.exportar_por_defecto(m).map(Some);
        }
        if self.tomar("*") {
            let nombre = if self.es_palabra("as") {
                self.avanzar();
                Some(self.ident()?)
            } else {
                None
            };
            if !self.es_palabra("from") {
                return error(self.pos(), "se esperaba `from` en `export *`");
            }
            self.avanzar();
            let origen = self.cadena_modulo()?;
            self.atributos_de_importacion()?;
            self.fin_sentencia()?;
            let t = self.importar_origen(origen, m);
            match nombre {
                Some(n) => m.exportaciones.push((n, Expr::Ident(t))),
                None => m.reexportar.push(t),
            }
            return Ok(None);
        }
        if self.es("{") {
            let nombres = self.lista_de_nombres()?;
            let desde = if self.es_palabra("from") {
                self.avanzar();
                let origen = self.cadena_modulo()?;
                self.atributos_de_importacion()?;
                Some(self.importar_origen(origen, m))
            } else {
                None
            };
            self.fin_sentencia()?;
            for (dentro, fuera) in nombres {
                let valor = match &desde {
                    Some(t) => Expr::Miembro(Box::new(Expr::Ident(t.clone())), dentro),
                    None => Expr::Ident(dentro),
                };
                m.exportaciones.push((fuera, valor));
            }
            return Ok(None);
        }
        // `export const/let/var`, `export function`, `export async function`, `export class`.
        let s = self.sentencia()?;
        let nombres: Vec<String> = match &s {
            Sent::Declarar(_, lista) => {
                lista.iter().map(|(n, _)| n.clone()).filter(|n| !n.starts_with('\u{1}')).collect()
            }
            Sent::Funcion(f) => f.nombre.iter().cloned().collect(),
            _ => return error(self.pos(), "export necesita una declaración"),
        };
        for n in nombres {
            m.exportaciones.push((n.clone(), Expr::Ident(n)));
        }
        Ok(Some(s))
    }

    /// `export default ...` (con `default` ya leído).
    fn exportar_por_defecto(&mut self, m: &mut Modulo) -> Resultado<Sent> {
        let con_nombre =
            (self.es("function") || self.es("class") || self.es_palabra("async") && self.es_en(1, "function"))
                && (matches!(self.token(1).tipo, Tipo::Ident(_))
                    || self.es_en(1, "*") && matches!(self.token(2).tipo, Tipo::Ident(_))
                    || self.es_en(1, "function"));
        if con_nombre {
            let s = self.sentencia()?;
            let nombre = match &s {
                Sent::Funcion(f) => f.nombre.clone(),
                Sent::Declarar(_, lista) => lista.first().map(|(n, _)| n.clone()),
                _ => None,
            };
            if let Some(n) = nombre {
                m.exportaciones.push((String::from("default"), Expr::Ident(n)));
                return Ok(s);
            }
        }
        let valor = self.asignacion()?;
        self.fin_sentencia()?;
        let t = self.temporal();
        m.exportaciones.push((String::from("default"), Expr::Ident(t.clone())));
        Ok(Sent::Declarar(Decl::Const, vec![(t, Some(valor))]))
    }
}

/// Analiza un módulo y lo convierte en `__quimera_funcion = function (__exportar, __importar,
/// __meta) {...}` (ver arriba).
pub fn analizar_modulo(fuente: &str) -> Resultado<Vec<Sent>> {
    let mut p = Parser::con_tokens(crate::lexico::tokens(fuente)?);
    p.modulo = true;
    p.contextos.push((false, true));
    p.abrir_temporales();
    let mut m = Modulo::default();
    let mut cuerpo = Vec::new();
    let pos = p.pos();
    while p.ver() != &Tipo::Fin {
        let es_import = p.es("import") && !p.es_en(1, "(") && !p.es_en(1, ".");
        if es_import {
            p.importacion(&mut m)?;
        } else if p.es("export") {
            if let Some(s) = p.exportacion(&mut m)? {
                cuerpo.push(s);
            }
        } else {
            cuerpo.push(p.sentencia()?);
        }
    }
    let mut todo = core::mem::take(&mut m.importaciones);
    for (nombre, valor) in m.exportaciones {
        let getter = flecha(valor, pos);
        todo.push(Sent::Expr(definir_accesor(
            Expr::Ident(String::from("__exportar")),
            Clave::Nombre(nombre),
            true,
            getter,
            true,
        )));
    }
    for t in m.reexportar {
        todo.push(Sent::Expr(llamar(
            "__quimera_reexportar",
            vec![Expr::Ident(String::from("__exportar")), Expr::Ident(t)],
        )));
    }
    todo.append(&mut cuerpo);
    p.cerrar_temporales(&mut todo);
    p.contextos.pop();
    let mut externo = vec![Sent::Expr(Expr::Cadena(String::from("use strict")))];
    externo.extend(envolver_async(todo, Expr::Ident(String::from("arguments")), pos));
    let params = ["__exportar", "__importar", "__meta"]
        .iter()
        .map(|n| Param { nombre: String::from(*n), defecto: None })
        .collect();
    let f = Funcion { nombre: None, params, cuerpo: externo, flecha: false, pos, generador: false };
    let asignar = Expr::Asignar(
        "=",
        Box::new(Expr::Ident(String::from("__quimera_funcion"))),
        Box::new(Expr::Funcion(Box::new(f))),
    );
    Ok(vec![Sent::Expr(asignar)])
}
