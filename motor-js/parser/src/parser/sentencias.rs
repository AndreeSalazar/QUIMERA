//! Sentencias: declaraciones, control de flujo, funciones, `try` y `switch`.

use super::Parser;
use super::azucar::{llamar, miembro, nombres};
use crate::ast::{Caso, Decl, Expr, Funcion, Sent};
use crate::fuente::{Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::vec::Vec;

impl Parser {
    pub(super) fn sentencia(&mut self) -> Resultado<Sent> {
        let pos = self.pos();
        if self.es_palabra("async") && self.es_en(1, "function") && !self.token(1).salto_antes {
            self.avanzar();
            return Ok(Sent::Funcion(self.funcion_asincrona_o_no(true, true)?));
        }
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
                    let etiqueta = self.ident()?;
                    self.fin_sentencia()?;
                    return Ok(if k == "break" {
                        Sent::RomperEtiqueta(etiqueta)
                    } else {
                        Sent::ContinuarEtiqueta(etiqueta)
                    });
                }
                self.fin_sentencia()?;
                Ok(if k == "break" { Sent::Romper } else { Sent::Continuar })
            }
            "try" => self.intentar(),
            "switch" => self.elegir(),
            "class" => {
                let (nombre, e) = self.clase()?;
                let Some(nombre) = nombre else { return error(pos, "una declaración de clase necesita nombre") };
                Ok(Sent::Declarar(Decl::Let, alloc::vec![(nombre, Some(e))]))
            }
            "yield" if self.contextos.last().is_some_and(|c| c.0) => self.sentencia_expresion(),
            "import" if self.es_en(1, "(") || self.es_en(1, ".") => self.sentencia_expresion(),
            "import" | "export" => error(pos, alloc::format!("`{k}` solo vale en un módulo (<script type=module>)")),
            "with" | "yield" => error(pos, alloc::format!("`{k}` todavía no está en QUIMERA")),
            _ => self.sentencia_expresion(),
        }
    }

    fn sentencia_expresion(&mut self) -> Resultado<Sent> {
        if matches!(self.ver(), Tipo::Ident(_)) && self.es_en(1, ":") {
            let etiqueta = self.ident()?;
            self.avanzar();
            return Ok(Sent::Etiquetada(etiqueta, Box::new(self.sentencia()?)));
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
                let p = self.patron()?;
                if !self.es("=") && (self.es_palabra("of") || self.es("in")) {
                    // `for (const [a, b] of x)`: el patrón se despliega dentro del cuerpo.
                    self.patron_pendiente = Some(p);
                    lista.push((self.temporal(), None));
                    return Ok(Sent::Declarar(decl, lista));
                }
                self.esperar("=")?;
                let valor = self.asignacion()?;
                let t = self.temporal();
                lista.push((t.clone(), Some(valor)));
                let mut pares = Vec::new();
                self.desplegar(p, Expr::Ident(t), true, &mut pares);
                lista.extend(nombres(pares));
                if !self.tomar(",") {
                    return Ok(Sent::Declarar(decl, lista));
                }
                continue;
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
        self.funcion_asincrona_o_no(declaracion, false)
    }

    /// `function [*] nombre(...) {...}`, con `async` ya leído si `asincrona`.
    pub(super) fn funcion_asincrona_o_no(&mut self, declaracion: bool, asincrona: bool) -> Resultado<Funcion> {
        let pos = self.pos();
        self.esperar("function")?;
        let generador = self.tomar("*");
        let nombre = if matches!(self.ver(), Tipo::Ident(_)) { Some(self.ident()?) } else { None };
        if declaracion && nombre.is_none() {
            return error(self.pos(), "una declaración de función necesita nombre");
        }
        self.funcion_con(nombre, pos, generador, asincrona)
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
        let esperar = self.es_palabra("await") && self.contextos.last().is_some_and(|c| c.1);
        if esperar {
            self.avanzar();
        }
        if esperar {
            return self.para_await();
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

    /// `for await (const x of fuente) cuerpo` (con `for await` ya leído), como
    /// `{ let t = __quimera_iterador_async(fuente); while (true) { let r = await t.next();
    /// if (r.done) break; const x = r.value; cuerpo } }`.
    fn para_await(&mut self) -> Resultado<Sent> {
        let pos = self.pos();
        self.esperar("(")?;
        self.sin_in = true;
        let inicio = if self.es("var") || self.es("let") || self.es("const") {
            Some(self.declaracion()?)
        } else {
            Some(Sent::Expr(self.expresion()?))
        };
        self.sin_in = false;
        if !self.es_palabra("of") {
            return error(pos, "for await necesita `of`");
        }
        let Sent::ParaEn { decl, nombre, objeto, cuerpo, .. } = self.para_en(inicio)? else {
            return error(pos, "for await mal formado");
        };
        let (t, r) = (self.temporal(), self.temporal());
        let siguiente = Expr::Llamar(Box::new(miembro(Expr::Ident(t.clone()), "next")), Vec::new());
        let paso = self.esperar_expr(siguiente);
        let valor = miembro(Expr::Ident(r.clone()), "value");
        let asignar = match decl {
            Some(d) => Sent::Declarar(d, alloc::vec![(nombre, Some(valor))]),
            None => Sent::Expr(Expr::Asignar("=", Box::new(Expr::Ident(nombre)), Box::new(valor))),
        };
        let vuelta = Sent::Bloque(alloc::vec![
            Sent::Declarar(Decl::Let, alloc::vec![(r.clone(), Some(paso))]),
            Sent::Si(miembro(Expr::Ident(r), "done"), Box::new(Sent::Romper), None),
            asignar,
            *cuerpo,
        ]);
        let fuente = llamar("__quimera_iterador_async", alloc::vec![objeto]);
        Ok(Sent::Bloque(alloc::vec![
            Sent::Declarar(Decl::Let, alloc::vec![(t, Some(fuente))]),
            Sent::Mientras(Expr::Bool(true), Box::new(vuelta)),
        ]))
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
        let patron = self.patron_pendiente.take();
        let objeto = if de { self.asignacion()? } else { self.expresion()? };
        self.esperar(")")?;
        let mut cuerpo = Box::new(self.sentencia()?);
        if let Some(p) = patron {
            let mut pares = Vec::new();
            self.desplegar(p, Expr::Ident(nombre.clone()), true, &mut pares);
            let d = Sent::Declarar(decl.unwrap_or(Decl::Let), nombres(pares));
            cuerpo = Box::new(Sent::Bloque(alloc::vec![d, *cuerpo]));
        }
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
