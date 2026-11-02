//! `RegExp` (`exec`, `test`, `toString`) y los métodos de cadenas que la usan (`replace`,
//! `match`, `search`, `split`).
//!
//! Cada par distinto (patrón, banderas) se compila una sola vez con `quimera-regex`: un
//! literal dentro de un bucle crea un objeto nuevo en cada vuelta (como manda JavaScript),
//! pero todos comparten el mismo programa. El motor trabaja en bytes UTF-8; los índices de
//! JavaScript (`index`, `lastIndex`) cuentan unidades UTF-16 y se convierten aquí.

use super::{arg, constructor, definir_nativa};
use crate::llamadas::Llamada;
use crate::{Motor, R_ARREGLO, R_CADENA, R_REGEXP, RegexCompilada, operaciones as op};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_objetos::{Clase, Valor};

/// El constructor `RegExp` (admite `new`).
pub(super) const REGEXP: u32 = 500;
const EXEC: u32 = 501;
const TEST: u32 = 502;
const TO_STRING: u32 = 503;
const REPLACE: u32 = 510;
const MATCH: u32 = 511;
const SEARCH: u32 = 512;

/// Los grupos de un encaje, en bytes (`None` si el grupo no participó).
type Grupos = Vec<Option<(usize, usize)>>;

pub(super) fn preparar(m: &mut Motor) {
    constructor(m, "RegExp", REGEXP, R_REGEXP);
    for (nombre, id) in [("exec", EXEC), ("test", TEST), ("toString", TO_STRING)] {
        definir_nativa(m, R_REGEXP, nombre, id);
    }
    for (nombre, id) in [("replace", REPLACE), ("match", MATCH), ("search", SEARCH)] {
        definir_nativa(m, R_CADENA, nombre, id);
    }
}

/// Índice UTF-16 de una posición en bytes.
fn a_utf16(t: &str, byte: usize) -> usize {
    if t.is_ascii() { byte } else { t[..byte].encode_utf16().count() }
}

/// Posición en bytes de un índice UTF-16 (`None` si se pasa del final).
fn a_byte(t: &str, u: usize) -> Option<usize> {
    if t.is_ascii() {
        return (u <= t.len()).then_some(u);
    }
    let mut unidades = 0;
    for (b, c) in t.char_indices() {
        if unidades >= u {
            return Some(b);
        }
        unidades += c.len_utf16();
    }
    (unidades >= u).then_some(t.len())
}

/// La posición siguiente a `pos` (un carácter más allá).
fn avanzar(t: &str, pos: usize) -> usize {
    pos + t[pos..].chars().next().map_or(1, char::len_utf8)
}

impl Motor {
    /// Compila (o encuentra ya compilada) la expresión `patron`/`banderas` (ids de cadenas).
    fn compilar_regex(&mut self, patron: u32, banderas: u32) -> Result<u32, Valor> {
        if let Some(&i) = self.indice_regex.get(&(patron, banderas)) {
            return Ok(i);
        }
        let fuente = String::from(self.mundo.cadenas.texto(patron));
        let texto_banderas = String::from(self.mundo.cadenas.texto(banderas));
        let re = match quimera_regex::Regex::nueva(&fuente, &texto_banderas) {
            Ok(re) => re,
            Err(e) => {
                let mensaje = alloc::format!("expresión regular no válida /{fuente}/{texto_banderas}: {}", e.mensaje);
                return Err(self.error("SyntaxError", &mensaje));
            }
        };
        let i = self.regexes.len() as u32;
        self.regexes.push(RegexCompilada { re, fuente, banderas: texto_banderas });
        self.indice_regex.insert((patron, banderas), i);
        Ok(i)
    }

    /// Un objeto `RegExp` nuevo, con `lastIndex = 0`.
    pub(crate) fn crear_regex(&mut self, patron: u32, banderas: u32) -> Result<Valor, Valor> {
        let i = self.compilar_regex(patron, banderas)?;
        let o = self.mundo.crear_regexp(i, self.pila(R_REGEXP));
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(o);
        let clave = self.mundo.cadenas.internar("lastIndex");
        self.mundo.escribir(o, clave, Valor::entero(0));
        let o = self.pila(marca);
        self.mundo.m.soltar_raices(marca);
        Ok(o)
    }

    /// `source`, `flags`, `global`, `dotAll`... de una expresión regular: se calculan al leerlos
    /// (no son propiedades del objeto: crear cada `/re/` no las paga).
    pub(crate) fn propiedad_regexp(&mut self, obj: Valor, clave: u32) -> Option<Valor> {
        let i = self.mundo.regexp(obj)? as usize;
        let nombre = self.mundo.cadenas.texto(clave);
        let bandera = match nombre {
            "global" => 'g',
            "ignoreCase" => 'i',
            "multiline" => 'm',
            "dotAll" => 's',
            "unicode" => 'u',
            "sticky" => 'y',
            "hasIndices" => 'd',
            "unicodeSets" => 'v',
            "source" => {
                let f = self.regexes.get(i)?.fuente.clone();
                return Some(self.mundo.cadena(if f.is_empty() { "(?:)" } else { &f }));
            }
            "flags" => {
                let b = &self.regexes.get(i)?.banderas;
                let orden: String = "dgimsuvy".chars().filter(|c| b.contains(*c)).collect();
                return Some(self.mundo.cadena(&orden));
            }
            _ => return None,
        };
        Some(Valor::booleano(self.regexes.get(i)?.banderas.contains(bandera)))
    }

    /// El primer encaje de la expresión `i` desde `desde` (bytes).
    fn buscar_regex(&mut self, i: u32, texto: &str, desde: usize) -> Option<Grupos> {
        let re = &self.regexes[i as usize].re;
        if !re.buscar(texto, desde, &mut self.memoria_regex) {
            return None;
        }
        Some((0..=re.grupos()).map(|g| self.memoria_regex.captura(g)).collect())
    }

    fn last_index(&mut self, re: Valor) -> f64 {
        let clave = self.mundo.cadenas.internar("lastIndex");
        let v = self.mundo.leer(re, clave).unwrap_or(Valor::INDEFINIDO);
        let n = op::a_numero(&self.mundo, v);
        if n.is_nan() || n < 0.0 { 0.0 } else { n }
    }

    fn fijar_last_index(&mut self, re: Valor, n: usize) {
        let clave = self.mundo.cadenas.internar("lastIndex");
        self.mundo.escribir(re, clave, Valor::numero(n as f64));
    }
}

/// `este` como expresión regular: su número de programa.
fn expresion(m: &mut Motor, v: Valor) -> Result<u32, Valor> {
    m.mundo.regexp(v).ok_or_else(|| m.error("TypeError", "el método necesita una expresión regular"))
}

/// Un valor como texto propio (copia: las llamadas de JavaScript necesitan `&mut Motor`).
fn texto_de(m: &Motor, v: Valor) -> String {
    match m.mundo.texto(v) {
        Some(s) => String::from(s),
        None => op::a_texto(&m.mundo, v),
    }
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    match id {
        REGEXP => construir(m, ll),
        EXEC | TEST => {
            let re = m.pila(ll.este);
            let texto = texto_de(m, arg(m, ll, 0));
            ejecutar(m, re, &texto, id == TEST)
        }
        TO_STRING => {
            let i = expresion(m, m.pila(ll.este))?;
            let c = &m.regexes[i as usize];
            let texto = alloc::format!("/{}/{}", c.fuente, c.banderas);
            Ok(m.mundo.cadena(&texto))
        }
        REPLACE => reemplazar(m, ll),
        MATCH => coincidir(m, ll),
        _ => {
            let texto = texto_de(m, m.pila(ll.este));
            let i = expresion_o_nueva(m, arg(m, ll, 0))?;
            Ok(match m.buscar_regex(i, &texto, 0) {
                Some(g) => Valor::entero(a_utf16(&texto, g[0].map_or(0, |x| x.0)) as i32),
                None => Valor::entero(-1),
            })
        }
    }
}

/// El programa de un argumento: una expresión regular, o un texto que se compila como patrón.
fn expresion_o_nueva(m: &mut Motor, v: Valor) -> Result<u32, Valor> {
    if let Some(i) = m.mundo.regexp(v) {
        return Ok(i);
    }
    let patron = texto_de(m, v);
    let (p, b) = (m.mundo.cadenas.internar(&patron), m.mundo.cadenas.internar(""));
    m.compilar_regex(p, b)
}

/// `new RegExp(patrón, banderas)` y `RegExp(...)`.
fn construir(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let (p, b) = (arg(m, ll, 0), arg(m, ll, 1));
    let (patron, banderas) = match m.mundo.regexp(p) {
        Some(i) => {
            let c = &m.regexes[i as usize];
            let banderas = if b == Valor::INDEFINIDO { c.banderas.clone() } else { texto_de(m, b) };
            (c.fuente.clone(), banderas)
        }
        None => {
            let patron = if p == Valor::INDEFINIDO { String::from("(?:)") } else { texto_de(m, p) };
            (patron, if b == Valor::INDEFINIDO { String::new() } else { texto_de(m, b) })
        }
    };
    let (p, b) = (m.mundo.cadenas.internar(&patron), m.mundo.cadenas.internar(&banderas));
    m.crear_regex(p, b)
}

/// `re.exec(texto)` (o `re.test(texto)` si `solo_saber`), con `lastIndex` si es `g` o `y`.
fn ejecutar(m: &mut Motor, re: Valor, texto: &str, solo_saber: bool) -> Result<Valor, Valor> {
    let i = expresion(m, re)?;
    let banderas = m.regexes[i as usize].re.banderas();
    let usa_last = banderas.global || banderas.pegajosa;
    let desde = if usa_last { a_byte(texto, m.last_index(re) as usize) } else { Some(0) };
    let encaje = desde.and_then(|d| m.buscar_regex(i, texto, d));
    let Some(grupos) = encaje else {
        if usa_last {
            m.fijar_last_index(re, 0);
        }
        return Ok(if solo_saber { Valor::FALSO } else { Valor::NULO });
    };
    let (inicio, fin) = grupos[0].unwrap_or((0, 0));
    if usa_last {
        m.fijar_last_index(re, a_utf16(texto, fin));
    }
    if solo_saber {
        return Ok(Valor::VERDADERO);
    }
    resultado_exec(m, texto, &grupos, inicio)
}

/// El arreglo de `exec`: el encaje, sus grupos y las propiedades `index` e `input`.
fn resultado_exec(m: &mut Motor, texto: &str, grupos: &Grupos, inicio: usize) -> Result<Valor, Valor> {
    let valores: Vec<Valor> = grupos
        .iter()
        .map(|g| match g {
            Some((a, b)) => m.mundo.cadena(&texto[*a..*b]),
            None => Valor::INDEFINIDO,
        })
        .collect();
    let a = m.mundo.crear_arreglo(&valores, m.pila(R_ARREGLO));
    let marca = m.mundo.m.marca_raices();
    m.mundo.m.raiz(a);
    let entrada = m.mundo.cadena(texto);
    let (k_index, k_input) = (m.mundo.cadenas.internar("index"), m.mundo.cadenas.internar("input"));
    let indice = Valor::entero(a_utf16(texto, inicio) as i32);
    m.mundo.escribir(m.pila(marca), k_index, indice);
    m.mundo.escribir(m.pila(marca), k_input, entrada);
    let a = m.pila(marca);
    m.mundo.m.soltar_raices(marca);
    Ok(a)
}

/// Todos los encajes (con `g`) o el primero; los vacíos avanzan un carácter para no repetirse.
fn todos(m: &mut Motor, i: u32, texto: &str, global: bool) -> Vec<Grupos> {
    let mut v = Vec::new();
    let mut pos = 0;
    while pos <= texto.len() {
        let Some(g) = m.buscar_regex(i, texto, pos) else { break };
        let (inicio, fin) = g[0].unwrap_or((pos, pos));
        v.push(g);
        if !global {
            break;
        }
        pos = if fin == inicio { if fin >= texto.len() { break } else { avanzar(texto, fin) } } else { fin };
    }
    v
}

/// `texto.replace(patrón, reemplazo)`: el patrón es una expresión regular o un texto (solo la
/// primera aparición); el reemplazo, un texto con `$1`, `$&`... o una función.
fn reemplazar(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let texto = texto_de(m, m.pila(ll.este));
    let (patron, reemplazo) = (arg(m, ll, 0), arg(m, ll, 1));
    let encajes = match m.mundo.regexp(patron) {
        Some(i) => {
            let global = m.regexes[i as usize].re.banderas().global;
            if global {
                m.fijar_last_index(patron, 0);
            }
            todos(m, i, &texto, global)
        }
        None => {
            let buscado = texto_de(m, patron);
            texto
                .find(buscado.as_str())
                .map(|p| alloc::vec![alloc::vec![Some((p, p + buscado.len()))]])
                .unwrap_or_default()
        }
    };
    if encajes.is_empty() {
        return Ok(m.mundo.cadena(&texto));
    }
    let funcion = matches!(m.mundo.clase(reemplazo), Some(Clase::Funcion | Clase::Nativa));
    let plantilla = if funcion { String::new() } else { texto_de(m, reemplazo) };
    let mut salida = String::with_capacity(texto.len());
    let mut ultimo = 0;
    for g in &encajes {
        let (inicio, fin) = g[0].unwrap_or((0, 0));
        salida.push_str(&texto[ultimo..inicio]);
        if funcion {
            let r = llamar_reemplazo(m, arg(m, ll, 1), &texto, g)?;
            salida.push_str(&r);
        } else {
            expandir(&mut salida, &plantilla, &texto, g);
        }
        ultimo = fin;
    }
    salida.push_str(&texto[ultimo..]);
    Ok(m.mundo.cadena(&salida))
}

/// Llama a la función de reemplazo con (encaje, grupos..., índice, texto).
fn llamar_reemplazo(m: &mut Motor, f: Valor, texto: &str, g: &Grupos) -> Result<String, Valor> {
    let mut args: Vec<Valor> = g
        .iter()
        .map(|x| match x {
            Some((a, b)) => m.mundo.cadena(&texto[*a..*b]),
            None => Valor::INDEFINIDO,
        })
        .collect();
    args.push(Valor::entero(a_utf16(texto, g[0].map_or(0, |x| x.0)) as i32));
    args.push(m.mundo.cadena(texto));
    let r = m.llamar_interno(f, Valor::INDEFINIDO, &args)?;
    let r = m.a_primitivo(r)?;
    Ok(op::a_texto(&m.mundo, r))
}

/// Copia `plantilla` a `salida` sustituyendo `$$`, `$&`, `` $` ``, `$'`, `$1`..`$99`.
fn expandir(salida: &mut String, plantilla: &str, texto: &str, g: &Grupos) {
    let (inicio, fin) = g[0].unwrap_or((0, 0));
    let b = plantilla.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let siguiente = b.get(i + 1).copied();
        if b[i] != b'$' || siguiente.is_none() {
            let c = plantilla[i..].chars().next().unwrap_or('$');
            salida.push(c);
            i += c.len_utf8();
            continue;
        }
        i += 2;
        match siguiente.unwrap_or(0) {
            b'$' => salida.push('$'),
            b'&' => salida.push_str(&texto[inicio..fin]),
            b'`' => salida.push_str(&texto[..inicio]),
            b'\'' => salida.push_str(&texto[fin..]),
            d @ b'0'..=b'9' => {
                let uno = usize::from(d - b'0');
                let dos = b.get(i).filter(|x| x.is_ascii_digit()).map(|x| uno * 10 + usize::from(x - b'0'));
                let (n, usados) = match dos {
                    Some(n) if n >= 1 && n < g.len() => (n, 1),
                    _ => (uno, 0),
                };
                if n == 0 || n >= g.len() {
                    salida.push('$');
                    salida.push(char::from(d));
                    continue;
                }
                i += usados;
                if let Some((a, z)) = g[n] {
                    salida.push_str(&texto[a..z]);
                }
            }
            _ => {
                // `$` sin significado: se queda tal cual y se sigue por el carácter de después.
                salida.push('$');
                i -= 1;
            }
        }
    }
}

/// `texto.match(re)`: sin `g`, como `exec`; con `g`, un arreglo con todos los encajes (o `null`).
fn coincidir(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let texto = texto_de(m, m.pila(ll.este));
    let patron = arg(m, ll, 0);
    let i = expresion_o_nueva(m, patron)?;
    let re = match m.mundo.regexp(patron) {
        Some(_) => patron,
        None => {
            let c = &m.regexes[i as usize];
            let (f, b) = (c.fuente.clone(), c.banderas.clone());
            let (f, b) = (m.mundo.cadenas.internar(&f), m.mundo.cadenas.internar(&b));
            m.crear_regex(f, b)?
        }
    };
    if !m.regexes[i as usize].re.banderas().global {
        return ejecutar(m, re, &texto, false);
    }
    m.fijar_last_index(re, 0);
    let encajes = todos(m, i, &texto, true);
    if encajes.is_empty() {
        return Ok(Valor::NULO);
    }
    let valores: Vec<Valor> = encajes
        .iter()
        .map(|g| {
            let (a, b) = g[0].unwrap_or((0, 0));
            m.mundo.cadena(&texto[a..b])
        })
        .collect();
    Ok(m.mundo.crear_arreglo(&valores, m.pila(R_ARREGLO)))
}

/// `texto.split(re, límite)`: los trozos entre encajes, con los grupos de cada encaje en medio.
pub(super) fn dividir(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let texto = texto_de(m, m.pila(ll.este));
    let i = expresion(m, arg(m, ll, 0))?;
    let limite = match arg(m, ll, 1) {
        Valor::INDEFINIDO => u32::MAX,
        v => op::a_numero(&m.mundo, v) as i64 as u32,
    } as usize;
    let mut trozos: Vec<Option<(usize, usize)>> = Vec::new();
    if texto.is_empty() {
        // Un texto vacío da [""] salvo que la expresión encaje en él.
        if m.buscar_regex(i, &texto, 0).is_none() {
            trozos.push(Some((0, 0)));
        }
    } else {
        let (mut p, mut q) = (0, 0);
        while q < texto.len() && trozos.len() < limite {
            let Some(g) = m.buscar_regex(i, &texto, q) else { break };
            let (inicio, fin) = g[0].unwrap_or((q, q));
            if inicio >= texto.len() {
                break;
            }
            // Un encaje vacío justo donde empieza el trozo no corta: se prueba un carácter después.
            if fin == p {
                q = avanzar(&texto, inicio);
                continue;
            }
            trozos.push(Some((p, inicio)));
            trozos.extend(g.iter().skip(1).copied());
            p = fin;
            q = fin;
        }
        trozos.push(Some((p, texto.len())));
    }
    trozos.truncate(limite);
    let valores: Vec<Valor> = trozos
        .iter()
        .map(|t| match t {
            Some((a, b)) => m.mundo.cadena(&texto[*a..*b]),
            None => Valor::INDEFINIDO,
        })
        .collect();
    Ok(m.mundo.crear_arreglo(&valores, m.pila(R_ARREGLO)))
}
