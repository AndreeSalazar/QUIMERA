//! quimera-html contra html5ever 0.39 (el analizador de HTML de Servo) en páginas reales.
//!
//! Lee páginas HTML de una carpeta (por defecto, la documentación de Rust que instala rustup:
//! miles de páginas reales), las analiza con los dos y compara los árboles volcados en el mismo
//! formato. Dice cuántas dan el mismo árbol, muestra las primeras diferencias y mide la velocidad.
//!
//! Uso: `cargo run --release -p quimera-banco-web [carpeta] [máximo de páginas]`
//! Una sola página: `cargo run --release -p quimera-banco-web pagina.html` muestra los dos
//! árboles alrededor de la primera diferencia.

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Todas las páginas `.html` bajo `dir`, ordenadas.
fn paginas(dir: &Path, v: &mut Vec<PathBuf>) {
    archivos(dir, "html", v);
}

/// Todos los archivos con esta extensión bajo `dir`, ordenados.
fn archivos(dir: &Path, extension: &str, v: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(dir) else { return };
    let mut entradas: Vec<PathBuf> = entradas.flatten().map(|e| e.path()).collect();
    entradas.sort();
    for p in entradas {
        if p.is_dir() {
            archivos(&p, extension, v);
        } else if p.extension().is_some_and(|x| x == extension) {
            v.push(p);
        }
    }
}

/// La documentación de Rust de la cadena de herramientas estable, si está instalada.
fn carpeta_por_defecto() -> PathBuf {
    let casa = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    let toolchains = Path::new(&casa).join(".rustup").join("toolchains");
    let mut candidatas: Vec<PathBuf> = std::fs::read_dir(&toolchains)
        .map(|d| d.flatten().map(|e| e.path().join("share").join("doc").join("rust").join("html")).collect())
        .unwrap_or_default();
    candidatas.retain(|p| p.is_dir());
    candidatas.sort();
    candidatas.into_iter().find(|p| p.to_string_lossy().contains("stable")).unwrap_or_else(|| PathBuf::from("."))
}

/// El árbol de html5ever en el formato de `quimera_html::Documento::volcar`.
fn volcar_html5ever(dom: &RcDom) -> String {
    fn nodo(h: &Handle, nivel: usize, s: &mut String) {
        for hijo in h.children.borrow().iter() {
            let linea = match &hijo.data {
                NodeData::Doctype { name, .. } => Some(format!("<!DOCTYPE {name}>")),
                NodeData::Element { name, attrs, .. } => {
                    let mut l = format!("<{}", name.local);
                    for a in attrs.borrow().iter() {
                        // El nombre completo (`xmlns:xlink`), que es el que ve `getAttribute`.
                        match &a.name.prefix {
                            Some(p) if !p.is_empty() => l.push_str(&format!(" {p}:{}=\"{}\"", a.name.local, a.value)),
                            _ => l.push_str(&format!(" {}=\"{}\"", a.name.local, a.value)),
                        }
                    }
                    l.push('>');
                    Some(l)
                }
                NodeData::Text { contents } => Some(format!("\"{}\"", contents.borrow())),
                NodeData::Comment { contents } => Some(format!("<!-- {contents} -->")),
                _ => None,
            };
            if let Some(l) = linea {
                s.push_str(&"  ".repeat(nivel));
                s.push_str(&l);
                s.push('\n');
            }
            nodo(hijo, nivel + 1, s);
            // El contenido de un `<template>` vive aparte (un fragmento, como en el DOM);
            // quimera-html lo deja como hijos: para comparar se vuelca como hijos también.
            if let NodeData::Element { template_contents, .. } = &hijo.data
                && let Some(fragmento) = template_contents.borrow().as_ref()
            {
                nodo(fragmento, nivel + 1, s);
            }
        }
    }
    let mut s = String::new();
    nodo(&dom.document, 0, &mut s);
    s
}

fn html5ever(texto: &str) -> RcDom {
    html5ever::parse_document(RcDom::default(), Default::default()).one(texto)
}

/// La primera línea en que difieren dos volcados, con su número.
fn primera_diferencia(a: &str, b: &str) -> Option<(usize, String, String)> {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let i = (0..la.len().max(lb.len())).find(|&i| la.get(i) != lb.get(i))?;
    let corta = |s: Option<&&str>| s.map_or(String::from("(nada)"), |x| x.chars().take(140).collect());
    Some((i + 1, corta(la.get(i)), corta(lb.get(i))))
}

/// Los dos árboles de una página, alrededor de su primera diferencia.
fn mostrar_diferencia(ruta: &Path) {
    let texto = std::fs::read_to_string(ruta).expect("la página se lee");
    let (q, h) = (quimera_html::analizar(&texto).volcar(), volcar_html5ever(&html5ever(&texto)));
    let Some((linea, _, _)) = primera_diferencia(&q, &h) else {
        println!("mismo árbol");
        return;
    };
    for (nombre, volcado) in [("QUIMERA", &q), ("html5ever", &h)] {
        println!("--- {nombre}");
        for (i, l) in volcado.lines().enumerate().skip(linea.saturating_sub(8)).take(14) {
            println!(
                "{}{:>5} {}",
                if i + 1 == linea { ">" } else { " " },
                i + 1,
                l.chars().take(120).collect::<String>()
            );
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().filter(|a| !a.is_empty()).map_or_else(carpeta_por_defecto, PathBuf::from);
    if dir.is_file() {
        return mostrar_diferencia(&dir);
    }
    let maximo: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(400);
    let mut todas = Vec::new();
    paginas(&dir, &mut todas);
    // Una muestra repartida por toda la carpeta (libro, referencia, std...).
    let paso = (todas.len() / maximo.max(1)).max(1);
    let muestra: Vec<(PathBuf, String)> = todas
        .iter()
        .step_by(paso)
        .take(maximo)
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();
    let bytes: usize = muestra.iter().map(|(_, t)| t.len()).sum();
    println!("{} páginas de {} ({:.1} MB) en {}\n", muestra.len(), todas.len(), bytes as f64 / 1e6, dir.display());

    let (mut iguales, mut diferencias) = (0, Vec::new());
    let (mut t_quimera, mut t_html5ever) = (Duration::ZERO, Duration::ZERO);
    for (ruta, texto) in &muestra {
        let t = Instant::now();
        let doc = quimera_html::analizar(texto);
        t_quimera += t.elapsed();
        let t = Instant::now();
        let dom = html5ever(texto);
        t_html5ever += t.elapsed();
        match primera_diferencia(&doc.volcar(), &volcar_html5ever(&dom)) {
            None => iguales += 1,
            Some(d) => diferencias.push((ruta.strip_prefix(&dir).unwrap_or(ruta).display().to_string(), d)),
        }
    }
    println!("Mismo árbol que html5ever: {iguales} de {} páginas", muestra.len());
    for (ruta, (linea, q, h)) in diferencias.iter().take(8) {
        println!("\n{ruta}, línea {linea}:\n  QUIMERA:   {q}\n  html5ever: {h}");
    }
    let mbs = |t: Duration| bytes as f64 / 1e6 / t.as_secs_f64();
    println!("\nVelocidad (texto → árbol):");
    println!("  QUIMERA    {:>8.1} ms  {:>6.1} MB/s", t_quimera.as_secs_f64() * 1e3, mbs(t_quimera));
    println!("  html5ever  {:>8.1} ms  {:>6.1} MB/s", t_html5ever.as_secs_f64() * 1e3, mbs(t_html5ever));
    println!("  QUIMERA / html5ever: {:.2}x", t_html5ever.as_secs_f64() / t_quimera.as_secs_f64());
    hojas_de_estilo(&dir);
}

/// Las hojas de estilo de la carpeta: que se analizan enteras, cuánto tienen y a qué velocidad.
fn hojas_de_estilo(dir: &Path) {
    let mut rutas = Vec::new();
    archivos(dir, "css", &mut rutas);
    let mut vistos = std::collections::HashSet::new();
    let hojas: Vec<String> =
        rutas.iter().filter_map(|p| std::fs::read_to_string(p).ok()).filter(|t| vistos.insert(t.clone())).collect();
    let bytes: usize = hojas.iter().map(String::len).sum();
    let t = Instant::now();
    let analizadas: Vec<quimera_css::Hoja> = hojas.iter().map(|h| quimera_css::analizar(h)).collect();
    let tiempo = t.elapsed();
    let reglas: usize =
        analizadas.iter().map(|h| h.reglas.len() + h.medios.iter().map(|m| m.reglas.len()).sum::<usize>()).sum();
    let declaraciones: usize = analizadas
        .iter()
        .flat_map(|h| h.reglas.iter().chain(h.medios.iter().flat_map(|m| m.reglas.iter())))
        .map(|r| r.declaraciones.len())
        .sum();
    println!(
        "
CSS: {} hojas distintas ({:.0} KB): {reglas} reglas, {declaraciones} declaraciones, {:.2} ms ({:.1} MB/s)",
        hojas.len(),
        bytes as f64 / 1e3,
        tiempo.as_secs_f64() * 1e3,
        bytes as f64 / 1e6 / tiempo.as_secs_f64()
    );
}

#[cfg(test)]
mod pruebas {
    /// Trozos difíciles: el mismo árbol que html5ever (lo que aún falta, como el algoritmo
    /// de adopción, no está aquí; ver docs/HTML-CSS.md).
    #[test]
    fn mismo_arbol_que_html5ever_en_casos_dificiles() {
        let casos = [
            "Hola",
            "<!DOCTYPE html><html lang=es><head><title>T &amp; U</title>
<meta charset=utf-8>
</head><body class=x>",
            "<p>uno<p>dos<div>tres</div><ul><li>a<li>b</ul><h1>t<h2>u</h2>",
            "<table><tr><td>1<td>2<tr><th>3</table>",
            "<div>a<br>b<img src=x>c</span></div><!--c--><script>if(a<b)</script><pre>
x</pre></p>",
            "<dl><dt>a<dd>b<dt>c</dl><select><option>1<option>2</select>",
            "<svg viewBox='0 0 1 1'><path d='M0'/><lineargradient/><foreignObject><p>x</p></foreignObject></svg><p>fin",
            "<math><mi>x</mi></math><noscript><b>sin js</b></noscript>",
            "a &lt; b &copy; &#150; &ampx &#x1F600; <a href='?a=1&copy=2'>e</a>",
            "</p><body id=b><p>x</body></html>tras",
        ];
        for caso in casos {
            let q = quimera_html::analizar(caso).volcar();
            let h = super::volcar_html5ever(&super::html5ever(caso));
            assert_eq!(q, h, "{caso:?}");
        }
    }
}
