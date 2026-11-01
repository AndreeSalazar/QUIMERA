//! El DOM de QUIMERA: el árbol del documento (`quimera-html`) visto desde JavaScript
//! (`quimera-vm`).
//!
//! - **Un solo árbol.** Los scripts cambian el mismo [`Documento`] que leen el estilo y la
//!   maquetación: no hay una copia para JavaScript. Tras un cambio, quien muestra la página
//!   vuelve a maquetar ([`Dom::tomar_cambios`]).
//! - **Un solo GC.** Cada nodo que JavaScript toca tiene un objeto del anfitrión en el montón de
//!   la VM (con el número del nodo), creado la primera vez y guardado en la raíz del anfitrión:
//!   el mismo nodo da siempre el mismo objeto (`a === b`). Los listeners y temporizadores también
//!   viven en ese montón. No hay dos montones que se apunten entre sí (el problema de Chromium
//!   que resolvió Oilpan).
//! - **Las propiedades** (`textContent`, `id`...) se le preguntan al anfitrión en cada acceso
//!   (`puente.rs`); los métodos son funciones del anfitrión en tres prototipos: nodo, elemento y
//!   documento.
//!
//! Hecho a mano para lo que más usan las páginas. Cuando la superficie crezca, estas tablas se
//! generarán desde WebIDL (ver docs/PLAN.md).
#![no_std]

extern crate alloc;

mod eventos;
mod metodos;
mod puente;
mod selectores;

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, Ref, RefCell};
use quimera_html::{Dato, Documento, NodoId};
use quimera_vm::{Motor, Valor};

pub use selectores::Selector;

/// El número de "nodo" de `window` en los listeners (no es un nodo del árbol).
pub(crate) const VENTANA: u32 = u32::MAX;
/// Bit de los objetos `style` de un elemento (el resto es el número del nodo).
pub(crate) const BIT_ESTILO: u32 = 1 << 30;
/// Bit de los objetos `classList`.
pub(crate) const BIT_CLASES: u32 = 1 << 29;

/// Lo que comparten el [`Dom`] y su puente con la VM.
pub(crate) struct Estado {
    pub(crate) doc: RefCell<Documento>,
    /// El árbol cambió desde la última vez que se preguntó.
    pub(crate) cambiado: Cell<bool>,
    /// `preventDefault()` y `stopPropagation()` del evento en curso.
    pub(crate) prevenido: Cell<bool>,
    pub(crate) detenido: Cell<bool>,
    /// La hora del anfitrión en milisegundos (para los temporizadores).
    pub(crate) ahora: Cell<f64>,
    pub(crate) siguiente_temporizador: Cell<i32>,
    /// La dirección de la página (`location.href`, `document.URL`).
    pub(crate) url: String,
    /// Errores de JavaScript que nadie capturó (de eventos y temporizadores).
    pub(crate) errores: RefCell<Vec<String>>,
}

/// Una página con JavaScript: su árbol y su motor.
pub struct Dom {
    /// El motor de JavaScript de la página.
    pub motor: Motor,
    estado: Rc<Estado>,
    /// Cuántas líneas de la consola ya se entregaron.
    consola_leida: usize,
}

/// El objeto de JavaScript de un nodo (el mismo siempre; se crea la primera vez).
pub(crate) fn envolver(m: &mut Motor, est: &Estado, id: NodoId) -> Valor {
    if id.0 == VENTANA {
        return m.global();
    }
    let raiz = m.raiz_anfitrion();
    let nodos = m.leer_nombre(raiz, "nodos");
    if let Some(w) = m.mundo.leer_elemento(nodos, id.0).filter(|v| m.mundo.es_objeto(*v)) {
        return w;
    }
    let proto = match est.doc.borrow().nodo(id).dato {
        Dato::Elemento(_) => "p_elemento",
        Dato::Documento => "p_documento",
        _ => "p_nodo",
    };
    let proto = m.leer_nombre(raiz, proto);
    let w = m.crear_objeto_anfitrion(id.0, proto);
    let marca = m.apilar(w);
    let nodos = m.leer_nombre(m.raiz_anfitrion(), "nodos");
    let w = m.de_pila(marca);
    m.mundo.escribir_elemento(nodos, id.0, w);
    let w = m.de_pila(marca);
    m.soltar_pila(marca);
    w
}

/// El objeto de un nodo, o `null`.
pub(crate) fn envolver_o_nulo(m: &mut Motor, est: &Estado, id: Option<NodoId>) -> Valor {
    id.map_or(Valor::NULO, |id| envolver(m, est, id))
}

/// Un arreglo con los objetos de estos nodos (una foto, no una lista viva).
pub(crate) fn lista(m: &mut Motor, est: &Estado, ids: &[NodoId]) -> Valor {
    let arr = m.crear_arreglo(&[]);
    let marca = m.apilar(arr);
    for &id in ids {
        let w = envolver(m, est, id);
        let arr = m.de_pila(marca);
        m.mundo.empujar(arr, w);
    }
    let arr = m.de_pila(marca);
    m.soltar_pila(marca);
    arr
}

/// Si `tipo` es de un `<script>` clásico que QUIMERA ejecuta.
fn es_javascript(tipo: Option<&str>) -> bool {
    match tipo.map(|t| t.trim().to_ascii_lowercase()) {
        None => true,
        Some(t) => matches!(
            t.as_str(),
            "" | "text/javascript" | "application/javascript" | "text/ecmascript" | "application/ecmascript"
        ),
    }
}

impl Dom {
    /// El DOM de un documento ya analizado, en la dirección `url`, con su `window` y su `document`.
    pub fn nuevo(doc: Documento, url: &str) -> Dom {
        let estado = Rc::new(Estado {
            doc: RefCell::new(doc),
            cambiado: Cell::new(false),
            prevenido: Cell::new(false),
            detenido: Cell::new(false),
            ahora: Cell::new(0.0),
            siguiente_temporizador: Cell::new(1),
            url: String::from(url),
            errores: RefCell::new(Vec::new()),
        });
        let mut motor = Motor::default();
        motor.poner_anfitrion(Rc::new(puente::Puente(estado.clone())));
        puente::preparar(&mut motor, &estado);
        Dom { motor, estado, consola_leida: 0 }
    }

    /// El árbol (para el estilo y la maquetación).
    pub fn documento(&self) -> Ref<'_, Documento> {
        self.estado.doc.borrow()
    }

    /// Si el árbol cambió desde la última vez (y lo olvida).
    pub fn tomar_cambios(&self) -> bool {
        self.estado.cambiado.replace(false)
    }

    /// Las líneas nuevas de `console.log` desde la última vez.
    pub fn consola(&mut self) -> Vec<String> {
        let nuevas = self.motor.salida().get(self.consola_leida..).unwrap_or_default().to_vec();
        self.consola_leida = self.motor.salida().len();
        nuevas
    }

    /// Los errores de JavaScript sin capturar desde la última vez.
    pub fn errores(&self) -> Vec<String> {
        core::mem::take(&mut *self.estado.errores.borrow_mut())
    }

    /// La hora del anfitrión en milisegundos.
    pub fn poner_hora(&self, ms: f64) {
        self.estado.ahora.set(ms);
    }

    /// Ejecuta los `<script>` de la página en orden (los que había al cargarla), y luego lanza
    /// `DOMContentLoaded` y `load`. `externo` da el código de un `src` (o `None` si no llegó).
    /// Los de tipo `module` todavía no se ejecutan.
    pub fn ejecutar_scripts(&mut self, externo: &dyn Fn(&str) -> Option<String>) {
        let scripts: Vec<(Option<String>, Option<String>, NodoId)> = {
            let doc = self.estado.doc.borrow();
            doc.descendientes(Documento::RAIZ)
                .filter_map(|id| doc.elemento(id).filter(|e| e.nombre == "script").map(|e| (e, id)))
                .filter(|(e, _)| es_javascript(e.atributo("type")))
                .map(|(e, id)| (e.atributo("src").map(String::from), e.atributo("type").map(String::from), id))
                .collect()
        };
        for (src, _, id) in scripts {
            let codigo = match &src {
                Some(s) => externo(s),
                None => Some(self.estado.doc.borrow().texto(id)),
            };
            match codigo {
                Some(c) => self.ejecutar(&c),
                None => {
                    self.estado.errores.borrow_mut().push(format!("no llegó el script {}", src.unwrap_or_default()))
                }
            }
        }
        self.despachar(Documento::RAIZ, "DOMContentLoaded");
        self.despachar(NodoId(VENTANA), "load");
    }

    /// Ejecuta código en la página (sus errores van a [`Dom::errores`]).
    pub fn ejecutar(&mut self, codigo: &str) {
        if let Err(e) = self.motor.ejecutar(codigo) {
            self.estado.errores.borrow_mut().push(e.mensaje);
        }
    }

    /// Un clic en `nodo`. Devuelve `true` si un script llamó a `preventDefault()` (el navegador
    /// no debe seguir el enlace ni enviar el formulario).
    pub fn clic(&mut self, nodo: NodoId) -> bool {
        self.despachar(nodo, "click")
    }

    /// Lanza el evento `tipo` en `nodo`, que sube por sus antepasados hasta `window`. Devuelve
    /// si se llamó a `preventDefault()`.
    pub fn despachar(&mut self, nodo: NodoId, tipo: &str) -> bool {
        puente::despachar(&mut self.motor, &self.estado, nodo, tipo)
    }

    /// Ejecuta los temporizadores que ya tocan a la hora `ahora` (ms). Los que se crean mientras
    /// tanto esperan a la siguiente vez.
    pub fn correr_temporizadores(&mut self, ahora: f64) {
        self.estado.ahora.set(ahora);
        puente::correr_temporizadores(&mut self.motor, &self.estado, ahora);
    }

    /// Cuándo toca el próximo temporizador (ms), si hay alguno.
    pub fn proximo_temporizador(&mut self) -> Option<f64> {
        puente::proximo_temporizador(&mut self.motor)
    }
}

#[cfg(test)]
mod pruebas;
