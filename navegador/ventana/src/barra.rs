//! La barra del navegador: ← → ⟳ y el campo de dirección, más la línea de estado de abajo
//! (a dónde lleva el enlace bajo el ratón, o qué se está cargando).
//!
//! Se dibuja con los mismos cuadros y letras que la página, en la capa fija de la GPU (no se
//! desplaza). Las medidas están en píxeles CSS y se multiplican por la escala de la pantalla.

use quimera_gpu::Cuadro;
use quimera_pintar::Pintor;

/// Alto de la barra en píxeles CSS.
pub const ALTO: f32 = 44.0;

const FONDO: [f32; 4] = [0.925, 0.925, 0.94, 1.0];
const LINEA: [f32; 4] = [0.78, 0.78, 0.82, 1.0];
const TEXTO: [f32; 4] = [0.13, 0.13, 0.15, 1.0];
const APAGADO: [f32; 4] = [0.65, 0.65, 0.68, 1.0];
const ACENTO: [f32; 4] = [0.42, 0.18, 0.56, 1.0];

/// Lo que hay bajo un punto de la barra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zona {
    /// El botón ←.
    Atras,
    /// El botón →.
    Adelante,
    /// El botón ⟳.
    Recargar,
    /// El campo de dirección.
    Campo,
}

/// (zona, x, ancho) de los botones, en píxeles CSS.
const BOTONES: [(Zona, f32); 3] = [(Zona::Atras, 8.0), (Zona::Adelante, 44.0), (Zona::Recargar, 80.0)];
const LADO_BOTON: f32 = 32.0;
const CAMPO_X: f32 = 124.0;

/// El estado del campo de dirección.
#[derive(Debug, Default)]
pub struct Barra {
    /// Si se está escribiendo una dirección.
    pub editando: bool,
    /// Lo escrito.
    pub texto: String,
    /// Todo seleccionado (al entrar en el campo): lo primero que se escriba lo sustituye.
    pub seleccionado: bool,
}

/// Lo que la barra necesita saber para dibujarse.
pub struct Vista<'a> {
    /// Ancho de la ventana en píxeles de pantalla.
    pub ancho: f32,
    /// Alto de la ventana en píxeles de pantalla.
    pub alto: f32,
    /// Píxeles de pantalla por píxel CSS.
    pub escala: f32,
    /// La dirección de la página.
    pub url: &'a str,
    /// Si se puede ir atrás y adelante.
    pub atras: bool,
    pub adelante: bool,
    /// Texto de la línea de estado (enlace bajo el ratón, «Cargando...»).
    pub estado: Option<&'a str>,
}

impl Barra {
    /// Empieza a escribir en el campo, con la dirección actual seleccionada.
    pub fn enfocar(&mut self, url: &str) {
        self.editando = true;
        self.texto = String::from(url);
        self.seleccionado = true;
    }

    /// Escribe texto en el campo.
    pub fn escribir(&mut self, s: &str) {
        if self.seleccionado {
            self.texto.clear();
            self.seleccionado = false;
        }
        self.texto.extend(s.chars().filter(|c| !c.is_control()));
    }

    /// Borra hacia atrás (todo si estaba seleccionado).
    pub fn borrar(&mut self) {
        if self.seleccionado {
            self.texto.clear();
            self.seleccionado = false;
        } else {
            self.texto.pop();
        }
    }

    /// Qué hay en el punto (x, y) en píxeles CSS, si está dentro de la barra.
    pub fn zona(x: f32, y: f32, ancho_css: f32) -> Option<Zona> {
        if !(0.0..ALTO).contains(&y) {
            return None;
        }
        for (z, bx) in BOTONES {
            if x >= bx && x < bx + LADO_BOTON {
                return Some(z);
            }
        }
        (x >= CAMPO_X && x < ancho_css - 12.0).then_some(Zona::Campo)
    }

    /// Los cuadros de la barra y de la línea de estado.
    pub fn cuadros(&self, p: &mut Pintor, v: &Vista<'_>) -> Vec<Cuadro> {
        let e = v.escala;
        let mut c = Vec::new();
        let rect = |c: &mut Vec<Cuadro>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]| {
            c.push(Cuadro {
                x: x * e,
                y: y * e,
                ancho: w * e,
                alto: h * e,
                color,
                letra: None,
                imagen: None,
                fijo: false,
            });
        };
        rect(&mut c, 0.0, 0.0, v.ancho / e, ALTO, FONDO);
        rect(&mut c, 0.0, ALTO - 1.0, v.ancho / e, 1.0, LINEA);
        for (z, bx) in BOTONES {
            let (simbolo, activo) = match z {
                Zona::Atras => ("←", v.atras),
                Zona::Adelante => ("→", v.adelante),
                _ => ("⟳", true),
            };
            let w = p.ancho_rotulo(simbolo, 20.0 * e, false);
            let x = (bx + LADO_BOTON / 2.0) * e - w / 2.0;
            p.rotulo(&mut c, simbolo, 20.0 * e, false, (x, 29.0 * e), if activo { TEXTO } else { APAGADO });
        }
        self.campo(p, v, &mut c);
        if let Some(estado) = v.estado {
            estado_abajo(p, v, &mut c, estado);
        }
        c
    }

    fn campo(&self, p: &mut Pintor, v: &Vista<'_>, c: &mut Vec<Cuadro>) {
        let e = v.escala;
        let (x, y, w, h) = (CAMPO_X * e, 7.0 * e, v.ancho - (CAMPO_X + 12.0) * e, 30.0 * e);
        let borde = if self.editando { ACENTO } else { LINEA };
        c.push(Cuadro { x, y, ancho: w, alto: h, color: borde, letra: None, imagen: None, fijo: false });
        let grosor = if self.editando { 2.0 * e } else { e };
        c.push(Cuadro {
            x: x + grosor,
            y: y + grosor,
            ancho: w - 2.0 * grosor,
            alto: h - 2.0 * grosor,
            color: [1.0; 4],
            letra: None,
            imagen: None,
            fijo: false,
        });
        let tam = 14.0 * e;
        let margen = 10.0 * e;
        let caben = w - 2.0 * margen;
        let texto = if self.editando { self.texto.as_str() } else { v.url };
        // Si no cabe: al escribir se ve el final (donde está el cursor); si no, el principio.
        let visible = recortar(p, texto, tam, caben, self.editando);
        if self.editando && self.seleccionado && !visible.is_empty() {
            let ancho = p.ancho_rotulo(&visible, tam, false);
            c.push(Cuadro {
                x: x + margen,
                y: y + 6.0 * e,
                ancho,
                alto: h - 12.0 * e,
                color: [0.8, 0.72, 0.9, 1.0],
                letra: None,
                imagen: None,
                fijo: false,
            });
        }
        p.rotulo(c, &visible, tam, false, (x + margen, y + 20.0 * e), TEXTO);
        if self.editando && !self.seleccionado {
            let fin = x + margen + p.ancho_rotulo(&visible, tam, false);
            c.push(Cuadro {
                x: fin + e,
                y: y + 6.0 * e,
                ancho: (1.5 * e).max(1.0),
                alto: h - 12.0 * e,
                color: ACENTO,
                letra: None,
                imagen: None,
                fijo: false,
            });
        }
    }
}

/// La línea de estado: abajo a la izquierda, como en los navegadores.
fn estado_abajo(p: &mut Pintor, v: &Vista<'_>, c: &mut Vec<Cuadro>, estado: &str) {
    let e = v.escala;
    let tam = 12.0 * e;
    let texto = recortar(p, estado, tam, v.ancho * 0.6, false);
    let w = p.ancho_rotulo(&texto, tam, false) + 16.0 * e;
    let h = 22.0 * e;
    let y = v.alto - h;
    c.push(Cuadro {
        x: 0.0,
        y: y - e,
        ancho: w + e,
        alto: h + e,
        color: LINEA,
        letra: None,
        imagen: None,
        fijo: false,
    });
    c.push(Cuadro {
        x: 0.0,
        y,
        ancho: w,
        alto: h,
        color: [0.97, 0.97, 0.98, 1.0],
        letra: None,
        imagen: None,
        fijo: false,
    });
    p.rotulo(c, &texto, tam, false, (8.0 * e, y + 15.0 * e), TEXTO);
}

/// El texto que cabe en `ancho` (con «…» delante o detrás de lo que se corta).
fn recortar(p: &Pintor, texto: &str, tam: f32, ancho: f32, final_visible: bool) -> String {
    if p.ancho_rotulo(texto, tam, false) <= ancho {
        return String::from(texto);
    }
    let letras: Vec<char> = texto.chars().collect();
    let (mut bajo, mut alto) = (0, letras.len());
    // La mayor cantidad de letras que cabe junto al «…» (búsqueda binaria).
    while bajo < alto {
        let medio = (bajo + alto).div_ceil(2);
        let prueba: String = if final_visible {
            core::iter::once('…').chain(letras[letras.len() - medio..].iter().copied()).collect()
        } else {
            letras[..medio].iter().copied().chain(core::iter::once('…')).collect()
        };
        if p.ancho_rotulo(&prueba, tam, false) <= ancho {
            bajo = medio;
        } else {
            alto = medio - 1;
        }
    }
    if final_visible {
        core::iter::once('…').chain(letras[letras.len() - bajo..].iter().copied()).collect()
    } else {
        letras[..bajo].iter().copied().chain(core::iter::once('…')).collect()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn zonas_y_edicion() {
        assert_eq!(Barra::zona(10.0, 20.0, 1000.0), Some(Zona::Atras));
        assert_eq!(Barra::zona(90.0, 20.0, 1000.0), Some(Zona::Recargar));
        assert_eq!(Barra::zona(500.0, 20.0, 1000.0), Some(Zona::Campo));
        assert_eq!(Barra::zona(500.0, 60.0, 1000.0), None, "debajo de la barra es la página");
        let mut b = Barra::default();
        b.enfocar("https://example.com/");
        b.escribir("info.cern.ch");
        assert_eq!(b.texto, "info.cern.ch", "lo seleccionado se sustituye");
        b.borrar();
        b.escribir("\u{8}x");
        assert_eq!(b.texto, "info.cern.cx", "los caracteres de control no se escriben");
        b.enfocar("abc");
        b.borrar();
        assert_eq!(b.texto, "", "borrar con todo seleccionado lo vacía");
    }
}
