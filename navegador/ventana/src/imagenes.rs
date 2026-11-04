//! Las imágenes de una página, trabajando solo en lo que se ve (como los motores de videojuegos).
//!
//! - **Se descargan todas**, en unos pocos hilos, pero se guardan **comprimidas**, con su tamaño
//!   leído de la cabecera (`quimera_imagen::dimensiones`). Con eso ya se maqueta.
//! - **Se decodifican solo las que están cerca de la vista** (en pantalla o a una pantalla), en
//!   hilos aparte, y entonces se suben a la GPU.
//! - **Las que quedan lejos sueltan su textura** (a más de [`LEJOS`] pantallas). Si se vuelve a
//!   ellas, se decodifican otra vez desde los bytes comprimidos, que pesan de 10 a 50 veces menos.
//!
//! Cada `src` (tal como aparece en el HTML) tiene un número, el de su textura en la GPU. También
//! se piden las que añaden los scripts: tras cada maquetación se mira el DOM vivo.

use quimera_html::Documento;
use quimera_imagen::Imagen;
use quimera_maquetacion::{Maquetacion, Pieza};
use quimera_red::bloqueo::Tipo;
use quimera_red::{Cliente, Peticion, Url};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Hilos que descargan imágenes a la vez.
const HILOS: usize = 4;
/// Pantallas de margen alrededor de la vista donde ya se decodifica (para que al desplazar la
/// imagen esté lista).
pub const CERCA: f32 = 1.0;
/// Pantallas de distancia a partir de las que una imagen suelta su textura.
pub const LEJOS: f32 = 3.0;

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

/// Una imagen descargada: sus bytes comprimidos y su tamaño.
pub type Descargada = (Arc<Vec<u8>>, (u32, u32));

/// Descarga una imagen y lee su tamaño (sin decodificarla).
pub fn descargar(cliente: &Cliente, base: &Url, src: &str) -> Result<Descargada, String> {
    let url = base.unir(src.trim()).map_err(|e| e.to_string())?;
    let r = cliente.solicitar(&url, &Peticion::recurso(Tipo::Imagen, base)).map_err(|e| e.to_string())?;
    if !r.exito() {
        return Err(format!("el servidor respondió {}", r.estado));
    }
    match quimera_imagen::dimensiones(&r.cuerpo) {
        Some(tam) => Ok((Arc::new(r.cuerpo), tam)),
        // Un formato que no se lee (WebP, SVG...): el error del decodificador lo dice.
        None => Err(quimera_imagen::decodificar(&r.cuerpo)
            .err()
            .map_or_else(|| String::from("cabecera rota"), |e| e.to_string())),
    }
}

/// Descarga `fuentes` en [`HILOS`] hilos; `avisar` recibe cada una al llegar (en su hilo).
pub fn descargar_todas<F>(cliente: Arc<Cliente>, base: Url, fuentes: Vec<String>, avisar: F)
where
    F: Fn(String, Result<Descargada, String>) + Send + Sync + 'static,
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

/// Decodifica en otro hilo; `avisar` recibe el número de la imagen y el resultado.
pub fn decodificar_aparte<F>(id: u32, bytes: Arc<Vec<u8>>, avisar: F)
where
    F: FnOnce(u32, Result<Imagen, String>) + Send + 'static,
{
    std::thread::spawn(move || {
        let imagen = {
            let _t = crate::trazas::tramo("grafico/imagen", Some("red/red"));
            quimera_imagen::decodificar(&bytes).map_err(|e| e.to_string())
        };
        avisar(id, imagen)
    });
}

/// Los `src` de las imágenes de la maquetación que caen entre `desde` y `hasta` (en píxeles
/// CSS, en vertical).
pub fn entre(m: &Maquetacion, desde: f32, hasta: f32) -> HashSet<String> {
    let mut v = HashSet::new();
    for (i, p) in m.piezas.iter().enumerate() {
        // Las de una capa fija están siempre en la vista.
        let fija = m.fijas.iter().any(|r| r.contains(&i));
        if let Pieza::Imagen { y, alto, src, .. } = p
            && (fija || (y + alto >= desde && *y <= hasta))
        {
            v.insert(src.clone());
        }
    }
    v
}

/// Una imagen que llegó.
struct Cargada {
    id: u32,
    ancho: u32,
    alto: u32,
    /// Los bytes comprimidos, para decodificarla cuando haga falta (otra vez si se soltó).
    bytes: Arc<Vec<u8>>,
    /// Ya está en la GPU.
    en_gpu: bool,
    /// Se está decodificando.
    decodificando: bool,
    /// Un fotograma de vídeo, ya en RGBA: se sube tal cual y no se suelta.
    crudo: Option<Arc<Vec<u8>>>,
    /// Un `<video>` de la ventana: su textura la pinta el compositor (no se decodifica ni se
    /// suelta aquí).
    video: bool,
}

/// Las imágenes de la página actual.
#[derive(Default)]
pub struct Imagenes {
    cargadas: HashMap<String, Cargada>,
    /// Lo ya pedido (llegado, en camino o fallido): no se vuelve a pedir.
    pedidas: HashSet<String>,
    siguiente: u32,
}

impl Imagenes {
    /// Otra página: se olvidan todas.
    pub fn vaciar(&mut self) {
        self.cargadas.clear();
        self.pedidas.clear();
    }

    /// Las imágenes de `doc` que aún no se han pedido (también las que añaden los scripts).
    /// Quedan apuntadas como pedidas.
    pub fn nuevas(&mut self, doc: &Documento) -> Vec<String> {
        let v: Vec<String> = fuentes(doc).into_iter().filter(|s| !self.pedidas.contains(s)).collect();
        self.pedidas.extend(v.iter().cloned());
        v
    }

    /// Las imágenes `data:` de la maquetación que aún no se tienen (los `<svg>` de la página
    /// salen así): se leen ya, sin red; se decodifican luego como las demás, si están cerca.
    pub fn en_linea(&mut self, cliente: &Cliente, base: &Url, m: &Maquetacion) {
        for p in &m.piezas {
            let Pieza::Imagen { src, .. } = p else { continue };
            if !src.starts_with("data:") || self.pedidas.contains(src) {
                continue;
            }
            match descargar(cliente, base, src) {
                Ok(d) => self.llegada(src.clone(), d),
                Err(e) => {
                    eprintln!("imagen data: {e}");
                    self.pedidas.insert(src.clone());
                }
            }
        }
    }

    /// Llegó una imagen (comprimida, con su tamaño).
    pub fn llegada(&mut self, src: String, (bytes, (ancho, alto)): Descargada) {
        self.siguiente += 1;
        let c = Cargada {
            id: self.siguiente,
            ancho,
            alto,
            bytes,
            en_gpu: false,
            decodificando: false,
            crudo: None,
            video: false,
        };
        self.pedidas.insert(src.clone());
        self.cargadas.insert(src, c);
    }

    /// El fotograma que toca de un `<video>` (`src` = `quimera-video:N`), ya en RGBA. Conserva
    /// su número de textura: la GPU solo cambia los píxeles.
    pub fn fotograma(&mut self, src: String, ancho: u32, alto: u32, rgba: Vec<u8>) {
        let id = match self.cargadas.get(&src) {
            Some(c) => c.id,
            None => {
                self.siguiente += 1;
                self.siguiente
            }
        };
        let crudo = Some(Arc::new(rgba));
        let bytes = Arc::new(Vec::new());
        let c = Cargada { id, ancho, alto, bytes, en_gpu: false, decodificando: false, crudo, video: false };
        self.pedidas.insert(src.clone());
        self.cargadas.insert(src, c);
    }

    /// El `<video>` `src` (`quimera-video:N`) de `ancho` × `alto`, cuyas texturas pinta el
    /// compositor: su número de textura, y si es nuevo (hay que maquetar para que su caja lo use).
    pub fn video(&mut self, src: String, ancho: u32, alto: u32) -> (u32, bool) {
        if let Some(c) = self.cargadas.get(&src).filter(|c| (c.ancho, c.alto) == (ancho, alto)) {
            return (c.id, false);
        }
        let id = match self.cargadas.get(&src) {
            Some(c) => c.id,
            None => {
                self.siguiente += 1;
                self.siguiente
            }
        };
        let bytes = Arc::new(Vec::new());
        let c = Cargada { id, ancho, alto, bytes, en_gpu: true, decodificando: false, crudo: None, video: true };
        self.pedidas.insert(src.clone());
        self.cargadas.insert(src, c);
        (id, true)
    }

    /// Los fotogramas de vídeo nuevos que hay que subir a la GPU: (id, ancho, alto, RGBA).
    pub fn fotogramas_por_subir(&mut self) -> Vec<(u32, u32, u32, Arc<Vec<u8>>)> {
        let mut v = Vec::new();
        for c in self.cargadas.values_mut() {
            if let (Some(rgba), false) = (&c.crudo, c.en_gpu) {
                c.en_gpu = true;
                v.push((c.id, c.ancho, c.alto, rgba.clone()));
            }
        }
        v
    }

    /// Su tamaño propio (para maquetar).
    pub fn tam(&self, src: &str) -> Option<(u32, u32)> {
        self.cargadas.get(src).map(|c| (c.ancho, c.alto))
    }

    /// Su número de textura (para pintar; la GPU se salta las que no tiene).
    pub fn id(&self, src: &str) -> Option<u32> {
        self.cargadas.get(src).map(|c| c.id)
    }

    /// Las de `cerca` que hay que decodificar: (id, bytes). Quedan marcadas como en camino.
    pub fn por_decodificar(&mut self, cerca: &HashSet<String>) -> Vec<(u32, Arc<Vec<u8>>)> {
        let mut v = Vec::new();
        for (src, c) in &mut self.cargadas {
            if !c.en_gpu && !c.decodificando && c.crudo.is_none() && !c.video && cerca.contains(src) {
                c.decodificando = true;
                v.push((c.id, c.bytes.clone()));
            }
        }
        v
    }

    /// Terminó de decodificarse la imagen `id`: `true` si sigue siendo de esta página (y se sube).
    pub fn decodificada(&mut self, id: u32, ok: bool) -> bool {
        let Some(c) = self.cargadas.values_mut().find(|c| c.id == id) else { return false };
        c.decodificando = false;
        c.en_gpu = ok;
        ok
    }

    /// Las que están en la GPU pero no en `no_lejos`: sueltan su textura (devuelve sus números).
    pub fn por_soltar(&mut self, no_lejos: &HashSet<String>) -> Vec<u32> {
        let mut v = Vec::new();
        for (src, c) in &mut self.cargadas {
            if c.en_gpu && !c.video && !no_lejos.contains(src) {
                c.en_gpu = false;
                v.push(c.id);
            }
        }
        v
    }

    /// Cuántas hay.
    pub fn total(&self) -> usize {
        self.cargadas.len()
    }

    /// Cuántas están en la GPU ahora.
    pub fn en_gpu(&self) -> usize {
        self.cargadas.values().filter(|c| c.en_gpu).count()
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
    fn decodificar_cerca_y_soltar_lejos() {
        let mut i = Imagenes::default();
        let doc = quimera_html::analizar("<img src=a.png><img src=b.png>");
        assert_eq!(i.nuevas(&doc).len(), 2);
        assert!(i.nuevas(&doc).is_empty(), "no se piden dos veces");
        i.llegada(String::from("a.png"), (Arc::new(vec![1, 2, 3]), (2, 1)));
        i.llegada(String::from("b.png"), (Arc::new(vec![4]), (5, 5)));
        assert_eq!(i.tam("a.png"), Some((2, 1)), "el tamaño ya sirve para maquetar");
        let cerca: HashSet<String> = [String::from("a.png")].into();
        let a = i.id("a.png").unwrap_or(0);
        assert_eq!(i.por_decodificar(&cerca).iter().map(|(id, _)| *id).collect::<Vec<_>>(), [a]);
        assert!(i.por_decodificar(&cerca).is_empty(), "ya está en camino");
        assert!(i.decodificada(a, true));
        assert_eq!(i.en_gpu(), 1);
        assert!(i.por_soltar(&cerca).is_empty(), "sigue cerca");
        assert_eq!(i.por_soltar(&HashSet::new()), [a], "lejos: suelta la textura");
        assert_eq!((i.en_gpu(), i.tam("a.png")), (0, Some((2, 1))), "el tamaño se queda");
        assert_eq!(i.por_decodificar(&cerca).len(), 1, "al volver se decodifica otra vez");
    }
}
