//! Valores calculados: lo que queda de cada propiedad tras la cascada, listo para maquetar.
//!
//! Las longitudes relativas a la letra o a la ventana (`em`, `rem`, `vw`) ya están en píxeles;
//! los porcentajes se quedan como están, porque dependen del contenedor (eso lo sabe la
//! maquetación).

use alloc::string::String;
use alloc::vec::Vec;
pub use quimera_css::Color;

/// `display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// `block` (y por ahora `grid`, que se maqueta como bloque).
    Bloque,
    /// `flow-root`: un bloque con su propio contexto (abarca sus flotantes y no se mete debajo
    /// de los de fuera).
    RaizFlujo,
    /// `table` (y `inline-table`, por ahora como bloque).
    Tabla,
    /// `table-row-group`, `table-header-group`, `table-footer-group`.
    GrupoFilas,
    /// `table-row`
    Fila,
    /// `table-cell`
    Celda,
    /// `table-caption`
    Titulo,
    /// `table-column`, `table-column-group` (no se dibujan; su `width` todavía no cuenta).
    Columna,
    /// `flex`: un bloque que coloca a sus hijos con flexbox.
    Flex,
    /// `inline`.
    EnLinea,
    /// `inline-block` (por ahora se maqueta como `inline`).
    BloqueEnLinea,
    /// `list-item`: un bloque con su marcador.
    ElementoDeLista,
    /// `none`: ni él ni sus hijos se dibujan.
    Ninguno,
}

/// `float`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flotar {
    /// `none`
    No,
    /// `left`
    Izquierda,
    /// `right`
    Derecha,
}

/// `clear`: a qué lado de los flotantes no se puede poner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limpiar {
    /// `none`
    No,
    /// `left`
    Izquierda,
    /// `right`
    Derecha,
    /// `both`
    Ambos,
}

/// Una longitud calculada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Medida {
    /// Píxeles.
    Px(f32),
    /// Porcentaje del contenedor.
    Porcentaje(f32),
    /// `auto`.
    Auto,
}

impl Medida {
    /// En píxeles, con `base` como el 100 % (`auto` vale `si_auto`).
    pub fn px(self, base: f32, si_auto: f32) -> f32 {
        match self {
            Medida::Px(x) => x,
            Medida::Porcentaje(p) => base * p / 100.0,
            Medida::Auto => si_auto,
        }
    }
}

/// `text-align`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alinear {
    /// `left` / `start`.
    Izquierda,
    /// `right` / `end`.
    Derecha,
    /// `center`.
    Centro,
    /// `justify` (por ahora como `left`).
    Justificar,
}

/// `white-space`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Espacios {
    /// `normal`: los blancos se juntan y las líneas se parten.
    Normal,
    /// `pre`: todo se respeta y no se parte.
    Pre,
    /// `nowrap`: se juntan pero no se parte.
    SinSalto,
    /// `pre-wrap`: se respetan y se parte.
    PreAjustado,
    /// `pre-line`: se juntan los espacios pero se respetan los saltos de línea.
    PreLinea,
}

/// `line-height`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AltoLinea {
    /// `normal` (1,2 veces la letra).
    Normal,
    /// Un factor de la letra (`1.5`).
    Factor(f32),
    /// Píxeles.
    Px(f32),
}

/// Estilo de un borde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstiloBorde {
    /// `none` / `hidden`: no hay borde (y su ancho cuenta como 0).
    Ninguno,
    /// `solid` (y por ahora `inset`, `outset`, `groove`, `ridge`, `double`).
    Solido,
    /// `dashed`.
    Discontinuo,
    /// `dotted`.
    Punteado,
}

/// Un lado del borde.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Borde {
    /// Ancho en píxeles (el que cuenta: 0 si el estilo es `none`).
    pub ancho: f32,
    /// Estilo.
    pub estilo: EstiloBorde,
    /// Color (`None`: el de `color`).
    pub color: Option<Color>,
}

/// Lo que no tiene borde.
pub const SIN_BORDE: Borde = Borde { ancho: 3.0, estilo: EstiloBorde::Ninguno, color: None };

impl Borde {
    /// El ancho que ocupa (0 sin estilo).
    pub fn ocupa(&self) -> f32 {
        if self.estilo == EstiloBorde::Ninguno { 0.0 } else { self.ancho }
    }
}

/// `list-style-type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoLista {
    /// `disc` •
    Disco,
    /// `circle` ◦
    Circulo,
    /// `square` ▪
    Cuadrado,
    /// `decimal` 1.
    Decimal,
    /// `lower-alpha` a.
    Letra,
    /// `upper-alpha` A.
    LetraMayuscula,
    /// `lower-roman` i.
    Romano,
    /// `none`
    Ninguno,
}

/// Familia genérica de la letra (la que se usa si no hay fuentes propias).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generica {
    /// `serif`
    Serif,
    /// `sans-serif` (y fuentes con nombre desconocidas que suelen serlo)
    SinSerif,
    /// `monospace`
    Monoespaciada,
}

/// El estilo calculado de un elemento.
#[derive(Debug, Clone, PartialEq)]
pub struct Estilo {
    /// `display`
    pub display: Display,
    /// `color`
    pub color: Color,
    /// `background-color`
    pub fondo: Color,
    /// `font-size` en píxeles.
    pub tam_letra: f32,
    /// `font-weight` (400 normal, 700 negrita).
    pub peso: u16,
    /// `font-style: italic`
    pub cursiva: bool,
    /// `font-family` tal cual (la primera que exista, cuando haya fuentes).
    pub familias: Vec<String>,
    /// La genérica a la que cae `font-family`.
    pub generica: Generica,
    /// `text-align`
    pub alinear: Alinear,
    /// `text-align: -webkit-center` (el de `<center>` y `align=center`): además del texto, centra
    /// los bloques de dentro que no ocupan todo el ancho (las tablas, sobre todo). Se hereda.
    pub centrar_bloques: bool,
    /// `line-height`
    pub alto_linea: AltoLinea,
    /// `white-space`
    pub espacios: Espacios,
    /// `text-decoration: underline`
    pub subrayado: bool,
    /// `text-decoration: line-through`
    pub tachado: bool,
    /// `margin` (arriba, derecha, abajo, izquierda).
    pub margen: [Medida; 4],
    /// `padding` (arriba, derecha, abajo, izquierda).
    pub relleno: [Medida; 4],
    /// `border` (arriba, derecha, abajo, izquierda).
    pub borde: [Borde; 4],
    /// `width`
    pub ancho: Medida,
    /// `height`
    pub alto: Medida,
    /// `max-width` (`Auto` = sin límite).
    pub ancho_max: Medida,
    /// `min-width` (`Auto` = el mínimo automático: en un elemento flexible, su contenido).
    pub ancho_min: Medida,
    /// `box-sizing: border-box`: `width`/`height` incluyen relleno y borde. Las medidas en px ya
    /// vienen pasadas a contenido; un porcentaje se resuelve en la maquetación (que conoce el
    /// contenedor) restándoles relleno y borde.
    pub caja_borde: bool,
    /// `min-height`
    pub alto_min: Medida,
    /// `list-style-type`
    pub lista: TipoLista,
    /// `visibility: hidden` lo hace invisible sin quitarle el sitio.
    pub visible: bool,
    /// `fill` (heredado): el relleno de las formas SVG.
    pub relleno_svg: Relleno,
    /// `opacity` efectiva: la suya por la de sus antepasados. Al final de la cascada ya está
    /// aplicada a sus colores (texto, fondo, bordes); con 0, el elemento no se ve (`visible`).
    pub opacidad: f32,
    /// `text-transform: uppercase/lowercase`: `Some(true)` mayúsculas, `Some(false)` minúsculas.
    pub transformar: Option<bool>,
    /// Flexbox (como contenedor y como elemento flexible).
    pub flex: crate::flex::Flex,
    /// Tablas: `border-collapse`, `border-spacing`, `vertical-align`.
    pub tabla: crate::tabla::Tabla,
    /// `overflow` distinto de `visible` (`hidden`, `auto`, `scroll`): crea su propio contexto
    /// de formato. (El contenido todavía no se recorta.)
    pub recorta: bool,
    /// `position`, `top`/`right`/`bottom`/`left` y `z-index`.
    pub colocacion: crate::posicion::Colocacion,
    /// `float`
    pub flotar: Flotar,
    /// `clear`
    pub limpiar: Limpiar,
    /// Las propiedades personalizadas (`--x`) que ve, heredadas y propias (compartidas con el
    /// padre si no declara ninguna).
    pub variables: alloc::rc::Rc<crate::variables::Variables>,
}

/// El valor de `fill`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Relleno {
    /// El inicial: negro.
    Negro,
    /// `currentColor`: el `color` de cada elemento.
    Actual,
    /// Un color.
    Color(Color),
    /// `none`.
    Ninguno,
}

impl Default for Estilo {
    /// Los valores iniciales de CSS (los de la raíz antes de la hoja del navegador).
    fn default() -> Self {
        Estilo {
            display: Display::EnLinea,
            color: Color::NEGRO,
            fondo: Color::TRANSPARENTE,
            tam_letra: 16.0,
            peso: 400,
            cursiva: false,
            familias: Vec::new(),
            generica: Generica::Serif,
            alinear: Alinear::Izquierda,
            centrar_bloques: false,
            alto_linea: AltoLinea::Normal,
            espacios: Espacios::Normal,
            subrayado: false,
            tachado: false,
            margen: [Medida::Px(0.0); 4],
            relleno: [Medida::Px(0.0); 4],
            borde: [SIN_BORDE; 4],
            ancho: Medida::Auto,
            alto: Medida::Auto,
            ancho_max: Medida::Auto,
            ancho_min: Medida::Auto,
            caja_borde: false,
            alto_min: Medida::Px(0.0),
            lista: TipoLista::Disco,
            visible: true,
            opacidad: 1.0,
            relleno_svg: Relleno::Negro,
            transformar: None,
            flex: crate::flex::Flex::default(),
            tabla: crate::tabla::Tabla::default(),
            recorta: false,
            colocacion: crate::posicion::Colocacion::default(),
            flotar: Flotar::No,
            limpiar: Limpiar::No,
            variables: alloc::rc::Rc::default(),
        }
    }
}

impl Estilo {
    /// Un hijo nuevo: hereda lo heredable de su padre y el resto con su valor inicial.
    pub fn heredado(padre: &Estilo) -> Estilo {
        Estilo {
            color: padre.color,
            tam_letra: padre.tam_letra,
            peso: padre.peso,
            cursiva: padre.cursiva,
            familias: padre.familias.clone(),
            generica: padre.generica,
            alinear: padre.alinear,
            centrar_bloques: padre.centrar_bloques,
            alto_linea: padre.alto_linea,
            espacios: padre.espacios,
            lista: padre.lista,
            visible: padre.visible,
            opacidad: padre.opacidad,
            relleno_svg: padre.relleno_svg,
            transformar: padre.transformar,
            // text-decoration no se hereda, pero se propaga al texto de dentro.
            subrayado: padre.subrayado,
            tachado: padre.tachado,
            tabla: padre.tabla.heredada(),
            variables: padre.variables.clone(),
            ..Estilo::default()
        }
    }

    /// Con `box-sizing: border-box`, pasa `width`, `height`, `max-width` y `min-height` en px a
    /// medidas de contenido (restando relleno y borde; un relleno en `%` cuenta 0 aquí).
    pub(crate) fn resolver_caja_borde(&mut self) {
        if !self.caja_borde {
            return;
        }
        let px = |m: Medida| if let Medida::Px(v) = m { v } else { 0.0 };
        let horizontal = px(self.relleno[1]) + px(self.relleno[3]) + self.borde[1].ocupa() + self.borde[3].ocupa();
        let vertical = px(self.relleno[0]) + px(self.relleno[2]) + self.borde[0].ocupa() + self.borde[2].ocupa();
        let menos = |m: &mut Medida, extras: f32| {
            if let Medida::Px(v) = m {
                *v = (*v - extras).max(0.0);
            }
        };
        menos(&mut self.ancho, horizontal);
        menos(&mut self.ancho_max, horizontal);
        menos(&mut self.ancho_min, horizontal);
        menos(&mut self.alto, vertical);
        menos(&mut self.alto_min, vertical);
    }

    /// Lo que restar a un `width` en porcentaje ya resuelto para tener el ancho del contenido:
    /// relleno y borde con `border-box`; nada con `content-box`.
    pub fn extras_si_caja_borde(&self, contenedor: f32) -> f32 {
        if !self.caja_borde {
            return 0.0;
        }
        self.relleno[1].px(contenedor, 0.0)
            + self.relleno[3].px(contenedor, 0.0)
            + self.borde[1].ocupa()
            + self.borde[3].ocupa()
    }

    /// El alto de línea en píxeles.
    pub fn alto_linea_px(&self) -> f32 {
        match self.alto_linea {
            AltoLinea::Normal => self.tam_letra * 1.2,
            AltoLinea::Factor(f) => self.tam_letra * f,
            AltoLinea::Px(p) => p,
        }
    }

    /// Si es negrita (peso 600 o más).
    pub fn negrita(&self) -> bool {
        self.peso >= 600
    }

    /// El color de un borde (el de `color` si no tiene propio).
    pub fn color_borde(&self, lado: usize) -> Color {
        self.borde[lado].color.unwrap_or(self.color)
    }
}
