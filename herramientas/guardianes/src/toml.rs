//! Lector mínimo de TOML: solo lo que usan los `Cargo.toml` de QUIMERA y `guardianes.toml`.
//!
//! Entiende tablas (`[a.b]`), arreglos de tablas (`[[a]]`) y claves con texto, entero,
//! booleano o lista de esos (también listas en varias líneas). Las tablas en línea
//! (`{ path = "..." }`) se guardan como [`Dato::Otro`]: de ellas solo importa la clave.

/// Un valor de TOML.
#[derive(Debug, Clone, PartialEq)]
pub enum Dato {
    /// `"texto"`.
    Texto(String),
    /// `42`.
    Entero(i64),
    /// `true` o `false`.
    Bool(bool),
    /// `[a, b]`.
    Lista(Vec<Dato>),
    /// Cualquier otra cosa (tablas en línea, fechas...).
    Otro,
}

/// Una tabla con sus claves en orden.
#[derive(Debug, Clone, Default)]
pub struct Tabla {
    /// Nombre completo (`package.metadata.quimera`); la raíz es `""`.
    pub nombre: String,
    /// Viene de `[[nombre]]`.
    pub arreglo: bool,
    /// Claves tal cual se escribieron (`quimera-valor.workspace`).
    pub claves: Vec<(String, Dato)>,
}

impl Tabla {
    /// El valor de una clave.
    pub fn dato(&self, clave: &str) -> Option<&Dato> {
        self.claves.iter().find(|(k, _)| k == clave).map(|(_, v)| v)
    }

    /// Texto de una clave.
    pub fn texto(&self, clave: &str) -> Option<&str> {
        match self.dato(clave)? {
            Dato::Texto(s) => Some(s),
            _ => None,
        }
    }

    /// Entero de una clave.
    pub fn entero(&self, clave: &str) -> Option<i64> {
        match self.dato(clave)? {
            Dato::Entero(n) => Some(*n),
            _ => None,
        }
    }

    /// Booleano de una clave.
    pub fn booleano(&self, clave: &str) -> Option<bool> {
        match self.dato(clave)? {
            Dato::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Lista de textos de una clave (vacía si no está).
    pub fn textos(&self, clave: &str) -> Vec<String> {
        match self.dato(clave) {
            Some(Dato::Lista(l)) => l
                .iter()
                .filter_map(|d| match d {
                    Dato::Texto(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Busca una tabla (la primera con ese nombre).
pub fn tabla<'a>(tablas: &'a [Tabla], nombre: &str) -> Option<&'a Tabla> {
    tablas.iter().find(|t| t.nombre == nombre)
}

/// Lee un documento TOML como lista de tablas.
pub fn leer(texto: &str) -> Vec<Tabla> {
    let mut tablas = vec![Tabla::default()];
    let mut pendiente = String::new();
    for linea in texto.lines() {
        let linea = sin_comentario(linea);
        if pendiente.is_empty() {
            let l = linea.trim();
            if l.is_empty() {
                continue;
            }
            if let Some(n) = l.strip_prefix("[[").and_then(|r| r.strip_suffix("]]")) {
                tablas.push(Tabla { nombre: n.trim().to_string(), arreglo: true, claves: Vec::new() });
                continue;
            }
            if let Some(n) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                tablas.push(Tabla { nombre: n.trim().to_string(), arreglo: false, claves: Vec::new() });
                continue;
            }
        }
        pendiente.push_str(linea);
        pendiente.push('\n');
        if !equilibrado(&pendiente) {
            continue;
        }
        let entrada = std::mem::take(&mut pendiente);
        if let Some((k, v)) = entrada.split_once('=') {
            let clave = k.trim().trim_matches('"').to_string();
            if let Some(t) = tablas.last_mut() {
                t.claves.push((clave, valor(v.trim())));
            }
        }
    }
    tablas
}

/// Quita el comentario `#` que no esté dentro de una cadena.
fn sin_comentario(linea: &str) -> &str {
    let mut en_cadena = false;
    let mut escapado = false;
    for (i, c) in linea.char_indices() {
        match c {
            '\\' if en_cadena => escapado = !escapado,
            '"' if !escapado => en_cadena = !en_cadena,
            '#' if !en_cadena => return &linea[..i],
            _ => escapado = false,
        }
        if c != '\\' {
            escapado = false;
        }
    }
    linea
}

/// Corchetes y llaves cerrados (fuera de cadenas).
fn equilibrado(s: &str) -> bool {
    let mut nivel = 0i32;
    let mut en_cadena = false;
    let mut previo = ' ';
    for c in s.chars() {
        match c {
            '"' if previo != '\\' => en_cadena = !en_cadena,
            '[' | '{' if !en_cadena => nivel += 1,
            ']' | '}' if !en_cadena => nivel -= 1,
            _ => {}
        }
        previo = c;
    }
    nivel <= 0
}

fn valor(v: &str) -> Dato {
    let v = v.trim();
    if let Some(s) = v.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Dato::Texto(s.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    match v {
        "true" => return Dato::Bool(true),
        "false" => return Dato::Bool(false),
        _ => {}
    }
    if let Ok(n) = v.replace('_', "").parse::<i64>() {
        return Dato::Entero(n);
    }
    if let Some(dentro) = v.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return Dato::Lista(partir(dentro).into_iter().map(valor).collect());
    }
    Dato::Otro
}

/// Parte una lista por las comas de primer nivel (no las de dentro de cadenas o sublistas).
fn partir(s: &str) -> Vec<&str> {
    let mut partes = Vec::new();
    let (mut nivel, mut en_cadena, mut inicio, mut previo) = (0i32, false, 0, ' ');
    for (i, c) in s.char_indices() {
        match c {
            '"' if previo != '\\' => en_cadena = !en_cadena,
            '[' | '{' if !en_cadena => nivel += 1,
            ']' | '}' if !en_cadena => nivel -= 1,
            ',' if !en_cadena && nivel == 0 => {
                partes.push(s[inicio..i].trim());
                inicio = i + 1;
            }
            _ => {}
        }
        previo = c;
    }
    let ultimo = s[inicio..].trim();
    if !ultimo.is_empty() {
        partes.push(ultimo);
    }
    partes
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn lee_un_cargo_toml() {
        let t = leer(
            "[package]\nname = \"x\" # nombre\n\n[package.metadata.quimera]\nnivel = 2\nno_std = true\nexternos = [\"a\", \"b\"]\n\n[dependencies]\nquimera-valor.workspace = true\nboa_gc = { version = \"0.22\" }\n",
        );
        assert_eq!(tabla(&t, "package").unwrap().texto("name"), Some("x"));
        let q = tabla(&t, "package.metadata.quimera").unwrap();
        assert_eq!(q.entero("nivel"), Some(2));
        assert_eq!(q.booleano("no_std"), Some(true));
        assert_eq!(q.textos("externos"), ["a", "b"]);
        let d = tabla(&t, "dependencies").unwrap();
        assert_eq!(d.claves[0].0, "quimera-valor.workspace");
        assert_eq!(d.dato("boa_gc"), Some(&Dato::Otro));
    }

    #[test]
    fn listas_en_varias_lineas_y_arreglos_de_tablas() {
        let t = leer(
            "[workspace]\nmembers = [\n  \"a/b\",\n  \"c\", # comentario\n]\n\n[[revisado]]\nregla = \"A\"\n[[revisado]]\nregla = \"B\"\n",
        );
        assert_eq!(tabla(&t, "workspace").unwrap().textos("members"), ["a/b", "c"]);
        let r: Vec<_> = t.iter().filter(|x| x.arreglo && x.nombre == "revisado").collect();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].texto("regla"), Some("B"));
    }

    #[test]
    fn el_almohadilla_dentro_de_una_cadena_no_es_comentario() {
        let t = leer("a = \"uno # dos\"\n");
        assert_eq!(t[0].texto("a"), Some("uno # dos"));
    }
}
