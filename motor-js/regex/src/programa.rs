//! Árbol → programa: una lista de operaciones para la máquina con vuelta atrás.
//!
//! Las repeticiones se compilan con `Dividir` (probar una rama y guardar la otra para volver).
//! `{n,m}` copia el cuerpo; un cuerpo que puede no consumir nada lleva una marca de posición
//! para que `(a*)*` no gire para siempre.

use crate::Error;
use crate::clases::{Clase, plegar};
use crate::patron::Nodo;
use alloc::vec::Vec;

/// Tamaño máximo del programa: `a{1000}{1000}` no debe agotar la memoria.
const MAX_OPERACIONES: usize = 200_000;

/// Una operación de la máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Caracter(char),
    /// Carácter ya plegado (bandera `i`): se compara con la entrada plegada.
    CaracterSinCaso(char),
    /// `.`: con `todo` (bandera `s`) también los fines de línea.
    Punto {
        todo: bool,
    },
    Clase(u32),
    Inicio {
        multilinea: bool,
    },
    Fin {
        multilinea: bool,
    },
    Limite(bool),
    /// Sigue en `.0`; si falla, vuelve a `.1`.
    Dividir(u32, u32),
    Saltar(u32),
    /// Guarda la posición en la ranura (inicio y fin de cada grupo).
    Guardar(u32),
    /// Borra las capturas de los grupos `desde..=hasta` (al empezar cada vuelta de una repetición).
    Limpiar(u32, u32),
    Referencia(u32),
    /// `(?=...)`: el subprograma va desde la siguiente hasta `fin` (que es un `Exito`). Con
    /// `atras` (`(?<=...)`), se prueba empezando en cada posición anterior y antes del `Exito`
    /// hay un `FinAtras`, que exige acabar justo donde se miraba.
    Mirar {
        positiva: bool,
        fin: u32,
        atras: bool,
    },
    /// El final de un `(?<=...)`: falla si no se llegó a la posición desde la que se mira.
    FinAtras,
    MarcarPosicion(u32),
    /// Falla si la posición no avanzó desde su marca.
    ExigirAvance(u32),
    Exito,
}

/// El programa compilado.
#[derive(Debug, Clone, Default)]
pub(crate) struct Programa {
    pub(crate) ops: Vec<Op>,
    pub(crate) clases: Vec<Clase>,
    /// Marcas de posición que usa (`MarcarPosicion`).
    pub(crate) registros: u32,
    /// Bandera `i` (para las referencias ``).
    pub(crate) sin_caso: bool,
}

pub(crate) struct Opciones {
    pub(crate) sin_caso: bool,
    pub(crate) multilinea: bool,
    pub(crate) punto_todo: bool,
}

pub(crate) fn compilar(nodo: &Nodo, op: &Opciones) -> Result<Programa, Error> {
    let mut c = Compilador { p: Programa::default(), op };
    c.emitir(Op::Guardar(0))?;
    c.nodo(nodo)?;
    c.emitir(Op::Guardar(1))?;
    c.emitir(Op::Exito)?;
    Ok(c.p)
}

struct Compilador<'a> {
    p: Programa,
    op: &'a Opciones,
}

impl Compilador<'_> {
    fn emitir(&mut self, o: Op) -> Result<u32, Error> {
        if self.p.ops.len() >= MAX_OPERACIONES {
            return Err(Error { mensaje: alloc::string::String::from("expresión regular demasiado grande") });
        }
        self.p.ops.push(o);
        Ok(self.p.ops.len() as u32 - 1)
    }

    fn aqui(&self) -> u32 {
        self.p.ops.len() as u32
    }

    fn parchear(&mut self, en: u32, a: u32, primera: bool) {
        match &mut self.p.ops[en as usize] {
            Op::Dividir(x, _) if primera => *x = a,
            Op::Dividir(_, y) => *y = a,
            Op::Saltar(x) => *x = a,
            Op::Mirar { fin, .. } => *fin = a,
            _ => {}
        }
    }

    fn nodo(&mut self, n: &Nodo) -> Result<(), Error> {
        match n {
            Nodo::Vacio => {}
            Nodo::Caracter(c) if self.op.sin_caso => {
                self.emitir(Op::CaracterSinCaso(plegar(*c)))?;
            }
            Nodo::Caracter(c) => {
                self.emitir(Op::Caracter(*c))?;
            }
            Nodo::Punto => {
                self.emitir(Op::Punto { todo: self.op.punto_todo })?;
            }
            Nodo::Clase(k) => {
                let i = self.p.clases.iter().position(|x| x == k).unwrap_or_else(|| {
                    self.p.clases.push(k.clone());
                    self.p.clases.len() - 1
                });
                self.emitir(Op::Clase(i as u32))?;
            }
            Nodo::Inicio => {
                self.emitir(Op::Inicio { multilinea: self.op.multilinea })?;
            }
            Nodo::Fin => {
                self.emitir(Op::Fin { multilinea: self.op.multilinea })?;
            }
            Nodo::Limite(b) => {
                self.emitir(Op::Limite(*b))?;
            }
            Nodo::Grupo(dentro, numero) => self.grupo(dentro, *numero)?,
            Nodo::Mirar(dentro, positiva, atras) => {
                let m = self.emitir(Op::Mirar { positiva: *positiva, fin: 0, atras: *atras })?;
                self.nodo(dentro)?;
                if *atras {
                    self.emitir(Op::FinAtras)?;
                }
                let fin = self.emitir(Op::Exito)?;
                self.parchear(m, fin, false);
            }
            Nodo::Referencia(g) => {
                self.emitir(Op::Referencia(*g))?;
            }
            Nodo::Secuencia(v) => v.iter().try_for_each(|x| self.nodo(x))?,
            Nodo::Alternativa(ramas) => self.alternativa(ramas)?,
            Nodo::Repetir { nodo, min, max, codicioso } => self.repetir(nodo, *min, *max, *codicioso)?,
        }
        Ok(())
    }

    fn grupo(&mut self, dentro: &Nodo, numero: Option<u32>) -> Result<(), Error> {
        if let Some(g) = numero {
            self.emitir(Op::Guardar(2 * g))?;
        }
        self.nodo(dentro)?;
        if let Some(g) = numero {
            self.emitir(Op::Guardar(2 * g + 1))?;
        }
        Ok(())
    }

    /// `a|b|c`: Dividir(a, siguiente) … cada rama salta al final.
    fn alternativa(&mut self, ramas: &[Nodo]) -> Result<(), Error> {
        let mut a_fin = Vec::new();
        for (i, r) in ramas.iter().enumerate() {
            let ultima = i + 1 == ramas.len();
            let division = if ultima { None } else { Some(self.emitir(Op::Dividir(0, 0))?) };
            if let Some(d) = division {
                let aqui = self.aqui();
                self.parchear(d, aqui, true);
            }
            self.nodo(r)?;
            if !ultima {
                a_fin.push(self.emitir(Op::Saltar(0))?);
            }
            if let Some(d) = division {
                let aqui = self.aqui();
                self.parchear(d, aqui, false);
            }
        }
        let fin = self.aqui();
        for s in a_fin {
            self.parchear(s, fin, true);
        }
        Ok(())
    }

    /// Una vuelta del cuerpo: borra sus capturas viejas y lo ejecuta.
    fn vuelta(&mut self, cuerpo: &Nodo) -> Result<(), Error> {
        if let Some((desde, hasta)) = grupos_de(cuerpo) {
            self.emitir(Op::Limpiar(desde, hasta))?;
        }
        self.nodo(cuerpo)
    }

    fn repetir(&mut self, cuerpo: &Nodo, min: u32, max: Option<u32>, codicioso: bool) -> Result<(), Error> {
        for _ in 0..min {
            self.vuelta(cuerpo)?;
        }
        let vacio = puede_vacio(cuerpo);
        match max {
            None => {
                // bucle: Dividir(cuerpo, fin); [marca] cuerpo [avance]; Saltar bucle
                let bucle = self.emitir(Op::Dividir(0, 0))?;
                let marca = if vacio { Some(self.registro()) } else { None };
                let cuerpo_pc = self.aqui();
                if let Some(r) = marca {
                    self.emitir(Op::MarcarPosicion(r))?;
                }
                self.vuelta(cuerpo)?;
                if let Some(r) = marca {
                    self.emitir(Op::ExigirAvance(r))?;
                }
                self.emitir(Op::Saltar(bucle))?;
                let fin = self.aqui();
                self.preferir(bucle, cuerpo_pc, fin, codicioso);
            }
            Some(max) => {
                let mut divisiones = Vec::new();
                for _ in min..max {
                    let d = self.emitir(Op::Dividir(0, 0))?;
                    divisiones.push((d, self.aqui()));
                    self.vuelta(cuerpo)?;
                }
                let fin = self.aqui();
                for (d, cuerpo_pc) in divisiones {
                    self.preferir(d, cuerpo_pc, fin, codicioso);
                }
            }
        }
        Ok(())
    }

    /// Codicioso: primero el cuerpo; perezoso (`*?`): primero salir.
    fn preferir(&mut self, division: u32, cuerpo: u32, fin: u32, codicioso: bool) {
        let (a, b) = if codicioso { (cuerpo, fin) } else { (fin, cuerpo) };
        self.p.ops[division as usize] = Op::Dividir(a, b);
    }

    fn registro(&mut self) -> u32 {
        self.p.registros += 1;
        self.p.registros - 1
    }
}

/// Si el nodo puede encajar sin consumir nada.
fn puede_vacio(n: &Nodo) -> bool {
    match n {
        Nodo::Caracter(_) | Nodo::Punto | Nodo::Clase(_) => false,
        Nodo::Vacio | Nodo::Inicio | Nodo::Fin | Nodo::Limite(_) | Nodo::Mirar(..) | Nodo::Referencia(_) => true,
        Nodo::Grupo(d, _) => puede_vacio(d),
        Nodo::Secuencia(v) => v.iter().all(puede_vacio),
        Nodo::Alternativa(v) => v.iter().any(puede_vacio),
        Nodo::Repetir { nodo, min, .. } => *min == 0 || puede_vacio(nodo),
    }
}

/// El primer y el último grupo que captura el nodo, si captura alguno.
fn grupos_de(n: &Nodo) -> Option<(u32, u32)> {
    let unir = |a: Option<(u32, u32)>, b: Option<(u32, u32)>| match (a, b) {
        (Some((x, y)), Some((z, w))) => Some((x.min(z), y.max(w))),
        (a, None) => a,
        (None, b) => b,
    };
    match n {
        Nodo::Grupo(d, g) => unir(g.map(|g| (g, g)), grupos_de(d)),
        Nodo::Mirar(d, ..) => grupos_de(d),
        Nodo::Repetir { nodo, .. } => grupos_de(nodo),
        Nodo::Secuencia(v) | Nodo::Alternativa(v) => v.iter().fold(None, |acc, x| unir(acc, grupos_de(x))),
        _ => None,
    }
}
