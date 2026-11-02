//! Llamadas, retornos y excepciones.
//!
//! Una llamada se describe con posiciones absolutas de la pila de raíces (no con valores),
//! porque crear el objeto de un `new` puede mover objetos: leyendo de la pila siempre se
//! obtiene la dirección actual.

use crate::{CABECERA, Manejador, Marco, Motor, R_ARREGLO, R_EXCEPCION, R_OBJETO, nativas};
use alloc::format;
use quimera_objetos::{Clase, Valor};

/// Una llamada pendiente: dónde están la función, `this`, los argumentos y el destino.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Llamada {
    pub f: usize,
    pub este: usize,
    pub args: usize,
    pub n: usize,
    pub destino: usize,
    pub construir: bool,
}

impl Motor {
    #[inline(always)]
    pub(crate) fn pila(&self, i: usize) -> Valor {
        self.mundo.m.pila().get(i).copied().unwrap_or(Valor::INDEFINIDO)
    }

    /// Un error de JavaScript (por ahora, una cadena `"TypeError: ..."`).
    pub(crate) fn error(&mut self, clase: &str, mensaje: &str) -> Valor {
        self.mundo.cadena(&format!("{clase}: {mensaje}"))
    }

    /// Empieza una llamada. `Ok(true)`: hay un marco nuevo que ejecutar; `Ok(false)`: ya terminó
    /// (una nativa) y el resultado está en el destino.
    pub(crate) fn llamar(&mut self, ll: Llamada) -> Result<bool, Valor> {
        let f = self.pila(ll.f);
        if let Some(datos) = self.mundo.funcion(f) {
            let generador = self.funciones.get(datos.0 as usize).is_some_and(|c| c.f.generador);
            if generador {
                if ll.construir {
                    return Err(self.error("TypeError", "un generador no es un constructor"));
                }
                self.abrir_marco(datos, ll, Valor::INDEFINIDO);
                self.crear_generador();
                return Ok(false);
            }
            let nuevo = if ll.construir { self.objeto_para_new(ll.f, datos.0)? } else { Valor::INDEFINIDO };
            // `new` puede haber movido la función: sus datos se vuelven a leer.
            let datos = if ll.construir { self.mundo.funcion(self.pila(ll.f)).unwrap_or(datos) } else { datos };
            self.abrir_marco(datos, ll, nuevo);
            return Ok(true);
        }
        if let Some(id) = self.mundo.nativa(f) {
            return nativas::llamar(self, id, ll);
        }
        let que = self.texto(f);
        Err(self.error("TypeError", &format!("{que} no es una función")))
    }

    /// El objeto que recibe `this` en `new F()`: su prototipo es `F.prototype`.
    fn objeto_para_new(&mut self, f_abs: usize, indice: u32) -> Result<Valor, Valor> {
        if self.funciones.get(indice as usize).is_some_and(|c| c.f.flecha) {
            return Err(self.error("TypeError", "una función flecha no es un constructor"));
        }
        let proto = self.prototipo_de_funcion(f_abs);
        let proto = if self.mundo.es_objeto(proto) { proto } else { self.pila(R_OBJETO) };
        Ok(self.mundo.crear_objeto(proto))
    }

    /// `F.prototype`, creándolo la primera vez (con `constructor` apuntando a `F`).
    pub(crate) fn prototipo_de_funcion(&mut self, f_abs: usize) -> Valor {
        let clave = self.mundo.cadenas.internar("prototype");
        if let Some(p) = self.mundo.leer_propia(self.pila(f_abs), clave) {
            return p;
        }
        let proto = self.mundo.crear_objeto(self.pila(R_OBJETO));
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(proto);
        let constructor = self.mundo.cadenas.internar("constructor");
        let f = self.pila(f_abs);
        self.mundo.escribir(proto, constructor, f);
        let proto = self.pila(marca);
        let f = self.pila(f_abs);
        self.mundo.escribir(f, clave, proto);
        self.mundo.m.soltar_raices(marca);
        self.mundo.leer_propia(self.pila(f_abs), clave).unwrap_or(Valor::INDEFINIDO)
    }

    /// Abre el marco de la función `indice` y copia los argumentos a sus primeros registros.
    fn abrir_marco(&mut self, (indice, entorno, este_lexico): (u32, Valor, Valor), ll: Llamada, nuevo: Valor) {
        let Some(c) = self.funciones.get(indice as usize) else { return };
        let (params, registros, flecha, estricta) =
            (c.f.params as usize, c.f.registros as usize, c.f.flecha, c.f.estricta);
        let argumentos = c.f.argumentos;
        let f = self.pila(ll.f);
        let este = if flecha {
            este_lexico
        } else if ll.construir {
            nuevo
        } else {
            // Fuera del modo estricto, `f()` recibe el objeto global como `this`.
            let e = self.pila(ll.este);
            if !estricta && matches!(e.tipo(), quimera_objetos::Tipo::Indefinido | quimera_objetos::Tipo::Nulo) {
                self.pila(crate::R_GLOBAL)
            } else {
                e
            }
        };
        let base_marco = self.mundo.m.abrir_raices(CABECERA + registros.max(params), Valor::INDEFINIDO);
        let base = base_marco + CABECERA;
        let pila = self.mundo.m.pila_mut();
        pila[base_marco] = f;
        pila[base_marco + 1] = este;
        pila[base_marco + 2] = entorno;
        pila[base_marco + 3] = nuevo;
        for i in 0..ll.n.min(params) {
            pila[base + i] = pila[ll.args + i];
        }
        if let Some(r) = argumentos {
            // El marco ya está en las raíces: crear el arreglo puede mover objetos sin peligro.
            let valores: alloc::vec::Vec<Valor> = pila[ll.args..ll.args + ll.n].to_vec();
            let a = self.mundo.crear_arreglo(&valores, self.pila(R_ARREGLO));
            self.mundo.m.pila_mut()[base + r as usize] = a;
        }
        let destino = Some(ll.destino);
        let soltar = base_marco;
        self.marcos.push(Marco {
            funcion: indice,
            pc: 0,
            base,
            destino,
            construir: ll.construir,
            soltar,
            generador: false,
        });
    }

    /// Cierra el marco actual con el valor `v` y lo escribe en el destino de quien llamó.
    pub(crate) fn cerrar_marco(&mut self, v: Valor) {
        let Some(marco) = self.marcos.pop() else { return };
        let base_marco = marco.base - CABECERA;
        let v = if marco.construir && !self.mundo.es_objeto(v) { self.pila(base_marco + 3) } else { v };
        let v = if marco.generador { self.terminar_generador(&marco, v) } else { v };
        self.mundo.m.soltar_raices(marco.soltar);
        if let Some(d) = marco.destino
            && let Some(hueco) = self.mundo.m.pila_mut().get_mut(d)
        {
            *hueco = v;
        }
        // Un `try` del marco que se cierra ya no protege nada.
        let n = self.marcos.len();
        while self.manejadores.last().is_some_and(|h| h.marcos > n) {
            self.manejadores.pop();
        }
    }

    /// `try {`: desde aquí, una excepción de este marco salta a `captura`.
    pub(crate) fn proteger(&mut self, captura: usize) {
        self.manejadores.push(Manejador { marcos: self.marcos.len(), captura });
    }

    /// Lanza `exc`. Si un `try` de esta ejecución (por encima de `hasta`) la captura, deshace los
    /// marcos hasta él y sigue en su `catch`; si no, deshace hasta `hasta` y la devuelve.
    pub(crate) fn lanzar(&mut self, exc: Valor, hasta: usize) -> Result<(), Valor> {
        if let Some(h) = self.mundo.m.pila_mut().get_mut(R_EXCEPCION) {
            *h = exc;
        }
        if let Some(h) = self.manejadores.last().copied().filter(|h| h.marcos > hasta) {
            self.manejadores.pop();
            self.deshacer_hasta(h.marcos);
            if let Some(m) = self.marcos.last_mut() {
                m.pc = h.captura;
            }
            return Ok(());
        }
        self.deshacer_hasta(hasta);
        Err(exc)
    }

    fn deshacer_hasta(&mut self, n: usize) {
        while self.marcos.len() > n {
            if let Some(m) = self.marcos.pop() {
                if m.generador {
                    self.abortar_generador(&m);
                }
                self.mundo.m.soltar_raices(m.soltar);
            }
        }
    }

    /// Llama a `f` con `este` y `args` **sin recursión de Rust** (`apply`): los pone en la pila,
    /// abre su marco como cualquier llamada y el marco suelta también esos valores al cerrarse.
    /// `Ok(true)`: hay un marco nuevo que ejecutar; `Ok(false)`: era una nativa y ya terminó.
    pub(crate) fn llamar_con(&mut self, f: Valor, este: Valor, args: &[Valor], destino: usize) -> Result<bool, Valor> {
        let base = self.mundo.m.abrir_raices(2 + args.len(), Valor::INDEFINIDO);
        let pila = self.mundo.m.pila_mut();
        pila[base] = f;
        pila[base + 1] = este;
        pila[base + 2..base + 2 + args.len()].copy_from_slice(args);
        let ll = Llamada { f: base, este: base + 1, args: base + 2, n: args.len(), destino, construir: false };
        let r = self.llamar(ll);
        match r {
            Ok(true) => {
                if let Some(m) = self.marcos.last_mut() {
                    m.soltar = base;
                }
            }
            _ => self.mundo.m.soltar_raices(base),
        }
        r
    }

    /// La clase de lo que hay en una posición de la pila.
    pub(crate) fn clase_en(&self, abs: usize) -> Option<Clase> {
        self.mundo.clase(self.pila(abs))
    }
}
