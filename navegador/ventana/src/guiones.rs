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

/// La página con su JavaScript, y desde cuándo corre su reloj.
pub struct Guion {
    pub dom: Dom,
    inicio: Instant,
}

impl Guion {
    /// Crea el DOM de la página y ejecuta sus scripts.
    pub fn preparar(pagina: &Pagina) -> Guion {
        let inicio = Instant::now();
        let mut dom = Dom::nuevo(pagina.documento(), &pagina.url.to_string());
        dom.con_sistema(reloj, entropia);
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
    pub fn adelantar(
        &mut self,
        hasta: f64,
        atender: &dyn Fn(&quimera_dom::PeticionJs) -> Result<quimera_dom::RespuestaJs, String>,
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
            match self.dom.proximo_temporizador() {
                Some(t) if t <= hasta => self.dom.correr_temporizadores(t),
                _ => break,
            }
        }
        self.informar();
    }
}
