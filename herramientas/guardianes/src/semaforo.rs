//! El semáforo: cada hallazgo es rojo (bloquea) o amarillo (obliga a revisar).

use std::fmt;

/// Color de un hallazgo o de un carril.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Color {
    /// Todo en orden.
    Verde,
    /// Hay que revisarlo y aceptarlo en `guardianes.toml`.
    Amarillo,
    /// Bloquea: hay que arreglarlo.
    Rojo,
}

impl Color {
    /// Solo el círculo.
    pub fn circulo(self) -> &'static str {
        match self {
            Color::Verde => "🟢",
            Color::Amarillo => "🟡",
            Color::Rojo => "🔴",
        }
    }
}

/// Una regla de los guardianes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regla {
    /// Identificador (`R-TAMANO`).
    pub id: &'static str,
    /// Color que da.
    pub color: Color,
    /// Qué exige, en una frase.
    pub exige: &'static str,
    /// Por qué existe.
    pub porque: &'static str,
}

/// Algo que encontró un guardián.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hallazgo {
    /// Regla que lo produce.
    pub regla: &'static Regla,
    /// Crate (vacío si es del proyecto entero).
    pub crate_: String,
    /// Archivo relativo a la raíz (vacío si es del crate entero).
    pub archivo: String,
    /// Línea (0 si no aplica).
    pub linea: usize,
    /// Qué pasa aquí.
    pub mensaje: String,
    /// Motivo de la revisión que lo acepta, si un amarillo fue revisado.
    pub revisado: Option<String>,
}

impl Hallazgo {
    /// Color efectivo: un amarillo revisado ya no cuenta; en modo estricto, uno sin revisar es rojo.
    pub fn color(&self, estricto: bool) -> Color {
        match (self.regla.color, &self.revisado, estricto) {
            (Color::Amarillo, Some(_), _) => Color::Verde,
            (Color::Amarillo, None, true) => Color::Rojo,
            (c, _, _) => c,
        }
    }
}

impl fmt::Display for Hallazgo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.regla.color.circulo(), self.regla.id)?;
        if self.archivo.is_empty() && !self.crate_.is_empty() {
            write!(f, " {}", self.crate_)?;
        }
        if !self.archivo.is_empty() {
            write!(f, " {}", self.archivo)?;
            if self.linea > 0 {
                write!(f, ":{}", self.linea)?;
            }
        }
        write!(f, " — {}", self.mensaje)?;
        if let Some(m) = &self.revisado {
            write!(f, "  [revisado: {m}]")?;
        }
        Ok(())
    }
}

/// Todas las reglas, en el orden en que se explican.
pub const REGLAS: &[Regla] = &[
    Regla {
        id: "R-TAMANO",
        color: Color::Rojo,
        exige: "Ningún archivo .rs pasa de 1000 líneas.",
        porque: "Un archivo enorme es un monolito: nadie lo revisa entero y ahí se esconde la aguja.",
    },
    Regla {
        id: "R-NIVEL",
        color: Color::Rojo,
        exige: "Un crate solo usa crates de QUIMERA de niveles menores.",
        porque: "La jerarquía permite probar y sustituir cada pieza sola; un ciclo la rompe.",
    },
    Regla {
        id: "R-EXTERNO",
        color: Color::Rojo,
        exige: "Solo se usan crates de fuera declarados en `externos`.",
        porque: "Cada dependencia ajena es código que no controlamos.",
    },
    Regla {
        id: "R-NOSTD",
        color: Color::Rojo,
        exige: "Si un crate declara no_std = true, su lib.rs es no_std.",
        porque: "El núcleo tiene que poder vivir en ULTRA sin la biblioteca estándar.",
    },
    Regla {
        id: "R-UNSAFE",
        color: Color::Rojo,
        exige: "No hay unsafe salvo en crates con permite_unsafe = true.",
        porque: "unsafe apaga las garantías de Rust: se concentra en pocos sitios conocidos.",
    },
    Regla {
        id: "R-SAFETY",
        color: Color::Rojo,
        exige: "Cada bloque o impl unsafe lleva antes un comentario // SAFETY:.",
        porque: "Sin el porqué, nadie puede revisar la invariante.",
    },
    Regla {
        id: "R-META",
        color: Color::Rojo,
        exige: "Cada crate declara nivel y capa, y su carpeta coincide con la capa.",
        porque: "Sin metadata el guardián no puede vigilar la jerarquía.",
    },
    Regla {
        id: "R-REVISION",
        color: Color::Rojo,
        exige: "Cada revisión de guardianes.toml corresponde a un amarillo que existe.",
        porque: "Una revisión vieja tapa problemas futuros sin que nadie lo note.",
    },
    Regla {
        id: "A-TAMANO",
        color: Color::Amarillo,
        exige: "Un archivo de más de 700 líneas se acerca al límite.",
        porque: "Mejor partirlo antes de que sea urgente.",
    },
    Regla {
        id: "A-FUNCION",
        color: Color::Amarillo,
        exige: "Una función de más de 80 líneas.",
        porque: "Una función larga hace varias cosas: cuesta leerla y probarla.",
    },
    Regla {
        id: "A-UNWRAP",
        color: Color::Amarillo,
        exige: "unwrap o expect fuera de las pruebas.",
        porque: "Convierte un error esperable en un panic.",
    },
    Regla {
        id: "A-PANIC",
        color: Color::Amarillo,
        exige: "panic!, todo!, unimplemented! o unreachable! fuera de las pruebas.",
        porque: "Un navegador no debe caerse por un dato raro.",
    },
    Regla {
        id: "A-ALLOW",
        color: Color::Amarillo,
        exige: "Un #[allow(...)] que apaga un aviso.",
        porque: "Apagar un aviso es una decisión que debe quedar explicada.",
    },
    Regla {
        id: "A-PENDIENTE",
        color: Color::Amarillo,
        exige: "Un TODO:, FIXME: o XXX: en un comentario.",
        porque: "Lo pendiente se olvida si no está a la vista.",
    },
    Regla {
        id: "A-SINPRUEBAS",
        color: Color::Amarillo,
        exige: "Un crate sin ninguna prueba.",
        porque: "Sin pruebas, un cambio puede romperlo sin que nadie lo sepa.",
    },
    Regla {
        id: "A-CRATE",
        color: Color::Amarillo,
        exige: "Un crate de más de 5000 líneas.",
        porque: "Un crate grande suele esconder dos o más piezas: ULTRA modular.",
    },
];

/// La regla con ese identificador.
pub fn regla(id: &str) -> &'static Regla {
    REGLAS.iter().find(|r| r.id == id).unwrap_or(&REGLAS[0])
}
