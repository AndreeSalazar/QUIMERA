//! Pintado de QUIMERA: la lista de dibujo de la maquetación → cuadros para la GPU.
//!
//! - Los rectángulos pasan tal cual (escalados a píxeles de pantalla).
//! - Cada texto se coloca letra a letra con las fuentes de verdad (`quimera-texto`); cada letra
//!   se rasteriza una sola vez por tamaño y se guarda en un **atlas** (una textura de un canal,
//!   llenada por filas). Si el atlas se llena, se vacía y se vuelve a llenar.
//! - [`Medidor`] da a la maquetación las mismas medidas con que luego se dibuja: el texto cabe
//!   donde la maquetación dijo.
//!
//! No toca la GPU: produce cuadros y píxeles del atlas, así se puede probar sin tarjeta gráfica.

use quimera_estilo::{Color, Generica};
use quimera_gpu::Cuadro;
use quimera_maquetacion::{Aproximado, Fuente, Maquetacion, MedirTexto, Pieza};
use quimera_texto::{Familia, Fuentes, IdCara};
use std::collections::HashMap;

/// Donde está una letra en el atlas.
#[derive(Debug, Clone, Copy)]
struct Hueco {
    x: u32,
    y: u32,
    ancho: u32,
    alto: u32,
    izquierda: i32,
    arriba: i32,
}

/// El atlas de letras: píxeles en memoria y qué filas cambiaron desde la última subida.
pub struct Atlas {
    /// Lado en píxeles.
    pub tam: u32,
    /// `tam × tam` bytes de cobertura.
    pub pixeles: Vec<u8>,
    x: u32,
    y: u32,
    alto_fila: u32,
    sucio: Option<(u32, u32)>,
    lleno: bool,
}

impl Atlas {
    fn nuevo(tam: u32) -> Atlas {
        Atlas { tam, pixeles: vec![0; (tam * tam) as usize], x: 0, y: 0, alto_fila: 0, sucio: None, lleno: false }
    }

    fn vaciar(&mut self) {
        self.pixeles.fill(0);
        (self.x, self.y, self.alto_fila, self.lleno) = (0, 0, 0, false);
        self.sucio = Some((0, self.tam));
    }

    /// Reserva un hueco de `ancho × alto` (con un píxel de margen); `None` si no cabe.
    fn reservar(&mut self, ancho: u32, alto: u32) -> Option<(u32, u32)> {
        if ancho + 1 > self.tam || alto + 1 > self.tam {
            return None;
        }
        if self.x + ancho + 1 > self.tam {
            (self.x, self.y, self.alto_fila) = (0, self.y + self.alto_fila, 0);
        }
        if self.y + alto + 1 > self.tam {
            self.lleno = true;
            return None;
        }
        let r = (self.x, self.y);
        self.x += ancho + 1;
        self.alto_fila = self.alto_fila.max(alto + 1);
        let (a, b) = self.sucio.unwrap_or((r.1, r.1));
        self.sucio = Some((a.min(r.1), b.max(r.1 + alto)));
        Some(r)
    }
}

fn familia(g: Generica) -> Familia {
    match g {
        Generica::Serif => Familia::Serif,
        Generica::SinSerif => Familia::SinSerif,
        Generica::Monoespaciada => Familia::Mono,
    }
}

fn cara(f: &Fuentes, fuente: &Fuente<'_>) -> Option<IdCara> {
    f.cara(familia(fuente.generica), fuente.negrita, fuente.cursiva)
}

/// Las medidas de las fuentes de verdad, para la maquetación.
pub struct Medidor<'a> {
    fuentes: &'a Fuentes,
}

impl MedirTexto for Medidor<'_> {
    fn ancho(&self, texto: &str, f: &Fuente<'_>) -> f32 {
        match cara(self.fuentes, f) {
            Some(c) => self.fuentes.ancho(c, texto, f.tam),
            None => Aproximado.ancho(texto, f),
        }
    }

    fn ascenso(&self, f: &Fuente<'_>) -> f32 {
        cara(self.fuentes, f).map_or(f.tam * 0.8, |c| self.fuentes.metricas(c, f.tam).0)
    }

    fn descenso(&self, f: &Fuente<'_>) -> f32 {
        cara(self.fuentes, f).map_or(f.tam * 0.2, |c| self.fuentes.metricas(c, f.tam).1)
    }
}

fn rgba(c: Color) -> [f32; 4] {
    [f32::from(c.r) / 255.0, f32::from(c.g) / 255.0, f32::from(c.b) / 255.0, f32::from(c.a) / 255.0]
}

/// El pintor: fuentes, atlas y las letras ya rasterizadas.
pub struct Pintor {
    fuentes: Fuentes,
    atlas: Atlas,
    /// (cara, glifo, tamaño × 4) → su hueco (`None`: la letra no tiene píxeles, como el espacio).
    letras: HashMap<(IdCara, u16, u32), Option<Hueco>>,
}

impl Pintor {
    /// Un pintor con estas fuentes y un atlas de `tam_atlas × tam_atlas`.
    pub fn nuevo(fuentes: Fuentes, tam_atlas: u32) -> Pintor {
        Pintor { fuentes, atlas: Atlas::nuevo(tam_atlas), letras: HashMap::new() }
    }

    /// Las medidas para maquetar.
    pub fn medidor(&self) -> Medidor<'_> {
        Medidor { fuentes: &self.fuentes }
    }

    /// Las filas del atlas que cambiaron desde la última vez (y las olvida): hay que subirlas.
    pub fn atlas_cambiado(&mut self) -> Option<(&[u8], u32, u32)> {
        let (desde, hasta) = self.atlas.sucio.take()?;
        Some((&self.atlas.pixeles, desde, hasta))
    }

    /// El atlas.
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// Los cuadros de una maquetación, en píxeles de pantalla (`escala` = píxeles por píxel CSS).
    /// `imagen` da el número de textura de cada imagen ya cargada, por su `src`.
    pub fn escena(&mut self, m: &Maquetacion, escala: f32, imagen: &dyn Fn(&str) -> Option<u32>) -> Vec<Cuadro> {
        let cuadros = self.intentar(m, escala, imagen);
        if !self.atlas.lleno {
            return cuadros;
        }
        // El atlas se llenó a medias: se vacía y se pinta todo otra vez desde cero.
        self.letras.clear();
        self.atlas.vaciar();
        self.intentar(m, escala, imagen)
    }

    fn intentar(&mut self, m: &Maquetacion, e: f32, imagen: &dyn Fn(&str) -> Option<u32>) -> Vec<Cuadro> {
        let mut v = Vec::with_capacity(m.piezas.len() * 4);
        for p in &m.piezas {
            match p {
                Pieza::Rect { x, y, ancho, alto, color } => {
                    v.push(Cuadro {
                        x: x * e,
                        y: y * e,
                        ancho: ancho * e,
                        alto: alto * e,
                        color: rgba(*color),
                        letra: None,
                        imagen: None,
                    });
                }
                // Una imagen que aún no llegó no se pinta (como en los navegadores: su hueco queda).
                Pieza::Imagen { x, y, ancho, alto, src } => {
                    if let Some(id) = imagen(src) {
                        v.push(Cuadro {
                            x: x * e,
                            y: y * e,
                            ancho: ancho * e,
                            alto: alto * e,
                            color: [1.0; 4],
                            letra: None,
                            imagen: Some(id),
                        });
                    }
                }
                // La familia con nombre aún no se carga: se usa la genérica.
                Pieza::Texto { x, base, texto, tam, color, negrita, cursiva, generica, .. } => {
                    let fuente = Fuente {
                        tam: tam * e,
                        negrita: *negrita,
                        cursiva: *cursiva,
                        generica: *generica,
                        familias: &[],
                    };
                    self.texto(&mut v, texto, &fuente, x * e, base * e, rgba(*color));
                }
            }
        }
        v
    }

    /// Un texto suelto (la interfaz del navegador: botones, la dirección), ya en píxeles de
    /// pantalla, con su origen en `(x, línea base)`.
    pub fn rotulo(
        &mut self,
        v: &mut Vec<Cuadro>,
        texto: &str,
        tam: f32,
        negrita: bool,
        (x, base): (f32, f32),
        color: [f32; 4],
    ) {
        let fuente = Fuente { tam, negrita, cursiva: false, generica: Generica::SinSerif, familias: &[] };
        self.texto(v, texto, &fuente, x, base, color);
    }

    /// El ancho de un texto suelto (sin serifa) en píxeles de pantalla.
    pub fn ancho_rotulo(&self, texto: &str, tam: f32, negrita: bool) -> f32 {
        let fuente = Fuente { tam, negrita, cursiva: false, generica: Generica::SinSerif, familias: &[] };
        self.medidor().ancho(texto, &fuente)
    }

    fn texto(&mut self, v: &mut Vec<Cuadro>, texto: &str, fuente: &Fuente<'_>, x: f32, base: f32, color: [f32; 4]) {
        let Some(c) = cara(&self.fuentes, fuente) else { return };
        let base = base.round();
        for l in self.fuentes.colocar(c, texto, fuente.tam, x) {
            let Some(h) = self.letra(l.cara, l.glifo, fuente.tam) else { continue };
            let t = self.atlas.tam as f32;
            v.push(Cuadro {
                x: l.x.round() + h.izquierda as f32,
                y: base + h.arriba as f32,
                ancho: h.ancho as f32,
                alto: h.alto as f32,
                color,
                letra: Some([h.x as f32 / t, h.y as f32 / t, (h.x + h.ancho) as f32 / t, (h.y + h.alto) as f32 / t]),
                imagen: None,
            });
        }
    }

    /// El hueco de una letra en el atlas, rasterizándola la primera vez.
    fn letra(&mut self, cara: IdCara, glifo: u16, tam: f32) -> Option<Hueco> {
        let clave = (cara, glifo, (tam * 4.0).round() as u32);
        if let Some(h) = self.letras.get(&clave) {
            return *h;
        }
        let hueco = self.fuentes.rasterizar(cara, glifo, tam).and_then(|m| {
            let (x, y) = self.atlas.reservar(m.ancho, m.alto)?;
            let lado = self.atlas.tam as usize;
            for fila in 0..m.alto as usize {
                let origen = &m.pixeles[fila * m.ancho as usize..(fila + 1) * m.ancho as usize];
                let destino = (y as usize + fila) * lado + x as usize;
                self.atlas.pixeles[destino..destino + m.ancho as usize].copy_from_slice(origen);
            }
            Some(Hueco { x, y, ancho: m.ancho, alto: m.alto, izquierda: m.izquierda, arriba: m.arriba })
        });
        if hueco.is_some() || !self.atlas.lleno {
            self.letras.insert(clave, hueco);
        }
        hueco
    }
}

/// `Color` de CSS a los cuatro flotantes de la GPU.
pub fn color_gpu(c: Color) -> [f32; 4] {
    rgba(c)
}
