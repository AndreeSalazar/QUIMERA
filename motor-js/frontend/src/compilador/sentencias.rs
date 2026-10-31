//! Compilación de sentencias: declaraciones, saltos y bucles.

use super::{Bucle, Compilador, Destino, segura_sobre_variable};
use crate::ast::{Caso, Decl, Expr, Sent};
use crate::bytecode::{Instr, OpBin};
use crate::fuente::{Pos, Resultado, error};

impl Compilador<'_> {
    pub(super) fn sentencia(&mut self, s: &Sent) -> Resultado<()> {
        let marca = self.marca();
        match s {
            Sent::Declarar(decl, lista) => {
                for (nombre, valor) in lista {
                    match valor {
                        Some(v) => self.asignar_variable(nombre, v)?,
                        // `let x;` dentro de un bucle vuelve a valer undefined en cada vuelta.
                        None if *decl != Decl::Var => {
                            let t = self.temporal()?;
                            self.emitir(Instr::Indefinido { dst: t });
                            self.guardar(nombre, t);
                        }
                        None => {}
                    }
                }
            }
            Sent::Funcion(_) | Sent::Vacio => {} // las funciones ya se izaron
            Sent::Expr(e) => self.expr_descartada(e)?,
            Sent::Si(c, a, b) => self.si(c, a, b.as_deref())?,
            Sent::Mientras(c, cuerpo) => self.mientras(c, cuerpo)?,
            Sent::Hacer(cuerpo, c) => self.hacer(cuerpo, c)?,
            Sent::Para { inicio, cond, paso, cuerpo } => {
                self.para(inicio.as_deref(), cond.as_ref(), paso.as_ref(), cuerpo)?
            }
            Sent::ParaEn { .. } => return error(Pos::default(), "for-in y for-of todavía no se compilan en QUIMERA"),
            Sent::Retornar(v) => {
                let r = match v {
                    Some(e) => self.operando(e)?,
                    None => {
                        let t = self.temporal()?;
                        self.emitir(Instr::Indefinido { dst: t });
                        t
                    }
                };
                self.emitir(Instr::Retornar { src: r });
            }
            Sent::Romper | Sent::Continuar => self.romper_o_continuar(matches!(s, Sent::Romper))?,
            Sent::Lanzar(e) => {
                let r = self.operando(e)?;
                self.emitir(Instr::Lanzar { src: r });
            }
            Sent::Intentar { cuerpo, captura, final_ } => self.intentar(cuerpo, captura.as_ref(), final_.is_some())?,
            Sent::Elegir(x, casos) => self.elegir(x, casos)?,
            Sent::Bloque(b) => {
                for s in b {
                    self.sentencia(s)?;
                }
            }
        }
        self.soltar(marca);
        Ok(())
    }

    /// `nombre = valor` al declarar; en una variable de registro se compila en su sitio.
    pub(super) fn asignar_variable(&mut self, nombre: &str, valor: &Expr) -> Resultado<()> {
        if let Destino::Local(r) = self.resolver(nombre)
            && segura_sobre_variable(valor)
        {
            return self.expr(valor, r);
        }
        let t = self.temporal()?;
        self.expr(valor, t)?;
        self.guardar(nombre, t);
        Ok(())
    }

    /// Una expresión cuyo resultado no se usa (`x++;`, `x += 1;`, `f();`): sobre una variable
    /// de registro se trabaja en su registro, sin copiar el resultado a ningún sitio.
    fn expr_descartada(&mut self, e: &Expr) -> Resultado<()> {
        let local = |c: &mut Self, d: &Expr| match d {
            Expr::Ident(n) => match c.resolver(n) {
                Destino::Local(r) => Some(r),
                _ => None,
            },
            _ => None,
        };
        match e {
            Expr::Asignar(op, destino, valor) => {
                if let (true, Expr::Ident(n)) = (*op == "=", destino.as_ref()) {
                    return self.asignar_variable(n, valor);
                }
                if let Some(r) = local(self, destino) {
                    return self.expr(e, r);
                }
            }
            // `x++;` y `++x;` son lo mismo si nadie usa el valor.
            Expr::Actualizar { op, destino, .. } => {
                if let Some(r) = local(self, destino) {
                    let prefija = Expr::Actualizar { op, prefijo: true, destino: destino.clone() };
                    return self.expr(&prefija, r);
                }
            }
            _ => {}
        }
        let t = self.temporal()?;
        self.expr(e, t)
    }

    /// Salta (destino pendiente) si la condición es falsa. `a < b` y demás comparaciones
    /// van en una sola instrucción que compara y salta.
    fn saltar_si_falso(&mut self, c: &Expr) -> Resultado<usize> {
        if let Expr::Binaria(op, a, b) = c {
            let comparacion = matches!(*op, "<" | "<=" | ">" | ">=" | "===" | "!==" | "==" | "!=");
            if let (true, Some(op)) = (comparacion, OpBin::desde(op)) {
                let (ra, rb) = (self.operando(a)?, self.operando(b)?);
                return Ok(self.emitir(Instr::SaltarSiNoCompara { op, a: ra, b: rb, destino: 0 }));
            }
        }
        let rc = self.operando(c)?;
        Ok(self.emitir(Instr::SaltarSiNo { c: rc, destino: 0 }))
    }

    fn si(&mut self, c: &Expr, a: &Sent, b: Option<&Sent>) -> Resultado<()> {
        let a_otro = self.saltar_si_falso(c)?;
        self.sentencia(a)?;
        match b {
            Some(b) => {
                let a_fin = self.emitir(Instr::Saltar { destino: 0 });
                let aqui = self.pc();
                self.parchear(a_otro, aqui);
                self.sentencia(b)?;
                let fin = self.pc();
                self.parchear(a_fin, fin);
            }
            None => {
                let fin = self.pc();
                self.parchear(a_otro, fin);
            }
        }
        Ok(())
    }

    fn abrir_bucle(&mut self, es_switch: bool) {
        self.ctx().bucles.push(Bucle { es_switch, ..Bucle::default() });
    }

    /// Cierra el bucle actual: los `break` van a `fin` y los `continue` a `seguir`.
    fn cerrar_bucle(&mut self, seguir: u32, fin: u32) {
        let Some(b) = self.ctx().bucles.pop() else { return };
        for i in b.romper {
            self.parchear(i, fin);
        }
        for i in b.continuar {
            self.parchear(i, seguir);
        }
    }

    fn mientras(&mut self, c: &Expr, cuerpo: &Sent) -> Resultado<()> {
        let inicio = self.pc();
        let a_fin = self.saltar_si_falso(c)?;
        self.abrir_bucle(false);
        self.sentencia(cuerpo)?;
        self.emitir(Instr::Saltar { destino: inicio });
        let fin = self.pc();
        self.parchear(a_fin, fin);
        self.cerrar_bucle(inicio, fin);
        Ok(())
    }

    fn hacer(&mut self, cuerpo: &Sent, c: &Expr) -> Resultado<()> {
        let inicio = self.pc();
        self.abrir_bucle(false);
        self.sentencia(cuerpo)?;
        let seguir = self.pc();
        let rc = self.operando(c)?;
        self.emitir(Instr::SaltarSi { c: rc, destino: inicio });
        let fin = self.pc();
        self.cerrar_bucle(seguir, fin);
        Ok(())
    }

    fn para(
        &mut self,
        inicio: Option<&Sent>,
        cond: Option<&Expr>,
        paso: Option<&Expr>,
        cuerpo: &Sent,
    ) -> Resultado<()> {
        if let Some(i) = inicio {
            self.sentencia(i)?;
        }
        let vuelta = self.pc();
        let a_fin = match cond {
            Some(c) => {
                let marca = self.marca();
                let salto = self.saltar_si_falso(c)?;
                self.soltar(marca);
                Some(salto)
            }
            None => None,
        };
        self.abrir_bucle(false);
        self.sentencia(cuerpo)?;
        let seguir = self.pc();
        if let Some(p) = paso {
            let marca = self.marca();
            self.expr_descartada(p)?;
            self.soltar(marca);
        }
        self.emitir(Instr::Saltar { destino: vuelta });
        let fin = self.pc();
        if let Some(s) = a_fin {
            self.parchear(s, fin);
        }
        self.cerrar_bucle(seguir, fin);
        Ok(())
    }

    fn romper_o_continuar(&mut self, romper: bool) -> Resultado<()> {
        let salto = self.emitir(Instr::Saltar { destino: 0 });
        let bucles = &mut self.ctx().bucles;
        // `continue` salta al bucle más cercano que no sea un switch.
        let bucle = if romper { bucles.last_mut() } else { bucles.iter_mut().rev().find(|b| !b.es_switch) };
        match bucle {
            Some(b) if romper => b.romper.push(salto),
            Some(b) => b.continuar.push(salto),
            None => {
                let que = if romper { "break" } else { "continue" };
                return error(Pos::default(), alloc::format!("{que} fuera de un bucle"));
            }
        }
        Ok(())
    }

    fn intentar(
        &mut self,
        cuerpo: &[Sent],
        captura: Option<&(Option<alloc::string::String>, alloc::vec::Vec<Sent>)>,
        con_final: bool,
    ) -> Resultado<()> {
        if con_final {
            return error(Pos::default(), "finally todavía no se compila en QUIMERA");
        }
        let proteger = self.emitir(Instr::Intentar { captura: 0 });
        for s in cuerpo {
            self.sentencia(s)?;
        }
        self.emitir(Instr::FinIntentar);
        let a_fin = self.emitir(Instr::Saltar { destino: 0 });
        let en_captura = self.pc();
        self.parchear(proteger, en_captura);
        if let Some((nombre, bloque)) = captura {
            let t = self.temporal()?;
            self.emitir(Instr::Excepcion { dst: t });
            if let Some(n) = nombre {
                self.guardar(n, t);
            }
            for s in bloque {
                self.sentencia(s)?;
            }
        }
        let fin = self.pc();
        self.parchear(a_fin, fin);
        Ok(())
    }

    /// `switch`: primero las comparaciones (`===`), luego los cuerpos seguidos (un caso sin
    /// `break` sigue en el siguiente, como en JavaScript).
    fn elegir(&mut self, x: &Expr, casos: &[Caso]) -> Resultado<()> {
        let rx = self.temporal()?;
        self.expr(x, rx)?;
        let mut saltos = alloc::vec::Vec::new();
        for (i, (prueba, _)) in casos.iter().enumerate() {
            let Some(p) = prueba else { continue };
            let marca = self.marca();
            let rp = self.temporal()?;
            self.expr(p, rp)?;
            self.emitir(Instr::Binaria { op: OpBin::Estricto, dst: rp, a: rx, b: rp });
            saltos.push((i, self.emitir(Instr::SaltarSi { c: rp, destino: 0 })));
            self.soltar(marca);
        }
        let a_defecto = self.emitir(Instr::Saltar { destino: 0 });
        self.abrir_bucle(true);
        let mut defecto = None;
        let mut inicios = alloc::vec::Vec::new();
        for (prueba, cuerpo) in casos {
            let aqui = self.pc();
            inicios.push(aqui);
            if prueba.is_none() {
                defecto = Some(aqui);
            }
            for s in cuerpo {
                self.sentencia(s)?;
            }
        }
        let fin = self.pc();
        for (i, salto) in saltos {
            self.parchear(salto, inicios[i]);
        }
        self.parchear(a_defecto, defecto.unwrap_or(fin));
        self.cerrar_bucle(fin, fin);
        Ok(())
    }
}
