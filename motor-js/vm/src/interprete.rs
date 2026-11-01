//! El bucle del intérprete: lee una instrucción, la ejecuta, sigue.
//!
//! El estado del marco actual (función, contador y base de registros) vive en variables
//! locales; solo se relee de `marcos` cuando una llamada, un retorno o una excepción
//! cambian de marco.

use crate::llamadas::Llamada;
use crate::{CABECERA, Motor, R_ARREGLO, R_EXCEPCION, R_FUNCION, R_OBJETO, operaciones as op};
use quimera_frontend::bytecode::{Instr, OpBin, OpUn, Reg};
use quimera_objetos::Valor;

fn es_nulo(v: Valor) -> bool {
    matches!(v.tipo(), quimera_objetos::Tipo::Nulo | quimera_objetos::Tipo::Indefinido)
}

/// Qué hacer después de una instrucción.
pub(crate) enum Flujo {
    Sigue,
    Salta(u32),
    /// Cambió el marco (llamada o excepción capturada): releer el estado.
    Recargar,
    /// `return` del marco actual.
    Retorno(Valor),
}

impl Motor {
    /// Ejecuta hasta que el número de marcos vuelve a `hasta`.
    pub(crate) fn correr(&mut self, hasta: usize) -> Result<(), Valor> {
        while self.marcos.len() > hasta {
            let n = self.marcos.len() - 1;
            let marco = self.marcos[n];
            let (fi, base, mut pc) = (marco.funcion as usize, marco.base, marco.pc);
            loop {
                let Some(ins) = self.funciones[fi].f.codigo.get(pc).copied() else {
                    self.cerrar_marco(Valor::INDEFINIDO);
                    break;
                };
                match self.paso(ins, fi, base, pc) {
                    Ok(Flujo::Sigue) => pc += 1,
                    Ok(Flujo::Salta(d)) => pc = d as usize,
                    Ok(Flujo::Recargar) => break,
                    Ok(Flujo::Retorno(v)) => {
                        self.cerrar_marco(v);
                        break;
                    }
                    Err(exc) => {
                        self.lanzar(exc, hasta)?;
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn r(&self, base: usize, r: Reg) -> Valor {
        self.pila(base + r as usize)
    }

    fn w(&mut self, base: usize, r: Reg, v: Valor) {
        if let Some(hueco) = self.mundo.m.pila_mut().get_mut(base + r as usize) {
            *hueco = v;
        }
    }

    /// Ejecuta una instrucción del marco con registros en `base`.
    fn paso(&mut self, ins: Instr, fi: usize, base: usize, pc: usize) -> Result<Flujo, Valor> {
        use Instr::*;
        let cab = base - CABECERA;
        match ins {
            Entero { dst, valor } => self.w(base, dst, Valor::entero(valor)),
            Constante { dst, k } => {
                let v = self.funciones[fi].constantes.get(k as usize).copied().unwrap_or(Valor::INDEFINIDO);
                self.w(base, dst, v);
            }
            Indefinido { dst } | Hueco { dst } => self.w(base, dst, Valor::INDEFINIDO),
            Nulo { dst } => self.w(base, dst, Valor::NULO),
            Booleano { dst, valor } => self.w(base, dst, Valor::booleano(valor)),
            Este { dst } => self.w(base, dst, self.pila(cab + 1)),
            FuncionActual { dst } => self.w(base, dst, self.pila(cab)),
            Mover { dst, src } => self.w(base, dst, self.r(base, src)),
            Binaria { op, dst, a, b } => {
                let v = self.binaria(op, self.r(base, a), self.r(base, b), base + b as usize)?;
                self.w(base, dst, v);
            }
            BinariaInm { op, dst, a, valor } => {
                let x = self.r(base, a);
                let v = match x.como_entero() {
                    // El caso de los bucles: entero más entero, sin salir de i32.
                    Some(i) if op == OpBin::Suma => i.checked_add(valor).map(Valor::entero),
                    Some(i) if op == OpBin::Resta => i.checked_sub(valor).map(Valor::entero),
                    _ => None,
                };
                let v = match v {
                    Some(v) => v,
                    None => self.binaria(op, x, Valor::entero(valor), usize::MAX)?,
                };
                self.w(base, dst, v);
            }
            SaltarSiNoCompara { op, a, b, destino } => {
                let (x, y) = (self.r(base, a), self.r(base, b));
                let cierto = match (x.como_entero(), y.como_entero(), op) {
                    (Some(i), Some(j), OpBin::Menor) => i < j,
                    (Some(i), Some(j), OpBin::MenorIgual) => i <= j,
                    (Some(i), Some(j), OpBin::Mayor) => i > j,
                    (Some(i), Some(j), OpBin::MayorIgual) => i >= j,
                    (Some(i), Some(j), OpBin::Estricto | OpBin::Igual) => i == j,
                    (Some(i), Some(j), OpBin::NoEstricto | OpBin::Distinto) => i != j,
                    _ => {
                        let v = self.binaria(op, x, y, base + b as usize)?;
                        op::a_booleano(&self.mundo, v)
                    }
                };
                return Ok(self.saltar_si(!cierto, destino));
            }
            Unaria { op, dst, a } => {
                let v = self.unaria(op, self.r(base, a))?;
                self.w(base, dst, v);
            }
            Saltar { destino } => return Ok(Flujo::Salta(destino)),
            SaltarSi { c, destino } => return Ok(self.saltar_si(op::a_booleano(&self.mundo, self.r(base, c)), destino)),
            SaltarSiNo { c, destino } => {
                return Ok(self.saltar_si(!op::a_booleano(&self.mundo, self.r(base, c)), destino));
            }
            SaltarSiDefinido { c, destino } => {
                let definido =
                    !matches!(self.r(base, c).tipo(), quimera_objetos::Tipo::Nulo | quimera_objetos::Tipo::Indefinido);
                return Ok(self.saltar_si(definido, destino));
            }
            LeerGlobal { dst, nombre } => {
                let v = self.leer_global(self.clave(fi, nombre), fi, pc)?;
                self.w(base, dst, v);
            }
            EscribirGlobal { nombre, src } => {
                let global = self.pila(crate::R_GLOBAL);
                self.escribir_prop(global, self.clave(fi, nombre), self.r(base, src), fi, pc)?;
            }
            TipoGlobal { dst, nombre } => {
                let v = self.tipo_global(self.clave(fi, nombre));
                self.w(base, dst, v);
            }
            otra => return self.paso_objetos(otra, fi, base, pc),
        }
        Ok(Flujo::Sigue)
    }

    fn saltar_si(&self, condicion: bool, destino: u32) -> Flujo {
        if condicion { Flujo::Salta(destino) } else { Flujo::Sigue }
    }

    /// Entornos, propiedades, objetos, llamadas y excepciones.
    fn paso_objetos(&mut self, ins: Instr, fi: usize, base: usize, pc: usize) -> Result<Flujo, Valor> {
        use Instr::*;
        let cab = base - CABECERA;
        match ins {
            CrearEntorno { huecos } => {
                let env = self.mundo.crear_entorno(self.pila(cab + 2), huecos);
                self.w(cab, 2, env);
            }
            LeerEntorno { dst, saltos, hueco } => {
                let v = self.mundo.leer_entorno(self.pila(cab + 2), saltos, hueco);
                let v = v.ok_or_else(|| self.error("InternalError", "entorno inexistente"))?;
                self.w(base, dst, v);
            }
            EscribirEntorno { saltos, hueco, src } => {
                let (env, v) = (self.pila(cab + 2), self.r(base, src));
                if !self.mundo.escribir_entorno(env, saltos, hueco, v) {
                    return Err(self.error("InternalError", "entorno inexistente"));
                }
            }
            LeerProp { dst, obj, nombre } => {
                let v = self.leer_prop(self.r(base, obj), self.clave(fi, nombre), fi, pc)?;
                self.w(base, dst, v);
            }
            EscribirProp { obj, nombre, src } => {
                self.escribir_prop(self.r(base, obj), self.clave(fi, nombre), self.r(base, src), fi, pc)?;
            }
            LeerIndice { dst, obj, indice } => {
                let v = self.leer_indice(self.r(base, obj), self.r(base, indice))?;
                self.w(base, dst, v);
            }
            EscribirIndice { obj, indice, src } => {
                self.escribir_indice(self.r(base, obj), self.r(base, indice), self.r(base, src))?
            }
            BorrarProp { dst, obj, nombre } => {
                let k = Valor::cadena(quimera_objetos::Ref::desde_u32(self.clave(fi, nombre)));
                let v = self.borrar(self.r(base, obj), k)?;
                self.w(base, dst, v);
            }
            BorrarIndice { dst, obj, indice } => {
                let v = self.borrar(self.r(base, obj), self.r(base, indice))?;
                self.w(base, dst, v);
            }
            IterarClaves { dst, obj } => {
                let v = self.iterar_claves(self.r(base, obj));
                self.w(base, dst, v);
            }
            IterarValores { dst, obj } => {
                let v = self.iterar_valores(self.r(base, obj))?;
                self.w(base, dst, v);
            }
            DeclararGlobal { nombre } => {
                let (clave, global) = (self.clave(fi, nombre), self.pila(crate::R_GLOBAL));
                if self.mundo.leer_propia(global, clave).is_none() {
                    self.mundo.escribir(global, clave, Valor::INDEFINIDO);
                }
            }
            CrearObjeto { dst } => {
                let o = self.mundo.crear_objeto(self.pila(R_OBJETO));
                self.w(base, dst, o);
            }
            CrearArreglo { dst, desde, n } => {
                let inicio = base + desde as usize;
                let elementos: alloc::vec::Vec<Valor> = self.mundo.m.pila()[inicio..inicio + n as usize].to_vec();
                let a = self.mundo.crear_arreglo(&elementos, self.pila(R_ARREGLO));
                self.w(base, dst, a);
            }
            Cierre { dst, funcion } => {
                let indice = self.funciones[fi].desplazamiento + funcion;
                let (env, este, proto) = (self.pila(cab + 2), self.pila(cab + 1), self.pila(R_FUNCION));
                let f = self.mundo.crear_funcion(indice, env, este, proto);
                self.w(base, dst, f);
            }
            otra => return self.paso_llamadas(otra, base, pc),
        }
        Ok(Flujo::Sigue)
    }

    fn paso_llamadas(&mut self, ins: Instr, base: usize, pc: usize) -> Result<Flujo, Valor> {
        use Instr::*;
        let abs = |r: Reg| base + r as usize;
        let llamada = match ins {
            Llamar { dst, f, este, args, n } => Llamada {
                f: abs(f),
                este: abs(este),
                args: abs(args),
                n: n as usize,
                destino: abs(dst),
                construir: false,
            },
            Construir { dst, f, args, n } => {
                Llamada { f: abs(f), este: abs(f), args: abs(args), n: n as usize, destino: abs(dst), construir: true }
            }
            Retornar { src } => return Ok(Flujo::Retorno(self.r(base, src))),
            Lanzar { src } => return Err(self.r(base, src)),
            Intentar { captura } => {
                self.proteger(captura as usize);
                return Ok(Flujo::Sigue);
            }
            FinIntentar => {
                self.manejadores.pop();
                return Ok(Flujo::Sigue);
            }
            Excepcion { dst } => {
                self.w(base, dst, self.pila(R_EXCEPCION));
                return Ok(Flujo::Sigue);
            }
            _ => return Ok(Flujo::Sigue),
        };
        // El contador solo hace falta guardarlo aquí: al volver, el marco sigue en la siguiente.
        if let Some(m) = self.marcos.last_mut() {
            m.pc = pc + 1;
        }
        Ok(if self.llamar(llamada)? { Flujo::Recargar } else { Flujo::Sigue })
    }

    /// `a op b`. `b_abs` es dónde está `b` en la pila (`instanceof` lo necesita para crear `F.prototype`).
    fn binaria(&mut self, op: OpBin, a: Valor, b: Valor, b_abs: usize) -> Result<Valor, Valor> {
        // Con un objeto de por medio, ToPrimitive (valueOf/toString) salvo donde no aplica.
        let (a, b) = match op {
            OpBin::Estricto | OpBin::NoEstricto | OpBin::En | OpBin::Instancia => (a, b),
            // `==` entre dos objetos compara identidad, y con null/undefined nunca convierte (§7.2.14).
            OpBin::Igual | OpBin::Distinto if self.mundo.es_objeto(a) && self.mundo.es_objeto(b) => (a, b),
            OpBin::Igual | OpBin::Distinto if es_nulo(a) || es_nulo(b) => (a, b),
            _ => self.primitivos(a, b)?,
        };
        Ok(match op {
            OpBin::Suma if (self.mundo.texto(a).is_some() || self.mundo.texto(b).is_some()) || self.objetos(a, b) => {
                let s = alloc::format!("{}{}", op::a_texto(&self.mundo, a), op::a_texto(&self.mundo, b));
                self.mundo.cadena(&s)
            }
            OpBin::Estricto => Valor::booleano(op::estricto(a, b)),
            OpBin::NoEstricto => Valor::booleano(!op::estricto(a, b)),
            OpBin::Igual => Valor::booleano(op::flexible(&self.mundo, a, b)),
            OpBin::Distinto => Valor::booleano(!op::flexible(&self.mundo, a, b)),
            OpBin::Menor | OpBin::MenorIgual | OpBin::Mayor | OpBin::MayorIgual => {
                Valor::booleano(op::comparar(&self.mundo, op, a, b))
            }
            OpBin::En => Valor::booleano(self.contiene(a, b)?),
            OpBin::Instancia => Valor::booleano(self.instancia(a, b_abs)?),
            OpBin::Potencia => {
                let (x, y) = (op::a_numero(&self.mundo, a), op::a_numero(&self.mundo, b));
                op::numero(crate::matematicas::potencia(x, y))
            }
            _ => op::aritmetica(&self.mundo, op, a, b).unwrap_or(Valor::numero(f64::NAN)),
        })
    }

    /// `+` con un objeto o un arreglo concatena su texto (su `ToPrimitive` por defecto).
    fn objetos(&self, a: Valor, b: Valor) -> bool {
        self.mundo.es_objeto(a) || self.mundo.es_objeto(b)
    }

    fn unaria(&mut self, op: OpUn, a: Valor) -> Result<Valor, Valor> {
        let a = if matches!(op, OpUn::Negar | OpUn::ANumero | OpUn::NoBits) { self.a_primitivo(a)? } else { a };
        Ok(match op {
            OpUn::Negar => match a.como_entero() {
                Some(i) if i != 0 && i != i32::MIN => Valor::entero(-i),
                _ => op::numero(-op::a_numero(&self.mundo, a)),
            },
            OpUn::ANumero => op::numero(op::a_numero(&self.mundo, a)),
            OpUn::No => Valor::booleano(!op::a_booleano(&self.mundo, a)),
            OpUn::NoBits => Valor::entero(!op::a_int32(&self.mundo, a)),
            OpUn::Tipo => {
                let t = op::tipo_de(&self.mundo, a);
                self.mundo.cadena(t)
            }
            OpUn::Vacio => Valor::INDEFINIDO,
        })
    }
}
