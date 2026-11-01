//! Compilación de sentencias: declaraciones, saltos y bucles.

use super::{Bucle, Compilador, Destino, segura_sobre_variable};
use crate::ast::{Caso, Decl, Expr, Sent};
use crate::bytecode::{Instr, OpBin};
use crate::fuente::Resultado;

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
            Sent::ParaEn { decl, nombre, de, objeto, cuerpo } => self.para_en(*decl, nombre, *de, objeto, cuerpo)?,
            Sent::Retornar(v) => self.retornar(v.as_ref())?,
            Sent::Romper | Sent::Continuar => self.romper_o_continuar(matches!(s, Sent::Romper))?,
            Sent::Lanzar(e) => {
                let r = self.operando(e)?;
                self.emitir(Instr::Lanzar { src: r });
            }
            Sent::Intentar { cuerpo, captura, final_ } => self.intentar(cuerpo, captura.as_ref(), final_.as_ref())?,
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

    /// Salta a `destino` si la condición es cierta (la de un bucle rotado, al final de la vuelta).
    fn saltar_si_cierto(&mut self, c: &Expr, destino: u32) -> Resultado<()> {
        if let Expr::Binaria(op, a, b) = c {
            let comparacion = matches!(*op, "<" | "<=" | ">" | ">=" | "===" | "!==" | "==" | "!=");
            if let (true, Some(op)) = (comparacion, OpBin::desde(op)) {
                let (ra, rb) = (self.operando(a)?, self.operando(b)?);
                self.emitir(Instr::SaltarSiCompara { op, a: ra, b: rb, destino });
                return Ok(());
            }
        }
        let rc = self.operando(c)?;
        self.emitir(Instr::SaltarSi { c: rc, destino });
        Ok(())
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

    pub(super) fn abrir_bucle(&mut self, es_switch: bool) {
        let regiones = self.ctx().regiones.len();
        self.ctx().bucles.push(Bucle { es_switch, regiones, ..Bucle::default() });
    }

    /// Cierra el bucle actual: los `break` van a `fin` y los `continue` a `seguir`.
    pub(super) fn cerrar_bucle(&mut self, seguir: u32, fin: u32) {
        let Some(b) = self.ctx().bucles.pop() else { return };
        for i in b.romper {
            self.parchear(i, fin);
        }
        for i in b.continuar {
            self.parchear(i, seguir);
        }
    }

    /// `while` rotado: se salta a la condición, que está al final y vuelve al cuerpo si se
    /// cumple. Cada vuelta es una instrucción más corta que con la condición arriba.
    fn mientras(&mut self, c: &Expr, cuerpo: &Sent) -> Resultado<()> {
        let a_cond = self.emitir(Instr::Saltar { destino: 0 });
        let inicio = self.pc();
        self.abrir_bucle(false);
        self.sentencia(cuerpo)?;
        let cond = self.pc();
        self.parchear(a_cond, cond);
        let marca = self.marca();
        self.saltar_si_cierto(c, inicio)?;
        self.soltar(marca);
        let fin = self.pc();
        self.cerrar_bucle(cond, fin);
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

    /// `for` rotado: tras `inicio`, salta a la condición, que va al final (después del paso) y
    /// vuelve al cuerpo si se cumple. Sin condición, el final salta al cuerpo sin mirar nada.
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
        let a_cond = cond.is_some().then(|| self.emitir(Instr::Saltar { destino: 0 }));
        let vuelta = self.pc();
        self.abrir_bucle(false);
        self.sentencia(cuerpo)?;
        let seguir = self.pc();
        if let Some(p) = paso {
            let marca = self.marca();
            self.expr_descartada(p)?;
            self.soltar(marca);
        }
        match cond {
            Some(c) => {
                let aqui = self.pc();
                if let Some(a) = a_cond {
                    self.parchear(a, aqui);
                }
                let marca = self.marca();
                self.saltar_si_cierto(c, vuelta)?;
                self.soltar(marca);
            }
            None => _ = self.emitir(Instr::Saltar { destino: vuelta }),
        }
        let fin = self.pc();
        self.cerrar_bucle(seguir, fin);
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
