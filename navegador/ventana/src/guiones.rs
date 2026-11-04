//! El JavaScript de la página en la ventana: crear su DOM al llegar, ejecutar sus `<script>`,
//! y contar lo que escriben en la consola y los errores.
//!
//! El reloj de la página son los milisegundos desde que llegó (`performance.now()` en un
//! navegador): los temporizadores se miden con él.

use crate::pagina::Pagina;
use quimera_dom::Dom;
use std::time::Instant;

/// La hora para `Date`: milisegundos desde 1970.
/// Nanosegundos desde que arrancó el programa (el cronómetro del perfil).
#[cfg(feature = "perfil")]
fn nanos() -> u64 {
    static ARRANQUE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    ARRANQUE.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

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

/// Tiempo y veces del medidor (para `QUIMERA_VIDEO`).
pub static MEDIDOR_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static MEDIDOR_VECES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Nanosegundos del medidor por parte: copiar el árbol, pseudoelementos, estilos y maquetación.
pub static MEDIDOR_PARTES: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];

/// Con `QUIMERA_TIEMPOS`: cuánto se llevó el medidor desde el informe anterior, y en qué.
pub fn informe_medidor(que: &str) {
    if std::env::var_os("QUIMERA_TIEMPOS").is_none() {
        return;
    }
    use std::sync::atomic::Ordering;
    let medidor = MEDIDOR_NS.swap(0, Ordering::Relaxed) as f64 / 1e6;
    let veces = MEDIDOR_VECES.swap(0, Ordering::Relaxed);
    let parte = |i: usize| MEDIDOR_PARTES[i].swap(0, Ordering::Relaxed) as f64 / 1e6;
    eprintln!(
        "tiempos: medidor ({que}) {medidor:.0} ms en {veces} veces: copiar {:.0}, pseudoelementos {:.0}, estilos {:.0}, maquetar {:.0}",
        parte(0),
        parte(1),
        parte(2),
        parte(3)
    );
}

fn sumar_parte(i: usize, desde: Instant) -> Instant {
    MEDIDOR_PARTES[i].fetch_add(desde.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    Instant::now()
}
struct Cronometro(Instant);
impl Cronometro {
    fn nuevo() -> Cronometro {
        Cronometro(Instant::now())
    }
}
impl Drop for Cronometro {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        MEDIDOR_NS.fetch_add(self.0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        MEDIDOR_VECES.fetch_add(1, Ordering::Relaxed);
    }
}

/// Quien maqueta para las medidas que lee JavaScript (`clientWidth`, `getBoundingClientRect`):
/// la cascada con las hojas de la página y la maquetación con anchos de texto aproximados (las
/// páginas miden sobre todo contenedores, que casi no dependen del texto).
fn medidor(pagina: &Pagina, (ancho, alto): (f32, f32)) -> quimera_dom::Medir {
    let hojas = pagina.hojas.clone();
    let cache = quimera_estilo::CacheHojas::default();
    Box::new(move |doc| {
        let _medida = Cronometro::nuevo();
        let externa = |h: &str| hojas.iter().find(|(x, _)| x == h).map(|(_, css)| css.clone());
        let op = quimera_estilo::Opciones { ancho, alto };
        // Con sus `::before`/`::after` (en una copia: el DOM de la página no los tiene).
        let t = Instant::now();
        let mut doc = doc.clone();
        let t = sumar_parte(0, t);
        quimera_estilo::anadir_seudoelementos(&mut doc, &op, &externa, &cache);
        let t = sumar_parte(1, t);
        let estilos = quimera_estilo::calcular_con_cache(&doc, &op, &externa, &cache);
        let t = sumar_parte(2, t);
        let m = quimera_maquetacion::maquetar(&doc, &estilos, &quimera_maquetacion::Aproximado, ancho, alto);
        sumar_parte(3, t);
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
        #[cfg(feature = "perfil")]
        if std::env::var_os("QUIMERA_PERFIL").is_some() {
            dom.motor.perfilar(nanos);
        }
        let antes = Instant::now();
        dom.ejecutar_scripts(&|src| pagina.script(src));
        if std::env::var_os("QUIMERA_TIEMPOS").is_some() {
            eprintln!(
                "tiempos: DOM {:.0} ms, ejecutar scripts {:.0} ms",
                (antes - inicio).as_secs_f64() * 1e3,
                antes.elapsed().as_secs_f64() * 1e3
            );
            informe_medidor("scripts");
        }
        let mut g = Guion { dom, inicio };
        #[cfg(feature = "perfil")]
        g.informe_perfil("scripts");
        g.informar();
        eprintln!("{}: scripts en {:.1} ms", pagina.url, inicio.elapsed().as_secs_f64() * 1e3);
        g
    }

    /// Con `QUIMERA_PERFIL=1` (y la característica `perfil`): las funciones y las instrucciones
    /// que más tiempo se llevaron desde el informe anterior.
    #[cfg(feature = "perfil")]
    pub fn informe_perfil(&mut self, que: &str) {
        if std::env::var_os("QUIMERA_PERFIL").is_none() {
            return;
        }
        let m = &mut self.dom.motor;
        eprintln!("perfil ({que}): funciones");
        for (f, veces, ns) in m.informe_funciones(30) {
            eprintln!("  {:>8.1} ms {:>10} instr  {f}", ns as f64 / 1e6, veces);
        }
        eprintln!("perfil ({que}): instrucciones");
        for (i, veces, ns) in m.informe_perfil().into_iter().take(15) {
            eprintln!("  {:>8.1} ms {:>10}  {i}", ns as f64 / 1e6, veces);
        }
        m.reiniciar_perfil();
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
