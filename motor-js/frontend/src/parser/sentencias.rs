//! Sentencias: declaraciones, control de flujo, funciones, `try` y `switch`.

use super::Parser;
use crate::ast::{Caso, Decl, Expr, Funcion, Param, Sent};
use crate::fuente::{Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::vec::Vec;

impl Parser {
    pub(super) fn sentencia(&mut self) -> Resultado<Sent> {
        let pos = self.pos();
        let (Tipo::Clave(k) | Tipo::Signo(k)) = *self.ver() else { return self.sentencia_expresion() };
        match k {
            "{" => Ok(Sent::Bloque(self.bloque()?)),
            ";" => {
                self.avanzar();
                Ok(Sent::Vacio)
            }
            "var" | "let" | "const" => {
                let s = self.declaracion()?;
                self.fin_sentencia()?;
                Ok(s)
            }
            "function" => Ok(Sent::Funcion(self.funcion(true)?)),
            "if" => self.si(),
            "while" => self.mientras(),
            "do" => self.hacer(),
            "for" => self.para(),
            "return" | "throw" => self.retornar_o_lanzar(k),
            "break" | "continue" => {
                self.avanzar();
                if matches!(self.ver(), Tipo::Ident(_)) && !self.token(0).salto_antes {
                    return error(self.pos(), "las etiquetas en break/continue todavía no están en QUIMERA");
                }
                self.fin_sentencia()?;
                Ok(if k == "break" { Sent::Romper } else { Sent::Continuar })
            }
            "try" => self.intentar(),
            "switch" => self.elegir(),
            "class" | "import" | "export" | "with" | "yield" => {
                error(pos, alloc::format!("`{k}` todavía no está en QUIMERA"))
            }
            _ => self.sentencia_expresion(),
        }
    }

    fn sentencia_expresion(&mut self) -> Resultado<Sent> {
        if matches!(self.ver(), Tipo::Ident(_)) && self.es_en(1, ":") {
            return error(self.pos(), "las etiquetas todavía no están en QUIMERA");
        }
        let e = self.expresion()?;
        self.fin_sentencia()?;
        Ok(Sent::Expr(e))
    }

    pub(super) fn bloque(&mut self) -> Resultado<Vec<Sent>> {
        self.esperar("{")?;
        let mut v = Vec::new();
        while !self.es("}") {
            if self.ver() == &Tipo::Fin {
                return error(self.pos(), "falta `}` al final del bloque");
            }
            v.push(self.sentencia()?);
        }
        self.avanzar();
        Ok(v)
    }

    /// `var a = 1, b` (sin el `;`).
    fn declaracion(&mut self) -> Resultado<Sent> {
        let decl = match self.avanzar().tipo {
            Tipo::Clave("let") => Decl::Let,
            Tipo::Clave("const") => Decl::Const,
            _ => Decl::Var,
        };
        let mut lista = Vec::new();
        loop {
            if self.es("[") || self.es("{") {
                return error(self.pos(), "la desestructuración todavía no está en QUIMERA");
            }
            let nombre = self.ident()?;
            let valor = if self.tomar("=") { Some(self.asignacion()?) } else { None };
            if valor.is_none() && decl == Decl::Const && !self.es_palabra("of") && !self.es("in") {
                return error(self.pos(), alloc::format!("const {nombre} necesita un valor"));
            }
            lista.push((nombre, valor));
            if !self.tomar(",") {
                return Ok(Sent::Declarar(decl, lista));
            }
        }
    }

    /// `function nombre(params) { cuerpo }`; el nombre es obligatorio en una declaración.
    pub(super) fn funcion(&mut self, declaracion: bool) -> Resultado<Funcion> {
        let pos = self.pos();
        self.esperar("function")?;
        if self.es("*") {
            return error(self.pos(), "los generadores (function*) todavía no están en QUIMERA");
        }
        let nombre = if matches!(self.ver(), Tipo::Ident(_)) { Some(self.ident()?) } else { None };
        if declaracion && nombre.is_none() {
            return error(self.pos(), "una declaración de función necesita nombre");
        }
        let params = self.parametros()?;
        let cuerpo = self.bloque()?;
        Ok(Funcion { nombre, params, cuerpo, flecha: false, pos })
    }

    /// `(a, b = 1)`.
    pub(super) fn parametros(&mut self) -> Resultado<Vec<Param>> {
        self.esperar("(")?;
        let mut v = Vec::new();
        while !self.tomar(")") {
            if self.es("...") || self.es("[") || self.es("{") {
                return error(self.pos(), "los parámetros ...rest y desestructurados todavía no están en QUIMERA");
            }
            let nombre = self.ident()?;
            let defecto = if self.tomar("=") { Some(self.asignacion()?) } else { None };
            v.push(Param { nombre, defecto });
            if !self.es(")") {
                self.esperar(",")?;
            }
        }
        Ok(v)
    }

    fn condicion(&mut self) -> Resultado<Expr> {
        self.esperar("(")?;
        let c = self.expresion()?;
        self.esperar(")")?;
        Ok(c)
    }

    fn si(&mut self) -> Resultado<Sent> {
        self.avanzar();
        let c = self.condicion()?;
        let a = Box::new(self.sentencia()?);
        let b = if self.tomar("else") { Some(Box::new(self.sentencia()?)) } else { None };
        Ok(Sent::Si(c, a, b))
    }

    fn mientras(&mut self) -> Resultado<Sent> {
        self.avanzar();
        let c = self.condicion()?;
        Ok(Sent::Mientras(c, Box::new(self.sentencia()?)))
    }

    fn hacer(&mut self) -> Resultado<Sent> {
        self.avanzar();
        let cuerpo = Box::new(self.sentencia()?);
        self.esperar("while")?;
        let c = self.condicion()?;
        self.tomar(";"); // tras do-while el `;` siempre puede faltar
        Ok(Sent::Hacer(cuerpo, c))
    }

    fn para(&mut self) -> Resultado<Sent> {
        self.avanzar();
        if self.es_palabra("await") {
            return error(self.pos(), "for await todavía no está en QUIMERA");
        }
        self.esperar("(")?;
        self.sin_in = true;
        let inicio = if self.es(";") {
            None
        } else if self.es("var") || self.es("let") || self.es("const") {
            Some(self.declaracion()?)
        } else {
            Some(Sent::Expr(self.expresion()?))
        };
        self.sin_in = false;
        if self.es("in") || self.es_palabra("of") {
            return self.para_en(inicio);
        }
        self.esperar(";")?;
        let cond = if self.es(";") { None } else { Some(self.expresion()?) };
        self.esperar(";")?;
        let paso = if self.es(")") { None } else { Some(self.expresion()?) };
        self.esperar(")")?;
        let cuerpo = Box::new(self.sentencia()?);
        Ok(Sent::Para { inicio: inicio.map(Box::new), cond, paso, cuerpo })
    }

    /// El resto de `for (x in o)` / `for (const x of l)`.
    fn para_en(&mut self, inicio: Option<Sent>) -> Resultado<Sent> {
        let pos = self.pos();
        let de = !self.es("in");
        self.avanzar();
        let (decl, nombre) = match inicio {
            Some(Sent::Declarar(d, mut l)) if l.len() == 1 && l[0].1.is_none() => (Some(d), l.remove(0).0),
            Some(Sent::Expr(Expr::Ident(n))) => (None, n),
            _ => return error(pos, "el lado izquierdo de for-in/for-of debe ser un nombre"),
        };
        let objeto = if de { self.asignacion()? } else { self.expresion()? };
        self.esperar(")")?;
        let cuerpo = Box::new(self.sentencia()?);
        Ok(Sent::ParaEn { decl, nombre, de, objeto, cuerpo })
    }

    /// `return x` / `throw x`: si hay un salto de línea justo después, `return` va sin valor.
    fn retornar_o_lanzar(&mut self, k: &str) -> Resultado<Sent> {
        self.avanzar();
        if k == "throw" {
            if self.token(0).salto_antes {
                return error(self.pos(), "no puede haber un salto de línea entre throw y su valor");
            }
            let e = self.expresion()?;
            self.fin_sentencia()?;
            return Ok(Sent::Lanzar(e));
        }
        let sin_valor = self.es(";") || self.es("}") || self.ver() == &Tipo::Fin || self.token(0).salto_antes;
        let valor = if sin_valor { None } else { Some(self.expresion()?) };
        self.fin_sentencia()?;
        Ok(Sent::Retornar(valor))
    }

    fn intentar(&mut self) -> Resultado<Sent> {
        let pos = self.pos();
        self.avanzar();
        let cuerpo = self.bloque()?;
        let captura = if self.tomar("catch") {
            let nombre = if self.tomar("(") {
                let n = self.ident()?;
                self.esperar(")")?;
                Some(n)
            } else {
                None
            };
            Some((nombre, self.bloque()?))
        } else {
            None
        };
        let final_ = if self.tomar("finally") { Some(self.bloque()?) } else { None };
        if captura.is_none() && final_.is_none() {
            return error(pos, "try necesita catch o finally");
        }
        Ok(Sent::Intentar { cuerpo, captura, final_ })
    }

    fn elegir(&mut self) -> Resultado<Sent> {
        self.avanzar();
        let x = self.condicion()?;
        self.esperar("{")?;
        let mut casos: Vec<Caso> = Vec::new();
        while !self.tomar("}") {
            let prueba = if self.tomar("default") {
                None
            } else {
                self.esperar("case")?;
                Some(self.expresion()?)
            };
            self.esperar(":")?;
            let mut cuerpo = Vec::new();
            while !self.es("case") && !self.es("default") && !self.es("}") {
                if self.ver() == &Tipo::Fin {
                    return error(self.pos(), "falta `}` al final del switch");
                }
                cuerpo.push(self.sentencia()?);
            }
            casos.push((prueba, cuerpo));
        }
        Ok(Sent::Elegir(x, casos))
    }
}
