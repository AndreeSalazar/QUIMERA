//! Las reglas: cada función revisa una cosa y añade hallazgos.

use crate::lexico::{self, Clase, Tok};
use crate::proyecto::{Archivo, Crate, Proyecto};
use crate::semaforo::{Hallazgo, regla};
use std::collections::HashMap;

/// Líneas máximas por archivo (rojo).
pub const MAX_LINEAS: usize = 1000;
/// A partir de aquí, amarillo.
pub const AVISO_LINEAS: usize = 700;
/// Líneas máximas por función antes del amarillo.
pub const MAX_FUNCION: usize = 80;
/// Líneas máximas por crate antes del amarillo.
pub const MAX_CRATE: usize = 5000;

fn nuevo(id: &str, c: &Crate, archivo: &str, linea: usize, mensaje: String) -> Hallazgo {
    Hallazgo {
        regla: regla(id),
        crate_: c.nombre.clone(),
        archivo: archivo.to_string(),
        linea,
        mensaje,
        revisado: None,
    }
}

/// Pasa todas las reglas por el proyecto y aplica las revisiones de `guardianes.toml`.
pub fn revisar(p: &Proyecto) -> Vec<Hallazgo> {
    let mut h = Vec::new();
    for (ruta, motivo) in &p.ilegibles {
        h.push(Hallazgo {
            regla: regla("R-META"),
            crate_: String::new(),
            archivo: format!("{ruta}/Cargo.toml"),
            linea: 0,
            mensaje: motivo.clone(),
            revisado: None,
        });
    }
    let niveles: HashMap<&str, i64> =
        p.crates.iter().filter_map(|c| Some((c.nombre.as_str(), c.meta.as_ref()?.nivel))).collect();
    for c in &p.crates {
        jerarquia(c, &niveles, &mut h);
        let mut total = 0;
        let mut con_pruebas = c.archivos.iter().any(|a| a.ruta.contains("/tests/"));
        for a in &c.archivos {
            total += a.texto.lines().count();
            con_pruebas |= archivo(c, a, &mut h);
        }
        if total > MAX_CRATE {
            h.push(nuevo(
                "A-CRATE",
                c,
                "",
                0,
                format!("{total} líneas en total (más de {MAX_CRATE}): pensar en partirlo"),
            ));
        }
        if !con_pruebas {
            h.push(nuevo("A-SINPRUEBAS", c, "", 0, "ni #[test] ni carpeta tests/".into()));
        }
    }
    aplicar_revisiones(p, &mut h);
    h
}

/// Metadata, niveles, dependencias externas y no_std.
fn jerarquia(c: &Crate, niveles: &HashMap<&str, i64>, h: &mut Vec<Hallazgo>) {
    let cargo = format!("{}/Cargo.toml", c.ruta);
    let Some(m) = &c.meta else {
        h.push(nuevo("R-META", c, &cargo, 0, "falta [package.metadata.quimera]".into()));
        return;
    };
    if m.nivel < 0 || m.capa.is_empty() {
        h.push(nuevo("R-META", c, &cargo, 0, "nivel y capa son obligatorios".into()));
    }
    let carpeta = c.ruta.split('/').next().unwrap_or("");
    if carpeta != m.capa {
        h.push(nuevo("R-META", c, &cargo, 0, format!("está en {carpeta}/ pero declara capa \"{}\"", m.capa)));
    }
    for d in &c.dependencias {
        match niveles.get(d.as_str()) {
            Some(&nd) if nd >= m.nivel => {
                h.push(nuevo(
                    "R-NIVEL",
                    c,
                    &cargo,
                    0,
                    format!("nivel {} usa {d} (nivel {nd}): solo se usan niveles menores", m.nivel),
                ));
            }
            Some(_) => {}
            None if !m.externos.contains(d) => {
                h.push(nuevo("R-EXTERNO", c, &cargo, 0, format!("usa {d}, que no está en su lista `externos`")));
            }
            None => {}
        }
    }
    if m.no_std {
        let lib = c.archivos.iter().find(|a| a.ruta == format!("{}/src/lib.rs", c.ruta));
        let dice = lib.is_some_and(|a| a.texto.contains("#![no_std]") || a.texto.contains("no_std)]"));
        if !dice {
            h.push(nuevo(
                "R-NOSTD",
                c,
                &format!("{}/src/lib.rs", c.ruta),
                0,
                "declara no_std = true pero su lib.rs no lo dice".into(),
            ));
        }
    }
}

/// Reglas de un archivo. Devuelve si contiene pruebas.
fn archivo(c: &Crate, a: &Archivo, h: &mut Vec<Hallazgo>) -> bool {
    let lineas: Vec<&str> = a.texto.lines().collect();
    tamano(c, a, lineas.len(), h);
    let (k, comentarios) = lexico::tokens_y_comentarios(&a.texto);
    let prueba =
        if lexico::archivo_de_prueba(&a.ruta, &k) { vec![true; k.len()] } else { lexico::regiones_de_prueba(&k) };
    for i in 0..k.len() {
        if k[i].clase == Clase::Ident {
            palabra(c, a, &k, i, prueba[i], &lineas, h);
        }
    }
    pendientes(c, a, &comentarios, h);
    prueba.iter().any(|p| *p)
}

/// El límite de líneas por archivo.
fn tamano(c: &Crate, a: &Archivo, n: usize, h: &mut Vec<Hallazgo>) {
    if n > MAX_LINEAS {
        let m = format!("{n} líneas (máximo {MAX_LINEAS}): partirlo en módulos");
        h.push(nuevo("R-TAMANO", c, &a.ruta, 0, m));
    } else if n > AVISO_LINEAS {
        let m = format!("{n} líneas (aviso desde {AVISO_LINEAS}, máximo {MAX_LINEAS})");
        h.push(nuevo("A-TAMANO", c, &a.ruta, 0, m));
    }
}

/// Lo que dice un identificador del código: unsafe, unwrap, panic!, allow, fn.
fn palabra(c: &Crate, a: &Archivo, k: &[Tok], i: usize, es_prueba: bool, lineas: &[&str], h: &mut Vec<Hallazgo>) {
    let linea = k[i].linea;
    let (antes, despues) = (lexico::t(k, i.wrapping_sub(1)), lexico::t(k, i + 1));
    let permite_unsafe = c.meta.as_ref().is_some_and(|m| m.permite_unsafe);
    match k[i].t {
        "unsafe" => inseguro(c, a, k, i, lineas, permite_unsafe, h),
        "unwrap" | "expect" if !es_prueba && antes == "." && despues == "(" => {
            h.push(nuevo("A-UNWRAP", c, &a.ruta, linea, format!("{}() fuera de las pruebas", k[i].t)));
        }
        "panic" | "todo" | "unimplemented" | "unreachable" if !es_prueba && despues == "!" => {
            h.push(nuevo("A-PANIC", c, &a.ruta, linea, format!("{}! fuera de las pruebas", k[i].t)));
        }
        "allow" if despues == "(" && antes == "[" => {
            let que: String = k.iter().skip(i + 2).take_while(|x| x.t != ")").map(|x| x.t).collect();
            h.push(nuevo("A-ALLOW", c, &a.ruta, linea, format!("#[allow({que})]")));
        }
        "fn" if !es_prueba => funcion(c, a, k, i, h),
        _ => {}
    }
}

/// Marcas de pendiente en los comentarios (no en las cadenas): TODO, FIXME o XXX seguidos de dos puntos o de un paréntesis.
fn pendientes(c: &Crate, a: &Archivo, comentarios: &[Tok], h: &mut Vec<Hallazgo>) {
    for com in comentarios {
        for (d, l) in com.t.lines().enumerate() {
            if let Some(marca) = ["TODO", "FIXME", "XXX"]
                .into_iter()
                .find(|m| l.contains(&format!("{m}:")) || l.contains(&format!("{m}(")))
            {
                h.push(nuevo("A-PENDIENTE", c, &a.ruta, com.linea + d, format!("{marca}: {}", l.trim())));
            }
        }
    }
}

/// `unsafe`: prohibido salvo permiso; con permiso, los bloques e impl necesitan `// SAFETY:`.
fn inseguro(c: &Crate, a: &Archivo, k: &[Tok], i: usize, lineas: &[&str], permite: bool, h: &mut Vec<Hallazgo>) {
    let linea = k[i].linea;
    let siguiente = lexico::t(k, i + 1);
    if !permite {
        h.push(nuevo(
            "R-UNSAFE",
            c,
            &a.ruta,
            linea,
            format!("unsafe {siguiente}: este crate no tiene permite_unsafe = true"),
        ));
        return;
    }
    if matches!(siguiente, "{" | "impl") {
        let desde = linea.saturating_sub(4);
        let documentado = lineas[desde..linea.min(lineas.len())].iter().any(|l| l.contains("SAFETY:"));
        if !documentado {
            h.push(nuevo(
                "R-SAFETY",
                c,
                &a.ruta,
                linea,
                format!("unsafe {siguiente} sin // SAFETY: en las 3 líneas anteriores"),
            ));
        }
    }
}

/// Funciones de más de [`MAX_FUNCION`] líneas.
fn funcion(c: &Crate, a: &Archivo, k: &[Tok], i: usize, h: &mut Vec<Hallazgo>) {
    let nombre = lexico::t(k, i + 1);
    if nombre.is_empty() {
        return;
    }
    // El cuerpo empieza en el primer `{` fuera de paréntesis; un `;` antes significa que no tiene.
    let mut nivel = 0i32;
    let mut abre = None;
    for j in i + 1..k.len() {
        match lexico::t(k, j) {
            "(" | "[" => nivel += 1,
            ")" | "]" => nivel -= 1,
            ";" if nivel == 0 => break,
            "{" if nivel == 0 => {
                abre = Some(j);
                break;
            }
            _ => {}
        }
    }
    let Some(abre) = abre else { return };
    let Some(fin) = lexico::cierre(k, abre) else {
        return;
    };
    let largo = k[fin].linea - k[i].linea + 1;
    if largo > MAX_FUNCION {
        h.push(nuevo(
            "A-FUNCION",
            c,
            &a.ruta,
            k[i].linea,
            format!("fn {nombre} ocupa {largo} líneas (aviso desde {MAX_FUNCION})"),
        ));
    }
}

/// Marca los amarillos aceptados y denuncia las revisiones que ya no aplican.
fn aplicar_revisiones(p: &Proyecto, h: &mut Vec<Hallazgo>) {
    let mut usadas = vec![false; p.revisiones.len()];
    for x in h.iter_mut() {
        if x.regla.color != crate::semaforo::Color::Amarillo {
            continue;
        }
        if let Some((n, r)) = p
            .revisiones
            .iter()
            .enumerate()
            .find(|(_, r)| r.regla == x.regla.id && (r.archivo == x.archivo || r.archivo == x.crate_))
        {
            usadas[n] = true;
            x.revisado = Some(if r.motivo.is_empty() { "(sin motivo)".into() } else { r.motivo.clone() });
        }
    }
    for (r, usada) in p.revisiones.iter().zip(usadas) {
        if !usada || r.motivo.trim().is_empty() {
            let que = if usada { "no explica el motivo" } else { "no corresponde a ningún amarillo actual: borrarla" };
            h.push(Hallazgo {
                regla: regla("R-REVISION"),
                crate_: String::new(),
                archivo: "guardianes.toml".into(),
                linea: 0,
                mensaje: format!("{} en {}: {que}", r.regla, r.archivo),
                revisado: None,
            });
        }
    }
}
