//! El hilo de pintado (el compositor): es dueño de la GPU y pinta por su cuenta, sin esperar al
//! JavaScript ni a la maquetación, que siguen en el hilo de la ventana.
//!
//! - El hilo de la ventana le manda órdenes por un canal (`Compositor`, con los mismos métodos
//!   que la GPU): la escena, las imágenes, el desplazamiento... Ninguna espera respuesta.
//! - Los `<video>` viven aquí. Llegan sus muestras comprimidas y su reloj (tiempo, si corre,
//!   velocidad); este hilo lleva el tiempo, decodifica y sube cada fotograma a su textura.
//!   Los fotogramas nunca cruzan de hilo, y el vídeo sigue fluyendo aunque la página tarde
//!   segundos en un script, mientras haya datos.
//! - Pinta cuando llega algo nuevo y, mientras un vídeo corre, a cada fotograma (la GPU espera
//!   al refresco de la pantalla).

use crate::video::Decodificado;
use quimera_gpu::{Cuadro, Gpu};
use quimera_video::mp4::Muestra;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

/// Lo que necesita el compositor de un `<video>`.
pub struct VideoParaPintar {
    /// La textura en la que se pinta (la de su imagen `quimera-video:N`).
    pub textura: u32,
    /// `avcC` y unidades por segundo de su pista.
    pub config: Option<(Vec<u8>, u32)>,
    /// Muestras nuevas.
    pub muestras: Vec<Muestra>,
    /// Su tiempo según la página, ahora.
    pub tiempo: f64,
    /// Si corre, y a qué velocidad.
    pub corre: bool,
    pub velocidad: f64,
}

enum Orden {
    Redimensionar(u32, u32),
    Atlas(Vec<u8>, u32, u32),
    Escena(Vec<Cuadro>),
    Fijos(Vec<Cuadro>),
    Imagen(u32, u32, u32, Vec<u8>),
    Olvidar(u32),
    OlvidarTodas,
    Dibujar([f32; 4], f32),
    /// Con cuándo se giró la rueda (para `QUIMERA_RUEDA`).
    Desplazar(f32, Instant),
    Video(VideoParaPintar),
}

/// El lado de la ventana: manda órdenes al hilo de pintado (se clona: lo usan el hilo de la
/// ventana, para la rueda, y el de la página).
#[derive(Clone)]
pub struct Compositor {
    tx: Sender<Orden>,
}

impl Compositor {
    /// Arranca el hilo de pintado con esta GPU.
    pub fn nuevo(gpu: Gpu) -> Compositor {
        let (tx, rx) = channel();
        if let Err(e) = std::thread::Builder::new().name(String::from("compositor")).spawn(move || bucle(gpu, &rx)) {
            eprintln!("no se pudo arrancar el hilo de pintado: {e}");
        }
        Compositor { tx }
    }

    fn mandar(&self, o: Orden) {
        let _ = self.tx.send(o);
    }

    pub fn redimensionar(&self, ancho: u32, alto: u32) {
        self.mandar(Orden::Redimensionar(ancho, alto));
    }

    pub fn subir_atlas(&self, pixeles: &[u8], desde: u32, hasta: u32) {
        self.mandar(Orden::Atlas(pixeles.to_vec(), desde, hasta));
    }

    pub fn cargar(&self, cuadros: &[Cuadro]) {
        self.mandar(Orden::Escena(cuadros.to_vec()));
    }

    pub fn cargar_fijos(&self, cuadros: &[Cuadro]) {
        self.mandar(Orden::Fijos(cuadros.to_vec()));
    }

    /// Una imagen ya decodificada (se mueve al otro hilo, sin copiarla).
    pub fn subir_imagen(&self, id: u32, ancho: u32, alto: u32, rgba: Vec<u8>) {
        self.mandar(Orden::Imagen(id, ancho, alto, rgba));
    }

    pub fn olvidar_imagen(&self, id: u32) {
        self.mandar(Orden::Olvidar(id));
    }

    /// Otra página: fuera las imágenes y los vídeos.
    pub fn olvidar_imagenes(&self) {
        self.mandar(Orden::OlvidarTodas);
    }

    /// Pinta con este fondo y este desplazamiento (en cuanto el hilo de pintado pueda).
    pub fn dibujar(&self, fondo: [f32; 4], desp_y: f32) -> Result<(), String> {
        self.mandar(Orden::Dibujar(fondo, desp_y));
        Ok(())
    }

    /// Solo el desplazamiento (la rueda, desde el hilo de la ventana): repinta al momento.
    pub fn desplazar(&self, desp_y: f32) {
        self.mandar(Orden::Desplazar(desp_y, Instant::now()));
    }

    pub fn video(&self, v: VideoParaPintar) {
        self.mandar(Orden::Video(v));
    }
}

/// Un `<video>` en el compositor: su decodificador y su reloj.
struct Video {
    dec: Decodificado,
    /// El tiempo que dijo la página, cuándo lo dijo, si corre y a qué velocidad.
    reloj: (f64, Instant, bool, f64),
}

impl Video {
    fn tiempo(&self) -> f64 {
        let (t, desde, corre, velocidad) = self.reloj;
        if corre { t + desde.elapsed().as_secs_f64() * velocidad } else { t }
    }
}

/// El estado del hilo de pintado.
struct Pintado {
    gpu: Gpu,
    videos: HashMap<u32, Video>,
    fondo: [f32; 4],
    desp: f32,
    /// Ya llegó la primera orden de dibujar (antes no hay nada que enseñar).
    listo: bool,
    /// `QUIMERA_VIDEO=1`: fotogramas pintados en el último segundo.
    cuenta: (Instant, u32),
    /// `QUIMERA_RUEDA=1`: el giro de rueda más antiguo aún sin pintar.
    rueda: Option<Instant>,
}

fn bucle(gpu: Gpu, rx: &Receiver<Orden>) {
    let mut p = Pintado {
        gpu,
        videos: HashMap::new(),
        fondo: [1.0; 4],
        desp: 0.0,
        listo: false,
        cuenta: (Instant::now(), 0),
        rueda: None,
    };
    loop {
        // Con un vídeo corriendo, se despierta para cada fotograma; si no, solo con órdenes.
        let corre = p.videos.values().any(|v| v.reloj.2);
        let espera = if corre { Duration::from_millis(4) } else { Duration::from_secs(3600) };
        let primera = match rx.recv_timeout(espera) {
            Ok(o) => Some(o),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let mut sucio = false;
        for o in primera.into_iter().chain(rx.try_iter()) {
            sucio |= p.aplicar(o);
        }
        sucio |= p.avanzar_videos();
        if sucio
            && p.listo
            && let Err(e) = p.gpu.dibujar(p.fondo, p.desp)
        {
            eprintln!("no se pudo dibujar: {e}");
        }
        if sucio
            && let Some(cuando) = p.rueda.take()
            && std::env::var_os("QUIMERA_RUEDA").is_some()
        {
            eprintln!("rueda: pintada {:.1} ms después del giro", cuando.elapsed().as_secs_f64() * 1e3);
        }
    }
}

impl Pintado {
    /// Aplica una orden; dice si hay que volver a pintar.
    fn aplicar(&mut self, o: Orden) -> bool {
        match o {
            Orden::Redimensionar(w, h) => self.gpu.redimensionar(w, h),
            Orden::Atlas(pixeles, desde, hasta) => self.gpu.subir_atlas(&pixeles, desde, hasta),
            Orden::Escena(c) => self.gpu.cargar(&c),
            Orden::Fijos(c) => self.gpu.cargar_fijos(&c),
            Orden::Imagen(id, w, h, rgba) => self.gpu.subir_imagen(id, w, h, &rgba),
            Orden::Olvidar(id) => self.gpu.olvidar_imagen(id),
            Orden::OlvidarTodas => {
                self.gpu.olvidar_imagenes();
                self.videos.clear();
            }
            Orden::Dibujar(fondo, desp) => {
                (self.fondo, self.desp, self.listo) = (fondo, desp, true);
            }
            Orden::Desplazar(desp, cuando) => {
                self.desp = desp;
                self.rueda = Some(self.rueda.map_or(cuando, |r: Instant| r.min(cuando)));
            }
            Orden::Video(v) => {
                let video = self.videos.entry(v.textura).or_insert_with(|| Video {
                    dec: Decodificado::default(),
                    reloj: (0.0, Instant::now(), false, 1.0),
                });
                video.dec.empujar(v.config.as_ref(), v.muestras);
                video.reloj = (v.tiempo, Instant::now(), v.corre, v.velocidad);
                return false;
            }
        }
        true
    }

    /// El fotograma que toca en cada vídeo, a su textura. Dice si alguno cambió.
    fn avanzar_videos(&mut self) -> bool {
        let mut cambio = false;
        for (&textura, v) in &mut self.videos {
            let t = v.tiempo();
            if let Some(f) = v.dec.elegir(t) {
                self.gpu.subir_imagen(textura, f.ancho, f.alto, &f.rgba);
                cambio = true;
                if std::env::var_os("QUIMERA_VIDEO").is_some() {
                    self.cuenta.1 += 1;
                    if self.cuenta.0.elapsed().as_secs_f64() >= 1.0 {
                        eprintln!(
                            "vídeo: {} fotogramas en el último segundo, va por {t:.2} s ({} muestras en cola)",
                            self.cuenta.1,
                            v.dec.en_cola()
                        );
                        self.cuenta = (Instant::now(), 0);
                    }
                }
            }
        }
        cambio
    }
}
