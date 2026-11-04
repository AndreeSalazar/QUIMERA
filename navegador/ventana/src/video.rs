//! Los `<video>` de la página: sus muestras H.264 (de MediaSource) se decodifican con
//! `quimera-video` y el fotograma que toca según su reloj se pinta como una imagen más
//! (`quimera-video:N`, ver `quimera-maquetacion`).
//!
//! Para no llenar la RAM, las muestras esperan comprimidas y solo se decodifica hasta el tiempo
//! actual (un fotograma de 640 × 360 en RGBA ya es casi 1 MB).

use crate::imagenes::Imagenes;
use std::collections::{HashMap, VecDeque};

/// El estado de un `<video>`.
#[cfg(windows)]
struct Uno {
    decodificador: Option<quimera_video::DecodificadorH264>,
    /// Muestras aún sin decodificar.
    cola: VecDeque<quimera_video::mp4::Muestra>,
    /// Fotogramas decodificados que aún no tocan (el decodificador entrega en orden de
    /// presentación, unos pocos por delante).
    listos: VecDeque<quimera_video::Fotograma>,
    /// El tiempo del fotograma que se pinta ahora.
    pintado: Option<f64>,
}

/// Todos los `<video>` de la página.
#[derive(Default)]
pub struct Videos {
    #[cfg(windows)]
    videos: HashMap<u32, Uno>,
    #[cfg(not(windows))]
    videos: HashMap<u32, ()>,
    /// El tiempo de cada `<video>` la última vez: si cambió, se está reproduciendo.
    tiempos: HashMap<u32, f64>,
    /// Alguno avanzó en la última vuelta (la ventana late a 30 por segundo mientras tanto).
    avanza: bool,
    /// `QUIMERA_VIDEO=1`: cada segundo, cuántos fotogramas se pintaron (y desde cuándo se cuenta).
    cuenta: Option<(std::time::Instant, u32)>,
}

impl Videos {
    /// Si algún `<video>` se está reproduciendo.
    pub fn reproduciendo(&self) -> bool {
        self.avanza
    }

    /// Apunta el tiempo de un `<video>` y dice si avanzó desde la vez anterior.
    fn anotar(&mut self, nodo: u32, tiempo: f64) -> bool {
        self.tiempos.insert(nodo, tiempo).is_some_and(|antes| antes != tiempo)
    }

    /// Recoge las muestras nuevas de la página y deja en `imagenes` el fotograma que toca en cada
    /// `<video>`. Devuelve si alguno cambió (hay que volver a pintar).
    #[cfg(windows)]
    pub fn avanzar(&mut self, dom: &quimera_dom::Dom, imagenes: &mut Imagenes) -> bool {
        let mut cambio = false;
        self.avanza = false;
        for v in dom.tomar_videos() {
            self.avanza |= self.anotar(v.nodo.0, v.tiempo);
            let uno = self.videos.entry(v.nodo.0).or_insert_with(|| Uno {
                decodificador: None,
                cola: VecDeque::new(),
                listos: VecDeque::new(),
                pintado: None,
            });
            if uno.decodificador.is_none()
                && let Some((avcc, escala)) = &v.config
            {
                match quimera_video::DecodificadorH264::nuevo(avcc, *escala) {
                    Ok(d) => uno.decodificador = Some(d),
                    Err(e) => eprintln!("vídeo: {e}"),
                }
            }
            uno.cola.extend(v.muestras);
            if let Some(f) = uno.elegir(v.tiempo) {
                imagenes.fotograma(format!("quimera-video:{}", v.nodo.0), f.ancho, f.alto, f.rgba);
                cambio = true;
                let en_cola = uno.cola.len();
                self.contar(v.tiempo, en_cola);
            }
        }
        cambio
    }

    /// Con `QUIMERA_VIDEO=1`, escribe cada segundo los fotogramas pintados.
    fn contar(&mut self, tiempo: f64, en_cola: usize) {
        if std::env::var_os("QUIMERA_VIDEO").is_none() {
            return;
        }
        let ahora = std::time::Instant::now();
        let (desde, n) = self.cuenta.get_or_insert((ahora, 0));
        *n += 1;
        if ahora.duration_since(*desde).as_secs_f64() >= 1.0 {
            eprintln!("vídeo: {n} fotogramas en el último segundo, va por {tiempo:.2} s ({en_cola} muestras en cola)");
            self.cuenta = Some((ahora, 0));
        }
    }

    /// Sin Media Foundation no hay decodificador: los `<video>` se quedan sin imagen.
    #[cfg(not(windows))]
    pub fn avanzar(&mut self, _dom: &quimera_dom::Dom, _imagenes: &mut Imagenes) -> bool {
        false
    }
}

#[cfg(windows)]
impl Uno {
    /// Decodifica hasta tener el fotograma de `tiempo` (o el primero, si aún no empezó) y lo
    /// devuelve si es otro que el pintado.
    fn elegir(&mut self, tiempo: f64) -> Option<quimera_video::Fotograma> {
        // Medio fotograma de margen (a 30 por segundo).
        let hasta = tiempo + 1.0 / 60.0;
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
