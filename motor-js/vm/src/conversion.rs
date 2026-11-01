//! `ToPrimitive`: convertir un objeto en primitivo llamando a sus métodos `valueOf` y `toString`.
//!
//! Esto necesita ejecutar JavaScript desde dentro de una operación (`a - b`, `Math.log(o)`):
//! se abre una llamada interna y se ejecuta con `correr(hasta)`, que vuelve cuando esa
//! llamada termina. Una excepción dentro sale como `Err` y la recoge el `try` de fuera.

use crate::Motor;
use crate::llamadas::Llamada;
use crate::operaciones as op;
use quimera_objetos::{Clase, Valor};

impl Motor {
    /// Llama a `f` con este `this` y estos argumentos desde dentro de la máquina, y devuelve su resultado.
    pub(crate) fn llamar_interno(&mut self, f: Valor, este: Valor, args: &[Valor]) -> Result<Valor, Valor> {
        let base = self.mundo.m.abrir_raices(3 + args.len(), Valor::INDEFINIDO);
        let pila = self.mundo.m.pila_mut();
        pila[base] = f;
        pila[base + 1] = este;
        pila[base + 3..base + 3 + args.len()].copy_from_slice(args);
        let hasta = self.marcos.len();
        let destino = base + 2;
        let ll = Llamada { f: base, este: base + 1, args: base + 3, n: args.len(), destino, construir: false };
        let r = match self.llamar(ll) {
            Ok(true) => self.correr(hasta),
            Ok(false) => Ok(()),
            Err(e) => Err(e),
        };
        let resultado = r.map(|()| self.pila(destino));
        self.mundo.m.soltar_raices(base);
        resultado
    }

    /// `ToPrimitive`: un primitivo se devuelve igual; un objeto prueba `valueOf` y luego
    /// `toString`. Sin ninguno propio, su texto por defecto (`[object Object]`, `1,2,3`).
    pub(crate) fn a_primitivo(&mut self, v: Valor) -> Result<Valor, Valor> {
        if !self.mundo.es_objeto(v) {
            return Ok(v);
        }
        if let Some(p) = op::valor_primitivo(&self.mundo, v) {
            return Ok(p);
        }
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        for nombre in ["valueOf", "toString"] {
            let clave = self.mundo.cadenas.internar(nombre);
            let obj = self.pila(marca);
            let Some(f) = self.mundo.leer(obj, clave) else { continue };
            if !matches!(self.mundo.clase(f), Some(Clase::Funcion | Clase::Nativa)) {
                continue;
            }
            let r = self.llamar_interno(f, obj, &[]);
            match r {
                Ok(r) if !self.mundo.es_objeto(r) => {
                    self.mundo.m.soltar_raices(marca);
                    return Ok(r);
                }
                Ok(_) => {}
                Err(e) => {
                    self.mundo.m.soltar_raices(marca);
                    return Err(e);
                }
            }
        }
        let texto = op::a_texto(&self.mundo, self.pila(marca));
        self.mundo.m.soltar_raices(marca);
        Ok(self.mundo.cadena(&texto))
    }

    /// Los dos operandos de una operación, convertidos a primitivos si alguno es un objeto.
    /// Se protegen en la pila: convertir el primero ejecuta JavaScript y puede mover el segundo.
    pub(crate) fn primitivos(&mut self, a: Valor, b: Valor) -> Result<(Valor, Valor), Valor> {
        if !self.mundo.es_objeto(a) && !self.mundo.es_objeto(b) {
            return Ok((a, b));
        }
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(a);
        self.mundo.m.raiz(b);
        let r = self.a_primitivo(self.pila(marca)).and_then(|pa| {
            self.mundo.m.pila_mut()[marca] = pa;
            self.a_primitivo(self.pila(marca + 1)).map(|pb| (self.pila(marca), pb))
        });
        self.mundo.m.soltar_raices(marca);
        r
    }

    /// `ToNumber` completo (con `valueOf` de los objetos).
    pub(crate) fn a_numero(&mut self, v: Valor) -> Result<f64, Valor> {
        let p = self.a_primitivo(v)?;
        Ok(op::a_numero(&self.mundo, p))
    }
}
