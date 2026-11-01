//! Fuentes de QUIMERA: buscar las del sistema, medir texto y rasterizar letras.
//!
//! Usa `ab_glyph` (Rust puro) para leer TrueType/OpenType. Cada familia genérica (serif, sin
//! serifa, monoespaciada) en sus cuatro caras (normal, negrita, cursiva, negrita cursiva) se
//! busca entre las fuentes del sistema: Windows (`C:\Windows\Fonts`) y Linux (DejaVu,
//! Liberation, Noto). Si a una cara le falta un carácter (`•`, `→`, un emoji), se prueba con
//! las fuentes de reserva.
//!
//! Las fuentes del sistema se cargan **cuando hacen falta**: al empezar solo se anota qué
//! archivos existen, y cada uno se lee la primera vez que se pide una letra suya. Las de
//! reserva (chino, japonés, coreano, emoji: unos 70 MB en Windows) no ocupan memoria hasta
//! que una página las usa.
//!
//! No conoce el CSS: el pintado traduce el estilo a [`Familia`] y caras.

use ab_glyph::{Font, FontVec, GlyphId, PxScale, ScaleFont, point};
use std::path::Path;
use std::sync::OnceLock;

/// Familia genérica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Familia {
    /// Con serifa (Times, Georgia, DejaVu Serif).
    Serif,
    /// Sin serifa (Arial, Segoe UI, DejaVu Sans).
    SinSerif,
    /// Monoespaciada (Consolas, Courier, DejaVu Sans Mono).
    Mono,
}

/// Una cara: su archivo y, cuando ya se leyó, la fuente (`None` si el archivo no sirve).
struct Cara {
    fuente: OnceLock<Option<FontVec>>,
    nombre: String,
}

/// Una fuente desde sus bytes (TrueType u OpenType; de una colección `.ttc`, la primera).
fn leer_fuente(bytes: Vec<u8>) -> Option<FontVec> {
    if bytes.starts_with(b"ttcf") { FontVec::try_from_vec_and_index(bytes, 0) } else { FontVec::try_from_vec(bytes) }
        .ok()
}

/// Número de una cara dentro de [`Fuentes`].
pub type IdCara = usize;

/// Una letra colocada: su cara, su glifo y dónde va su origen (en la línea base).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colocada {
    /// La cara que la dibuja (la pedida o una de reserva).
    pub cara: IdCara,
    /// El glifo.
    pub glifo: u16,
    /// Posición del origen.
    pub x: f32,
}

/// Una letra rasterizada: cobertura de 0 a 255 por píxel.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapa {
    /// Ancho en píxeles.
    pub ancho: u32,
    /// Alto en píxeles.
    pub alto: u32,
    /// Desde el origen de la letra hasta la esquina izquierda del mapa.
    pub izquierda: i32,
    /// Desde la línea base hasta la esquina superior del mapa (negativo: encima).
    pub arriba: i32,
    /// `ancho × alto` bytes, fila a fila.
    pub pixeles: Vec<u8>,
}

/// Las fuentes cargadas.
#[derive(Default)]
pub struct Fuentes {
    caras: Vec<Cara>,
    /// [familia][negrita + 2 × cursiva] → cara.
    tabla: [[Option<IdCara>; 4]; 3],
    /// Para los caracteres que no están en la cara pedida.
    reserva: Vec<IdCara>,
}

/// Archivos a probar para cada familia y cara (normal, negrita, cursiva, negrita cursiva).
const CANDIDATAS: [[&[&str]; 4]; 3] = [
    [
        &[
            "C:/Windows/Fonts/georgia.ttf",
            "C:/Windows/Fonts/times.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif.ttf",
            "/usr/share/fonts/liberation/LiberationSerif-Regular.ttf",
        ],
        &[
            "C:/Windows/Fonts/georgiab.ttf",
            "C:/Windows/Fonts/timesbd.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif-Bold.ttf",
        ],
        &[
            "C:/Windows/Fonts/georgiai.ttf",
            "C:/Windows/Fonts/timesi.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Italic.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif-Italic.ttf",
        ],
        &[
            "C:/Windows/Fonts/georgiaz.ttf",
            "C:/Windows/Fonts/timesbi.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-BoldItalic.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif-BoldItalic.ttf",
        ],
    ],
    [
        &[
            "C:/Windows/Fonts/segoeui.ttf",
            "C:/Windows/Fonts/arial.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        ],
        &[
            "C:/Windows/Fonts/segoeuib.ttf",
            "C:/Windows/Fonts/arialbd.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        ],
        &[
            "C:/Windows/Fonts/segoeuii.ttf",
            "C:/Windows/Fonts/ariali.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Oblique.ttf",
        ],
        &[
            "C:/Windows/Fonts/segoeuiz.ttf",
            "C:/Windows/Fonts/arialbi.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-BoldOblique.ttf",
        ],
    ],
    [
        &[
            "C:/Windows/Fonts/consola.ttf",
            "C:/Windows/Fonts/cour.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        ],
        &[
            "C:/Windows/Fonts/consolab.ttf",
            "C:/Windows/Fonts/courbd.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
        ],
        &[
            "C:/Windows/Fonts/consolai.ttf",
            "C:/Windows/Fonts/couri.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Oblique.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono-Oblique.ttf",
        ],
        &[
            "C:/Windows/Fonts/consolaz.ttf",
            "C:/Windows/Fonts/courbi.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-BoldOblique.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono-BoldOblique.ttf",
        ],
    ],
];

/// Fuentes de reserva para lo que falte (símbolos, flechas, emoji, CJK...).
const RESERVA: &[&str] = &[
    "C:/Windows/Fonts/seguisym.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/seguiemj.ttf",
    // Chino, japonés y coreano (las `.ttc` traen varias fuentes: se usa la primera).
    "C:/Windows/Fonts/msyh.ttc",
    "C:/Windows/Fonts/YuGothR.ttc",
    "C:/Windows/Fonts/malgun.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
];

impl Fuentes {
    /// Carga las fuentes del sistema. Las que no encuentra se quedan sin cara (y se usa otra).
    pub fn del_sistema() -> Fuentes {
        let mut f = Fuentes::default();
        for (fam, caras) in CANDIDATAS.iter().enumerate() {
            for (estilo, rutas) in caras.iter().enumerate() {
                f.tabla[fam][estilo] = rutas.iter().find_map(|r| f.cargar(r));
            }
        }
        f.reserva = RESERVA.iter().filter_map(|r| f.cargar(r)).collect();
        f
    }

    /// Carga una fuente desde sus bytes (TrueType u OpenType); devuelve su número.
    pub fn anadir(&mut self, nombre: &str, bytes: Vec<u8>) -> Option<IdCara> {
        let fuente = leer_fuente(bytes)?;
        self.caras.push(Cara { fuente: OnceLock::from(Some(fuente)), nombre: String::from(nombre) });
        Some(self.caras.len() - 1)
    }

    /// Anota un archivo de fuente si existe (sin leerlo todavía).
    fn cargar(&mut self, ruta: &str) -> Option<IdCara> {
        if let Some(i) = self.caras.iter().position(|c| c.nombre == ruta) {
            return Some(i);
        }
        if !Path::new(ruta).is_file() {
            return None;
        }
        self.caras.push(Cara { fuente: OnceLock::new(), nombre: String::from(ruta) });
        Some(self.caras.len() - 1)
    }

    /// La fuente de una cara, leyendo su archivo la primera vez.
    fn fuente(&self, cara: IdCara) -> Option<&FontVec> {
        let c = self.caras.get(cara)?;
        c.fuente.get_or_init(|| std::fs::read(Path::new(&c.nombre)).ok().and_then(leer_fuente)).as_ref()
    }

    /// Cuántas caras se leyeron ya del disco.
    pub fn leidas(&self) -> usize {
        self.caras.iter().filter(|c| c.fuente.get().is_some()).count()
    }

    /// Cuántas caras hay cargadas.
    pub fn total(&self) -> usize {
        self.caras.len()
    }

    /// El nombre (la ruta) de una cara.
    pub fn nombre(&self, cara: IdCara) -> &str {
        self.caras.get(cara).map_or("", |c| c.nombre.as_str())
    }

    /// La cara para una familia y estilo (la normal de la familia, o cualquier otra, si falta).
    pub fn cara(&self, familia: Familia, negrita: bool, cursiva: bool) -> Option<IdCara> {
        let fila = &self.tabla[familia as usize];
        let i = usize::from(negrita) + 2 * usize::from(cursiva);
        fila[i]
            .or(fila[0])
            .or_else(|| self.tabla.iter().flatten().flatten().next().copied())
            .or_else(|| self.reserva.first().copied())
    }

    /// Ascenso y descenso (positivos) de una cara a un tamaño en píxeles.
    pub fn metricas(&self, cara: IdCara, tam: f32) -> (f32, f32) {
        match self.fuente(cara) {
            Some(f) => {
                let s = f.as_scaled(PxScale::from(tam));
                (s.ascent(), -s.descent())
            }
            None => (tam * 0.8, tam * 0.2),
        }
    }

    /// La cara que tiene este carácter: la pedida o la primera de reserva que lo tenga.
    fn cara_para(&self, cara: IdCara, c: char) -> (IdCara, GlyphId) {
        let tiene = |i: IdCara| self.fuente(i).map(|f| f.glyph_id(c)).filter(|g| g.0 != 0);
        if let Some(g) = tiene(cara) {
            return (cara, g);
        }
        for &r in &self.reserva {
            if let Some(g) = tiene(r) {
                return (r, g);
            }
        }
        (cara, GlyphId(0))
    }

    /// Coloca las letras de un texto desde `x` (con el interletraje de la cara).
    pub fn colocar(&self, cara: IdCara, texto: &str, tam: f32, x: f32) -> Vec<Colocada> {
        let mut v = Vec::with_capacity(texto.len());
        let mut pluma = x;
        let mut anterior: Option<(IdCara, GlyphId)> = None;
        for c in texto.chars() {
            let (k, g) = self.cara_para(cara, c);
            let Some(fuente) = self.fuente(k).map(|f| f.as_scaled(PxScale::from(tam))) else { continue };
            // El interletraje solo entre letras de la misma cara.
            if let Some((_, ga)) = anterior.filter(|(ka, _)| *ka == k) {
                pluma += fuente.kern(ga, g);
            }
            v.push(Colocada { cara: k, glifo: g.0, x: pluma });
            pluma += fuente.h_advance(g);
            anterior = Some((k, g));
        }
        v
    }

    /// El ancho de un texto.
    pub fn ancho(&self, cara: IdCara, texto: &str, tam: f32) -> f32 {
        let colocadas = self.colocar(cara, texto, tam, 0.0);
        match colocadas.last() {
            Some(u) => {
                let avance =
                    self.fuente(u.cara).map_or(0.0, |f| f.as_scaled(PxScale::from(tam)).h_advance(GlyphId(u.glifo)));
                u.x + avance
            }
            None => 0.0,
        }
    }

    /// Rasteriza un glifo a un tamaño (origen en (0, 0), sobre la línea base).
    pub fn rasterizar(&self, cara: IdCara, glifo: u16, tam: f32) -> Option<Mapa> {
        let g = GlyphId(glifo).with_scale_and_position(PxScale::from(tam), point(0.0, 0.0));
        let contorno = self.fuente(cara)?.outline_glyph(g)?;
        let caja = contorno.px_bounds();
        let (ancho, alto) = (caja.width() as u32, caja.height() as u32);
        if ancho == 0 || alto == 0 {
            return None;
        }
        let mut pixeles = vec![0u8; (ancho * alto) as usize];
        contorno.draw(|x, y, cobertura| {
            if let Some(p) = pixeles.get_mut((y * ancho + x) as usize) {
                *p = (cobertura.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        });
        Some(Mapa { ancho, alto, izquierda: caja.min.x as i32, arriba: caja.min.y as i32, pixeles })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Estas pruebas necesitan alguna fuente del sistema; sin ninguna, no hay nada que probar.
    fn fuentes() -> Option<Fuentes> {
        let f = Fuentes::del_sistema();
        (f.total() > 0).then_some(f)
    }

    #[test]
    fn mide_y_rasteriza_con_fuentes_del_sistema() {
        let Some(f) = fuentes() else { return };
        let sans = f.cara(Familia::SinSerif, false, false).expect("una cara");
        let negrita = f.cara(Familia::SinSerif, true, false).expect("una cara");
        let (corto, largo) = (f.ancho(sans, "il", 16.0), f.ancho(sans, "MW", 16.0));
        assert!(corto > 0.0 && largo > corto * 2.0, "{corto} {largo}");
        assert!(f.ancho(negrita, "Hola", 16.0) >= f.ancho(sans, "Hola", 16.0));
        assert!(
            (f.ancho(sans, "Hola", 32.0) - 2.0 * f.ancho(sans, "Hola", 16.0)).abs() < 0.5,
            "el ancho escala con el tamaño"
        );
        let (asc, desc) = f.metricas(sans, 16.0);
        assert!(asc > 8.0 && desc > 1.0 && asc + desc < 30.0, "{asc} {desc}");
        let a = f.colocar(sans, "A", 32.0, 0.0)[0];
        let m = f.rasterizar(a.cara, a.glifo, 32.0).expect("la A se dibuja");
        assert!(m.ancho > 5 && m.alto > 10 && m.arriba < 0, "{m:?}");
        assert!(m.pixeles.iter().any(|&p| p > 200), "tiene píxeles llenos");
        let mono = f.cara(Familia::Mono, false, false).expect("una cara");
        assert!((f.ancho(mono, "iiii", 16.0) - f.ancho(mono, "MMMM", 16.0)).abs() < 0.01, "monoespaciada");
    }

    #[test]
    fn solo_se_leen_las_fuentes_que_hacen_falta() {
        let Some(f) = fuentes() else { return };
        assert_eq!(f.leidas(), 0, "al empezar no se lee ningún archivo");
        let sans = f.cara(Familia::SinSerif, false, false).expect("una cara");
        f.ancho(sans, "Hola", 16.0);
        assert_eq!(f.leidas(), 1, "solo la cara pedida, sin las de reserva");
    }

    #[test]
    fn caracteres_de_otras_escrituras_con_fuentes_de_reserva() {
        let Some(f) = fuentes() else { return };
        let sans = f.cara(Familia::SinSerif, false, false).expect("una cara");
        let cjk = f.colocar(sans, "中文日本語", 16.0, 0.0);
        // Si el sistema tiene una fuente CJK, cada carácter tiene glifo (no el 0, la caja vacía).
        if cjk.iter().any(|l| l.cara != sans) {
            assert!(cjk.iter().all(|l| l.glifo != 0), "{cjk:?}");
        }
    }
}
