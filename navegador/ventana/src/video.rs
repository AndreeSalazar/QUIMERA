//! El decodificador de un `<video>`: sus muestras H.264 (de MediaSource) esperan comprimidas y
//! solo se decodifica hasta el tiempo que se pide (un fotograma de 640 × 360 en RGBA ya es casi
//! 1 MB). Lo usan el hilo de pintado de la ventana (`compositor.rs`) y la captura.

use crate::imagenes::Imagenes;
use quimera_video::Fotograma;
use quimera_video::mp4::Muestra;
use std::collections::VecDeque;

/// Un `<video>`: sus muestras sin decodificar y los fotogramas que aún no tocan.
#[derive(Default)]
pub struct Decodificado {
    #[cfg(windows)]
    decodificador: Option<quimera_video::DecodificadorH264>,
    /// Muestras aún sin decodificar.
    cola: VecDeque<Muestra>,
    /// Fotogramas decodificados que aún no tocan (el decodificador entrega en orden de
    /// presentación, unos pocos por delante).
    listos: VecDeque<Fotograma>,
    /// El tiempo del fotograma que se pinta ahora.
    pintado: Option<f64>,
}

impl Decodificado {
    /// Muestras nuevas (con la configuración `avcC` de su pista, la primera vez).
    pub fn empujar(&mut self, config: Option<&(Vec<u8>, u32)>, muestras: Vec<Muestra>) {
        #[cfg(windows)]
        if self.decodificador.is_none()
            && let Some((avcc, escala)) = config
        {
            match quimera_video::DecodificadorH264::nuevo(avcc, *escala) {
                Ok(d) => self.decodificador = Some(d),
                Err(e) => eprintln!("vídeo: {e}"),
            }
        }
        #[cfg(not(windows))]
        let _ = config;
        self.cola.extend(muestras);
    }

    /// Cuántas muestras esperan.
    pub fn en_cola(&self) -> usize {
        self.cola.len()
    }

    /// Decodifica hasta tener el fotograma de `tiempo` (o el primero, si aún no empezó) y lo
    /// devuelve si es otro que el pintado. Sin Media Foundation, nunca hay.
    pub fn elegir(&mut self, tiempo: f64) -> Option<Fotograma> {
        // Medio fotograma de margen (a 30 por segundo).
        let hasta = tiempo + 1.0 / 60.0;
        #[cfg(windows)]
        while self.listos.back().is_none_or(|f| f.tiempo <= hasta) {
            let (Some(d), Some(m)) = (self.decodificador.as_mut(), self.cola.pop_front()) else { break };
            match d.decodificar(&m) {
                Ok(fs) => self.listos.extend(fs),
                Err(e) => {
                    eprintln!("vídeo: {e}");
                    break;
                }
            }
        }
        // El último que ya toca; si ninguno toca aún, el primero (la imagen de antes de empezar).
        let mut elegido = None;
        while self.listos.front().is_some_and(|f| f.tiempo <= hasta) {
            elegido = self.listos.pop_front();
        }
        if elegido.is_none() && self.pintado.is_none() {
            elegido = self.listos.front().cloned();
        }
        let f = elegido?;
        if self.pintado == Some(f.tiempo) {
            return None;
        }
        self.pintado = Some(f.tiempo);
        Some(f)
    }
}

/// La captura: el fotograma del tiempo de cada `<video>`, como imagen `quimera-video:N`.
pub fn fotogramas_de_captura(dom: &quimera_dom::Dom, imagenes: &mut Imagenes) {
    for v in dom.tomar_videos() {
        let mut d = Decodificado::default();
        d.empujar(v.config.as_ref(), v.muestras);
        if let Some(f) = d.elegir(v.tiempo) {
            imagenes.fotograma(format!("quimera-video:{}", v.nodo.0), f.ancho, f.alto, f.rgba);
        }
    }
}
