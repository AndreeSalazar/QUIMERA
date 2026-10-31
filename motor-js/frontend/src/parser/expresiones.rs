//! Expresiones, de la de menor a la de mayor precedencia.

use super::{Parser, describir, numero_a_clave};
use crate::ast::{Clave, Expr, Funcion, Param, Sent};
use crate::fuente::{Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const ASIGNACIONES: &[&str] =
    &["=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", ">>>=", "&=", "|=", "^=", "&&=", "||=", "??="];

/// Precedencia de un operador binario (mayor = se agrupa antes) y si asocia a la derecha.
fn precedencia(op: &str) -> Option<(u8, bool)> {
    Some(match op {
        "??" => (1, false),
        "||" => (2, false),
        "&&" => (3, false),
        "|" => (4, false),
        "^" => (5, false),
        "&" => (6, false),
        "==" | "!=" | "===" | "!==" => (7, false),
        "<" | ">" | "<=" | ">=" | "instanceof" | "in" => (8, false),
        "<<" | ">>" | ">>>" => (9, false),
        "+" | "-" => (10, false),
        "*" | "/" | "%" => (11, false),
        "**" => (12, true),
        _ => return None,
    })
}

fn es_destino(e: &Expr) -> bool {
    matches!(e, Expr::Ident(_) | Expr::Miembro(..) | Expr::Indice(..))
}

impl Parser {
    /// `a, b, c`.
    pub(super) fn expresion(&mut self) -> Resultado<Expr> {
        let primera = self.asignacion()?;
        if !self.es(",") {
            return Ok(primera);
        }
        let mut v = vec![primera];
        while self.tomar(",") {
            v.push(self.asignacion()?);
        }
        Ok(Expr::Secuencia(v))
    }

    /// `a = b`, `a += b`, una flecha o una condicional.
    pub(super) fn asignacion(&mut self) -> Resultado<Expr> {
        if let Some(f) = self.flecha()? {
            return Ok(Expr::Funcion(Box::new(f)));
        }
        let pos = self.pos();
        let izquierda = self.condicional()?;
        let op = match self.ver() {
            Tipo::Signo(s) if ASIGNACIONES.contains(s) => *s,
            _ => return Ok(izquierda),
        };
        if !es_destino(&izquierda) {
            return error(pos, "no se puede asignar a esto");
        }
        self.avanzar();
        let derecha = self.asignacion()?;
        Ok(Expr::Asignar(op, Box::new(izquierda), Box::new(derecha)))
    }

    /// Si empieza una flecha (`x =>`, `(a, b) =>`), la analiza.
    fn flecha(&mut self) -> Resultado<Option<Funcion>> {
        let pos = self.pos();
        let params = if matches!(self.ver(), Tipo::Ident(_)) && self.es_en(1, "=>") {
            vec![Param { nombre: self.ident()?, defecto: None }]
        } else if self.es("(") && self.cierre_desde(0).is_some_and(|c| self.es_en(c + 1, "=>")) {
            self.parametros()?
        } else {
            return Ok(None);
        };
        if self.token(0).salto_antes {
            return error(self.pos(), "no puede haber un salto de línea antes de =>");
        }
        self.esperar("=>")?;
        let cuerpo = if self.es("{") {
            self.bloque()?
        } else {
            let sin_in = core::mem::replace(&mut self.sin_in, false);
            let e = self.asignacion();
            self.sin_in = sin_in;
            vec![Sent::Retornar(Some(e?))]
        };
        Ok(Some(Funcion { nombre: None, params, cuerpo, flecha: true, pos }))
    }

    fn condicional(&mut self) -> Resultado<Expr> {
        let c = self.binaria(0)?;
        if !self.tomar("?") {
            return Ok(c);
        }
        let sin_in = core::mem::replace(&mut self.sin_in, false);
        let a = self.asignacion();
        self.sin_in = sin_in;
        let a = a?;
        self.esperar(":")?;
        let b = self.asignacion()?;
        Ok(Expr::Condicional(Box::new(c), Box::new(a), Box::new(b)))
    }

    /// Operadores binarios por precedencia (todos los de precedencia mayor que `minima`).
    fn binaria(&mut self, minima: u8) -> Resultado<Expr> {
        let mut izquierda = self.unaria()?;
        loop {
            let op = match self.ver() {
                Tipo::Signo(s) => *s,
                Tipo::Clave(k @ ("instanceof" | "in")) if !(self.sin_in && *k == "in") => *k,
                _ => return Ok(izquierda),
            };
            let Some((prec, derecha_asoc)) = precedencia(op) else { return Ok(izquierda) };
            if prec <= minima {
                return Ok(izquierda);
            }
            self.avanzar();
            let siguiente = if derecha_asoc { prec - 1 } else { prec };
            let derecha = self.binaria(siguiente)?;
            izquierda = match op {
                "&&" | "||" | "??" => Expr::Logica(op, Box::new(izquierda), Box::new(derecha)),
                _ => Expr::Binaria(op, Box::new(izquierda), Box::new(derecha)),
            };
        }
    }

    fn unaria(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        let op = match self.ver() {
            Tipo::Signo(s @ ("!" | "~" | "+" | "-" | "++" | "--")) => *s,
            Tipo::Clave(k @ ("typeof" | "void" | "delete")) => *k,
            Tipo::Ident(n) if n == "await" => return error(pos, "await todavía no está en QUIMERA"),
            _ => return self.postfija(),
        };
        self.avanzar();
        let e = self.unaria()?;
        if op == "++" || op == "--" {
            if !es_destino(&e) {
                return error(pos, format!("{op} necesita una variable o una propiedad"));
            }
            return Ok(Expr::Actualizar { op, prefijo: true, destino: Box::new(e) });
        }
        if self.es("**") {
            return error(self.pos(), "una unaria delante de ** necesita paréntesis: (-a) ** b");
        }
        Ok(Expr::Unaria(op, Box::new(e)))
    }

    fn postfija(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        let e = self.llamada_o_miembro()?;
        if (self.es("++") || self.es("--")) && !self.token(0).salto_antes {
            if !es_destino(&e) {
                return error(pos, "++/-- necesita una variable o una propiedad");
            }
            let op = if self.es("++") { "++" } else { "--" };
            self.avanzar();
            return Ok(Expr::Actualizar { op, prefijo: false, destino: Box::new(e) });
        }
        Ok(e)
    }

    /// `new F(x)`, `a.b`, `a[b]`, `f(x)`, encadenados.
    fn llamada_o_miembro(&mut self) -> Resultado<Expr> {
        let mut e = if self.es("new") { self.nuevo()? } else { self.primaria()? };
        loop {
            if self.tomar(".") {
                e = Expr::Miembro(Box::new(e), self.nombre_de_propiedad()?);
            } else if self.tomar("[") {
                let i = self.expresion()?;
                self.esperar("]")?;
                e = Expr::Indice(Box::new(e), Box::new(i));
            } else if self.es("(") {
                e = Expr::Llamar(Box::new(e), self.argumentos()?);
            } else if self.es("?.") {
                return error(self.pos(), "el encadenamiento opcional (?.) todavía no está en QUIMERA");
            } else {
                return Ok(e);
            }
        }
    }

    /// `new F(args)` o `new F` (sin paréntesis no hay argumentos).
    fn nuevo(&mut self) -> Resultado<Expr> {
        self.avanzar();
        if self.es(".") {
            return error(self.pos(), "new.target todavía no está en QUIMERA");
        }
        let mut f = if self.es("new") { self.nuevo()? } else { self.primaria()? };
        loop {
            if self.tomar(".") {
                f = Expr::Miembro(Box::new(f), self.nombre_de_propiedad()?);
            } else if self.tomar("[") {
                let i = self.expresion()?;
                self.esperar("]")?;
                f = Expr::Indice(Box::new(f), Box::new(i));
            } else {
                break;
            }
        }
        let args = if self.es("(") { self.argumentos()? } else { Vec::new() };
        Ok(Expr::Nuevo(Box::new(f), args))
    }

    fn argumentos(&mut self) -> Resultado<Vec<Expr>> {
        self.esperar("(")?;
        let mut v = Vec::new();
        while !self.tomar(")") {
            if self.es("...") {
                return error(self.pos(), "los argumentos ...spread todavía no están en QUIMERA");
            }
            v.push(self.asignacion()?);
            if !self.es(")") {
                self.esperar(",")?;
            }
        }
        Ok(v)
    }

    /// Tras `.`: cualquier nombre, también palabras reservadas (`a.default`, `a.new`).
    fn nombre_de_propiedad(&mut self) -> Resultado<String> {
        match self.avanzar().tipo {
            Tipo::Ident(n) => Ok(n),
            Tipo::Clave(k) => Ok(String::from(k)),
            otro => error(self.pos(), format!("se esperaba un nombre de propiedad y hay {}", describir(&otro))),
        }
    }

    fn primaria(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        let e = match self.ver().clone() {
            Tipo::Numero(n) => Expr::Numero(n),
            Tipo::Cadena(s) => Expr::Cadena(s),
            Tipo::Ident(n) => Expr::Ident(n),
            Tipo::Clave("true") => Expr::Bool(true),
            Tipo::Clave("false") => Expr::Bool(false),
            Tipo::Clave("null") => Expr::Nulo,
            Tipo::Clave("this") => Expr::Este,
            Tipo::Clave("function") => return Ok(Expr::Funcion(Box::new(self.funcion(false)?))),
            Tipo::Signo("(") => return self.parentesis(),
            Tipo::Signo("[") => return self.arreglo(),
            Tipo::Signo("{") => return self.objeto(),
            Tipo::Signo("/" | "/=") => return error(pos, "las expresiones regulares todavía no están en QUIMERA"),
            Tipo::Clave(k @ ("class" | "super" | "import" | "yield")) => {
                return error(pos, format!("`{k}` todavía no está en QUIMERA"));
            }
            otro => return error(pos, format!("se esperaba una expresión y hay {}", describir(&otro))),
        };
        self.avanzar();
        Ok(e)
    }

    fn parentesis(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let sin_in = core::mem::replace(&mut self.sin_in, false);
        let e = self.expresion();
        self.sin_in = sin_in;
        let e = e?;
        self.esperar(")")?;
        Ok(e)
    }

    fn arreglo(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let mut v = Vec::new();
        while !self.tomar("]") {
            if self.tomar(",") {
                v.push(None);
                continue;
            }
            if self.es("...") {
                return error(self.pos(), "...spread en arreglos todavía no está en QUIMERA");
            }
            v.push(Some(self.asignacion()?));
            if !self.es("]") {
                self.esperar(",")?;
            }
        }
        Ok(Expr::Arreglo(v))
    }

    fn objeto(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let mut props = Vec::new();
        while !self.tomar("}") {
            props.push(self.propiedad()?);
            if !self.es("}") {
                self.esperar(",")?;
            }
        }
        Ok(Expr::Objeto(props))
    }

    /// `a: 1`, `a` (abreviada), `[k]: 1`, `"a": 1`, `1: x` o un método `f() {}`.
    fn propiedad(&mut self) -> Resultado<(Clave, Expr)> {
        let pos = self.pos();
        if self.es("...") {
            return error(pos, "...spread en objetos todavía no está en QUIMERA");
        }
        let accesor = (self.es_palabra("get") || self.es_palabra("set")) && !self.es_en(1, ":") && !self.es_en(1, "(");
        let asincrona = self.es_palabra("async") && !self.es_en(1, ":") && !self.es_en(1, "(");
        if accesor || asincrona || self.es("*") {
            return error(pos, "get/set, async y * en objetos todavía no están en QUIMERA");
        }
        let abreviable = matches!(self.ver(), Tipo::Ident(_));
        let clave = match self.avanzar().tipo {
            Tipo::Ident(n) | Tipo::Cadena(n) => Clave::Nombre(n),
            Tipo::Clave(k) => Clave::Nombre(String::from(k)),
            Tipo::Numero(n) => Clave::Nombre(numero_a_clave(n)),
            Tipo::Signo("[") => {
                let e = self.asignacion()?;
                self.esperar("]")?;
                Clave::Calculada(e)
            }
            otro => return error(pos, format!("se esperaba una clave de propiedad y hay {}", describir(&otro))),
        };
        if self.es("(") {
            let params = self.parametros()?;
            let cuerpo = self.bloque()?;
            let nombre = if let Clave::Nombre(n) = &clave { Some(n.clone()) } else { None };
            return Ok((clave, Expr::Funcion(Box::new(Funcion { nombre, params, cuerpo, flecha: false, pos }))));
        }
        if self.tomar(":") {
            return Ok((clave, self.asignacion()?));
        }
        match (&clave, abreviable) {
            (Clave::Nombre(n), true) => {
                let valor = Expr::Ident(n.clone());
                Ok((clave, valor))
            }
            _ => error(self.pos(), "se esperaba `:` tras la clave"),
        }
    }
}
