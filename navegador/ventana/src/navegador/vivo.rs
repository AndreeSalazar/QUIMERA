//! La parte viva de la página: sus scripts, lo que pide a la red, sus temporizadores y sus
//! `<video>`.

use super::{Evento, Navegador};
use crate::guiones::Guion;
use std::time::Instant;

impl Navegador {
    /// Ejecuta los scripts de la página (ya pintada sin ellos) y la vuelve a maquetar.
    pub(super) fn ejecutar_scripts(&mut self) {
        let Some(pagina) = &self.pagina else { return };
        // La vista en píxeles CSS (sin la barra), para las medidas que lee la página.
        let e = self.escala();
        let vista = self.abierta.as_ref().map_or((1024.0, 700.0), |a| {
            let t = a.ventana.inner_size();
            (t.width as f32 / e, (t.height as f32 - self.alto_barra()) / e)
        });
        self.guion = Some({
            let _t = crate::trazas::tramo("motor-js/vm", Some("web/dom"));
            Guion::preparar(pagina, vista)
        });
        self.estatico = None;
        self.atender_peticiones();
        self.maquetar();
    }

    /// Lo que el JavaScript de la página pidió a la red, en otros hilos (con las reglas de origen
    /// de `peticiones.rs`). Cada respuesta llega como `Evento::Respuesta`.
    pub fn atender_peticiones(&mut self) {
        let Some(g) = &self.guion else { return };
        let Some(pagina) = &self.pagina else { return };
        for p in g.dom.tomar_peticiones() {
            let (cliente, proxy, id) = (self.cliente.clone(), self.proxy.clone(), self.id_carga);
            let (url, base) = (pagina.url.clone(), pagina.base.clone());
            std::thread::spawn(move || {
                let _t = crate::trazas::tramo("red/red", Some("motor-js/vm"));
                let resultado = crate::peticiones::atender(&cliente, &url, &base, &p);
                let _ = proxy.send_event(Evento::Respuesta { id, peticion: p.id, resultado });
            });
        }
    }

    /// Si JavaScript cambió el árbol, se vuelve a maquetar.
    pub fn tras_javascript(&mut self) {
        if self.guion.as_ref().is_some_and(|g| g.dom.tomar_cambios()) {
            self.maquetar();
        }
    }

    /// Ejecuta los temporizadores de la página que ya tocan; devuelve cuándo toca el siguiente.
    pub fn latido(&mut self) -> Option<Instant> {
        let t0 = Instant::now();
        let proximo = {
            let _t = crate::trazas::tramo("motor-js/vm", Some("web/dom"));
            self.guion.as_mut()?.latido()
        };
        let t1 = Instant::now();
        self.tras_javascript();
        self.atender_peticiones();
        let t2 = Instant::now();
        self.avanzar_videos();
        if std::env::var_os("QUIMERA_VIDEO").is_some() {
            let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1e3;
            let total = ms(t0, Instant::now());
            if total > 20.0 {
                use std::sync::atomic::Ordering;
                let medidor = crate::guiones::MEDIDOR_NS.swap(0, Ordering::Relaxed) as f64 / 1e6;
                let veces = crate::guiones::MEDIDOR_VECES.swap(0, Ordering::Relaxed);
                eprintln!(
                    "latido {total:.0} ms: js {:.0} (medidor {medidor:.0} en {veces} veces), maquetar {:.0}, vídeo {:.0}",
                    ms(t0, t1),
                    ms(t1, t2),
                    ms(t2, Instant::now())
                );
            }
        }
        // Con vídeos, cada 100 ms como mucho, para pasarle al compositor sus muestras y su reloj
        // (él pinta cada fotograma por su cuenta).
        if self.hay_videos {
            let pronto = Instant::now() + std::time::Duration::from_millis(100);
            return Some(proximo.map_or(pronto, |p| p.min(pronto)));
        }
        proximo
    }

    /// Pasa al compositor las muestras nuevas y el reloj de cada `<video>`: él los decodifica y
    /// pinta. La primera vez que un vídeo tiene tamaño se maqueta, para que su caja use su textura.
    fn avanzar_videos(&mut self) {
        let (Some(g), Some(a)) = (&self.guion, &self.abierta) else { return };
        let mut maquetar = false;
        for v in g.dom.tomar_videos() {
            self.hay_videos = true;
            let (ancho, alto) = (u32::from(v.tam.0), u32::from(v.tam.1));
            if ancho == 0 || alto == 0 {
                continue;
            }
            let (textura, nuevo) = self.imagenes.video(format!("quimera-video:{}", v.nodo.0), ancho, alto);
            maquetar |= nuevo;
            a.gpu.video(crate::compositor::VideoParaPintar {
                textura,
                config: v.config,
                muestras: v.muestras,
                tiempo: v.tiempo,
                corre: v.corre,
                velocidad: v.velocidad,
            });
        }
        if maquetar {
            self.imagenes_nuevas = true;
            a.ventana.request_redraw();
        }
    }
}
