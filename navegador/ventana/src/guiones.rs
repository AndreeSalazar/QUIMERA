//! El JavaScript de la página en la ventana: crear su DOM al llegar, ejecutar sus `<script>`,
//! y contar lo que escriben en la consola y los errores.
//!
//! El reloj de la página son los milisegundos desde que llegó (`performance.now()` en un
//! navegador): los temporizadores se miden con él.

use crate::pagina::Pagina;
use quimera_dom::Dom;
use std::time::Instant;

/// La hora para `Date`: milisegundos desde 1970.
fn reloj() -> f64 {
    let ahora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    ahora.map_or(0.0, |d| d.as_secs_f64() * 1e3)
}

/// Azar para `crypto.getRandomValues` y `Math.random`: SipHash con claves que la biblioteca
/// estándar saca del sistema operativo (`RandomState`), sobre un contador.
fn entropia(bytes: &mut [u8]) {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static CONTADOR: AtomicU64 = AtomicU64::new(0);
    let estado = std::collections::hash_map::RandomState::new();
    for trozo in bytes.chunks_mut(8) {
        let mut h = estado.build_hasher();
        h.write_u64(CONTADOR.fetch_add(1, Ordering::Relaxed));
        let n = trozo.len();
        trozo.copy_from_slice(&h.finish().to_le_bytes()[..n]);
    }
}

/// Quien maqueta para las medidas que lee JavaScript (`clientWidth`, `getBoundingClientRect`):
/// la cascada con las hojas de la página y la maquetación con anchos de texto aproximados (las
/// páginas miden sobre todo contenedores, que casi no dependen del texto).
fn medidor(pagina: &Pagina, (ancho, alto): (f32, f32)) -> quimera_dom::Medir {
    let hojas = pagina.hojas.clone();
    let cache = quimera_estilo::CacheHojas::default();
    Box::new(move |doc| {
        let externa = |h: &str| hojas.iter().find(|(x, _)| x == h).map(|(_, css)| css.clone());
        let op = quimera_estilo::Opciones { ancho, alto };
        // Con sus `::before`/`::after` (en una copia: el DOM de la página no los tiene).
        let mut doc = doc.clone();
        quimera_estilo::anadir_seudoelementos(&mut doc, &op, &externa, &cache);
        let estilos = quimera_estilo::calcular_con_cache(&doc, &op, &externa, &cache);
        let m = quimera_maquetacion::maquetar(&doc, &estilos, &quimera_maquetacion::Aproximado, ancho, alto);
        let cajas = m.cajas.into_iter().map(|(id, c)| (id.0, c)).collect();
        quimera_dom::Medidas { cajas, ventana: (ancho, alto) }
    })
}

/// Trae las imágenes nuevas de un documento: cada `src` con su tamaño (`None` si falló).
pub type TraerImagenes<'a> = dyn FnMut(&quimera_html::Documento) -> Vec<(String, Option<(u32, u32)>)> + 'a;

/// La página con su JavaScript, y desde cuándo corre su reloj.
pub struct Guion {
    pub dom: Dom,
    inicio: Instant,
}

impl Guion {
    /// Crea el DOM de la página y ejecuta sus scripts, en una vista de `vista` (ancho, alto).
    pub fn preparar(pagina: &Pagina, vista: (f32, f32)) -> Guion {
        let inicio = Instant::now();
        let mut dom = Dom::nuevo(pagina.documento(), &pagina.url.to_string());
        dom.con_sistema(reloj, entropia);
        // `QUIMERA_LANZADOS=1`: también las excepciones que la página captura y se calla.
        if std::env::var_os("QUIMERA_LANZADOS").is_some() {
            dom.motor.anotar_lanzados();
        }
        dom.con_medidas(vista, medidor(pagina, vista));
        dom.ejecutar_scripts(&|src| pagina.script(src));
        let mut g = Guion { dom, inicio };
        g.informar();
        eprintln!("{}: scripts en {:.1} ms", pagina.url, inicio.elapsed().as_secs_f64() * 1e3);
        g
    }

    /// Milisegundos desde que llegó la página.
    pub fn ahora(&self) -> f64 {
        self.inicio.elapsed().as_secs_f64() * 1e3
    }

    /// Escribe en la terminal lo nuevo de la consola y los errores de la página.
    pub fn informar(&mut self) {
        for l in self.dom.motor.lanzados() {
            eprintln!("lanzado: {l}");
        }
        for l in self.dom.consola() {
            eprintln!("consola: {l}");
        }
        for e in self.dom.errores() {
            eprintln!("error de JavaScript: {e}");
        }
    }

    /// Ejecuta los temporizadores que ya tocan. Devuelve cuándo toca el siguiente (en
    /// `Instant`), si hay.
    pub fn latido(&mut self) -> Option<Instant> {
        let ahora = self.ahora();
        self.dom.correr_temporizadores(ahora);
        self.informar();
        let proximo = self.dom.proximo_temporizador()?;
        Some(self.inicio + std::time::Duration::from_secs_f64(proximo.max(ahora) / 1e3))
    }

    /// Para capturar: adelanta el reloj hasta `hasta` ms ejecutando los temporizadores en orden,
    /// sin esperar (como si hubiera pasado ese tiempo), y atiende al momento lo que la página pida
    /// a la red con `atender`.
    /// Las imágenes nuevas de la página las trae `imagenes` (devuelve cada `src` con su tamaño, o
    /// `None` si falló), y sus `<img>` lanzan `load` antes de seguir con el reloj.
    pub fn adelantar(
        &mut self,
        hasta: f64,
        atender: &dyn Fn(&quimera_dom::PeticionJs) -> Result<quimera_dom::RespuestaJs, String>,
        imagenes: &mut TraerImagenes<'_>,
    ) {
        for _ in 0..10_000 {
            let pedidas = self.dom.tomar_peticiones();
            if !pedidas.is_empty() {
                for p in pedidas {
                    let r = atender(&p);
                    self.dom.responder(p.id, r);
                }
                continue;
            }
            let llegadas = imagenes(&self.dom.documento());
            if !llegadas.is_empty() {
                for (src, tam) in llegadas {
                    self.dom.imagen_cargada(&src, tam);
                }
                continue;
            }
            match self.dom.proximo_temporizador() {
                Some(t) if t <= hasta => self.dom.correr_temporizadores(t),
                _ => break,
            }
        }
        self.informar();
    }
}
