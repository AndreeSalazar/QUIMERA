//! Las imágenes de una página: se descargan después de la página, en unos pocos hilos, y cada
//! una avisa al llegar. Se decodifican con `quimera-imagen` en el mismo hilo que la descarga.
//!
//! Cada `src` (tal como aparece en el HTML) tiene un número, el de su textura en la GPU. Los
//! píxeles se guardan solo hasta subirlos; después basta el tamaño (para maquetar).

use quimera_html::Documento;
use quimera_imagen::Imagen;
use quimera_red::{Cliente, Url};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Hilos que descargan imágenes a la vez.
const HILOS: usize = 4;

/// Los `src` de los `<img>` del documento, sin repetir y en orden.
pub fn fuentes(doc: &Documento) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for id in doc.descendientes(Documento::RAIZ) {
        let Some(src) = doc.elemento(id).filter(|e| e.nombre == "img").and_then(|e| e.atributo("src")) else {
            continue;
        };
        if !src.trim().is_empty() && !v.iter().any(|s| s == src) {
            v.push(String::from(src));
        }
    }
    v
}

/// Descarga y decodifica una imagen.
pub fn descargar(cliente: &Cliente, base: &Url, src: &str) -> Result<Imagen, String> {
    let url = base.unir(src.trim()).map_err(|e| e.to_string())?;
    let r = cliente.pedir(&url).map_err(|e| e.to_string())?;
    if !r.exito() {
        return Err(format!("el servidor respondió {}", r.estado));
    }
    quimera_imagen::decodificar(&r.cuerpo).map_err(|e| e.to_string())
}

/// Descarga `fuentes` en [`HILOS`] hilos; `avisar` recibe cada una al llegar (en su hilo).
pub fn descargar_todas<F>(cliente: Arc<Cliente>, base: Url, fuentes: Vec<String>, avisar: F)
where
    F: Fn(String, Result<Imagen, String>) + Send + Sync + 'static,
{
    let cola = Arc::new(Mutex::new(fuentes.into_iter().rev().collect::<Vec<_>>()));
    let (base, avisar) = (Arc::new(base), Arc::new(avisar));
    for _ in 0..HILOS {
        let (cola, cliente, base, avisar) = (cola.clone(), cliente.clone(), base.clone(), avisar.clone());
        std::thread::spawn(move || {
            loop {
                let siguiente = cola.lock().ok().and_then(|mut c| c.pop());
                let Some(src) = siguiente else { break };
                let r = descargar(&cliente, &base, &src);
                avisar(src, r);
            }
        });
    }
}

/// Una imagen que llegó.
struct Cargada {
    id: u32,
    ancho: u32,
    alto: u32,
    /// Píxeles aún por subir a la GPU.
    rgba: Option<Vec<u8>>,
}

/// Las imágenes de la página actual.
#[derive(Default)]
pub struct Imagenes {
    cargadas: HashMap<String, Cargada>,
    siguiente: u32,
}

impl Imagenes {
    /// Otra página: se olvidan todas.
    pub fn vaciar(&mut self) {
        self.cargadas.clear();
    }

    /// Llegó una imagen.
    pub fn llegada(&mut self, src: String, imagen: Imagen) {
        self.siguiente += 1;
        let c = Cargada { id: self.siguiente, ancho: imagen.ancho, alto: imagen.alto, rgba: Some(imagen.rgba) };
        self.cargadas.insert(src, c);
    }

    /// Su tamaño propio (para maquetar).
    pub fn tam(&self, src: &str) -> Option<(u32, u32)> {
        self.cargadas.get(src).map(|c| (c.ancho, c.alto))
    }

    /// Su número de textura (para pintar).
    pub fn id(&self, src: &str) -> Option<u32> {
        self.cargadas.get(src).map(|c| c.id)
    }

    /// Las que faltan por subir a la GPU: (id, ancho, alto, píxeles). Se sueltan al devolverlas.
    pub fn por_subir(&mut self) -> Vec<(u32, u32, u32, Vec<u8>)> {
        self.cargadas.values_mut().filter_map(|c| Some((c.id, c.ancho, c.alto, c.rgba.take()?))).collect()
    }

    /// Cuántas hay.
    pub fn total(&self) -> usize {
        self.cargadas.len()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn fuentes_sin_repetir() {
        let doc = quimera_html::analizar("<img src=a.png><p><img src=b.gif><img src=a.png><img src=' '><img>");
        assert_eq!(fuentes(&doc), vec![String::from("a.png"), String::from("b.gif")]);
    }

    #[test]
    fn subir_suelta_los_pixeles() {
        let mut i = Imagenes::default();
        i.llegada(String::from("a.png"), Imagen { ancho: 2, alto: 1, rgba: vec![0; 8] });
        assert_eq!(i.tam("a.png"), Some((2, 1)));
        let subir = i.por_subir();
        assert_eq!((subir.len(), subir[0].0), (1, i.id("a.png").unwrap_or(0)));
        assert!(i.por_subir().is_empty(), "solo una vez");
        assert_eq!(i.tam("a.png"), Some((2, 1)), "el tamaño se queda");
    }
}
