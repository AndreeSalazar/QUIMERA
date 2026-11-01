//! Control de flujo que cruza `try`: `finally`, y `return`/`break`/`continue` que salen de él;
//! también `for-in` y `for-of`.
//!
//! Cada `try` abre una **región** en tiempo de compilación. Salir de ella antes de tiempo
//! (`return`, `break`, `continue`) cierra su manejador (`FinIntentar`) y ejecuta su `finally`
//! copiado en ese punto: el bytecode no necesita saltos indirectos.

use super::{Compilador, Region};
use crate::ast::{Decl, Expr, Sent};
use crate::bytecode::{Instr, OpBin};
use crate::fuente::{Pos, Resultado, error};
use alloc::string::String;
use alloc::vec::Vec;

impl Compilador<'_> {
    /// Cierra las regiones `try` desde la más interna hasta dejar solo `hasta`: quita cada
    /// manejador y ejecuta cada `finally` (compilado con solo las regiones de fuera activas).
    fn salir_regiones(&mut self, hasta: usize) -> Resultado<()> {
        let guardadas = self.ctx().regiones.clone();
        for i in (hasta..guardadas.len()).rev() {
            self.ctx().regiones.truncate(i);
            let region = &guardadas[i];
            if region.manejador {
                self.emitir(Instr::FinIntentar);
            }
            if let Some(f) = &region.final_ {
                for s in f {
                    self.sentencia(s)?;
                }
            }
        }
        self.ctx().regiones = guardadas;
        Ok(())
    }

    /// `return v`: el valor se calcula antes de los `finally`, que no pueden cambiarlo.
    pub(super) fn retornar(&mut self, v: Option<&Expr>) -> Resultado<()> {
        let hay_regiones = !self.ctx().regiones.is_empty();
        let r = match v {
            Some(e) if !hay_regiones => self.operando(e)?,
            Some(e) => {
                let t = self.temporal()?;
                self.expr(e, t)?;
                t
            }
            None => {
                let t = self.temporal()?;
                self.emitir(Instr::Indefinido { dst: t });
                t
            }
        };
        self.salir_regiones(0)?;
        self.emitir(Instr::Retornar { src: r });
        Ok(())
    }

    /// `break` / `continue`: cierra las regiones abiertas dentro del bucle y salta.
    pub(super) fn romper_o_continuar(&mut self, romper: bool) -> Resultado<()> {
        let bucles = &self.ctx().bucles;
        // `continue` va al bucle más cercano que no sea un switch.
        let indice = if romper { bucles.len().checked_sub(1) } else { bucles.iter().rposition(|b| !b.es_switch) };
        let Some(indice) = indice else {
            let que = if romper { "break" } else { "continue" };
            return error(Pos::default(), alloc::format!("{que} fuera de un bucle"));
        };
        let regiones = self.ctx().bucles[indice].regiones;
        self.salir_regiones(regiones)?;
        let salto = self.emitir(Instr::Saltar { destino: 0 });
        let b = &mut self.ctx().bucles[indice];
        if romper {
            b.romper.push(salto)
        } else {
            b.continuar.push(salto)
        }
        Ok(())
    }

    /// `try { B } catch (e) { C } finally { F }`.
    ///
    /// ```text
    ///   Intentar → captura        ; protege B
    ///   B ; FinIntentar ; F ; Saltar fin
    /// captura:
    ///   Intentar → relanzar       ; solo si hay finally: protege C para ejecutar F igualmente
    ///   e = excepción ; C ; FinIntentar ; F ; Saltar fin
    /// relanzar:                   ; solo si hay finally
    ///   t = excepción ; F ; throw t
    /// fin:
    /// ```
    pub(super) fn intentar(
        &mut self,
        cuerpo: &[Sent],
        captura: Option<&(Option<String>, Vec<Sent>)>,
        final_: Option<&Vec<Sent>>,
    ) -> Resultado<()> {
        let final_ = final_.cloned();
        let proteger = self.emitir(Instr::Intentar { captura: 0 });
        self.bloque_en_region(cuerpo, Region { manejador: true, final_: final_.clone() })?;
        self.emitir(Instr::FinIntentar);
        self.final_en_linea(final_.as_ref())?;
        let mut a_fin = alloc::vec![self.emitir(Instr::Saltar { destino: 0 })];
        let aqui = self.pc();
        self.parchear(proteger, aqui);
        let relanzar = match captura {
            Some((nombre, bloque)) => {
                let proteger_c = final_.as_ref().map(|_| self.emitir(Instr::Intentar { captura: 0 }));
                let t = self.temporal()?;
                self.emitir(Instr::Excepcion { dst: t });
                if let Some(n) = nombre {
                    self.guardar(n, t);
                }
                self.bloque_en_region(bloque, Region { manejador: final_.is_some(), final_: final_.clone() })?;
                if proteger_c.is_some() {
                    self.emitir(Instr::FinIntentar);
                }
                self.final_en_linea(final_.as_ref())?;
                a_fin.push(self.emitir(Instr::Saltar { destino: 0 }));
                proteger_c
            }
            None => None,
        };
        if let Some(f) = &final_ {
            if let Some(p) = relanzar {
                let aqui = self.pc();
                self.parchear(p, aqui);
            }
            let t = self.temporal()?;
            self.emitir(Instr::Excepcion { dst: t });
            self.final_en_linea(Some(f))?;
            self.emitir(Instr::Lanzar { src: t });
        }
        let fin = self.pc();
        for s in a_fin {
            self.parchear(s, fin);
        }
        Ok(())
    }

    fn bloque_en_region(&mut self, bloque: &[Sent], region: Region) -> Resultado<()> {
        self.ctx().regiones.push(region);
        let r = bloque.iter().try_for_each(|s| self.sentencia(s));
        self.ctx().regiones.pop();
        r
    }

    fn final_en_linea(&mut self, final_: Option<&Vec<Sent>>) -> Resultado<()> {
        for s in final_.map_or(&[][..], |f| f.as_slice()) {
            self.sentencia(s)?;
        }
        Ok(())
    }

    /// `for (x in o)` recorre las claves; `for (x of a)`, los valores (arreglos y cadenas).
    ///
    /// ```text
    ///   lista = claves(o) | valores(a) ; i = 0
    /// vuelta:
    ///   si no i < lista.length saltar fin
    ///   x = lista[i] ; cuerpo
    /// seguir:
    ///   i = i + 1 ; saltar vuelta
    /// fin:
    /// ```
    pub(super) fn para_en(
        &mut self,
        decl: Option<Decl>,
        nombre: &str,
        de: bool,
        objeto: &Expr,
        cuerpo: &Sent,
    ) -> Resultado<()> {
        let _ = decl; // la variable ya está declarada por el análisis de ámbitos
        let (lista, i, largo, valor) = (self.temporal()?, self.temporal()?, self.temporal()?, self.temporal()?);
        let o = self.operando(objeto)?;
        self.emitir(if de {
            Instr::IterarValores { dst: lista, obj: o }
        } else {
            Instr::IterarClaves { dst: lista, obj: o }
        });
        self.emitir(Instr::Entero { dst: i, valor: 0 });
        let vuelta = self.pc();
        let length = self.atomo("length");
        self.emitir(Instr::LeerProp { dst: largo, obj: lista, nombre: length });
        let a_fin = self.emitir(Instr::SaltarSiNoCompara { op: OpBin::Menor, a: i, b: largo, destino: 0 });
        self.emitir(Instr::LeerIndice { dst: valor, obj: lista, indice: i });
        self.guardar(nombre, valor);
        self.abrir_bucle(false);
        self.sentencia(cuerpo)?;
        let seguir = self.pc();
        self.emitir(Instr::BinariaInm { op: OpBin::Suma, dst: i, a: i, valor: 1 });
        self.emitir(Instr::Saltar { destino: vuelta });
        let fin = self.pc();
        self.parchear(a_fin, fin);
        self.cerrar_bucle(seguir, fin);
        Ok(())
    }
}
