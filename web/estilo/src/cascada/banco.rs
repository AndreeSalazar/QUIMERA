//! Banco del estilo con una página de verdad volcada por la ventana (`QUIMERA_VOLCAR`): si
//! `QUIMERA_BANCO_ESTILO` apunta a esa carpeta, cronometra por separado recoger las hojas,
//! construir el índice y calcular cada elemento. Sin la variable no hace nada.

extern crate std;

use super::*;
use std::time::Instant;

#[test]
fn banco_con_una_pagina_volcada() {
    let Some(carpeta) = std::env::var_os("QUIMERA_BANCO_ESTILO") else { return };
    let carpeta = std::path::PathBuf::from(carpeta);
    let html = std::fs::read_to_string(carpeta.join("pagina.html")).unwrap_or_default();
    let lista = std::fs::read_to_string(carpeta.join("hojas.txt")).unwrap_or_default();
    let hojas_css: Vec<(String, String)> = lista
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(f, href)| (String::from(href), std::fs::read_to_string(carpeta.join(f)).unwrap_or_default()))
        .collect();
    let doc = quimera_html::analizar(&html);
    let externa = |h: &str| hojas_css.iter().find(|(x, _)| x == h).map(|(_, c)| c.clone());
    let op = Opciones { ancho: 1280.0, alto: 856.0 };
    let cache = CacheHojas::default();
    {
        let hs = hojas(&doc, &externa, &cache);
        let ind = Indice::nuevo(&hs, &op);
        let mut clases: std::vec::Vec<(&str, usize)> = ind.por_clase.iter().map(|(k, v)| (*k, v.len())).collect();
        clases.sort_by_key(|x| core::cmp::Reverse(x.1));
        std::eprintln!("resto {} · clases más grandes {:?}", ind.resto.len(), &clases[..clases.len().min(8)]);
        // Candidatas medias por elemento.
        let (mut total, mut casan, mut n) = (0usize, 0usize, 0usize);
        for id in doc.descendientes(Documento::RAIZ) {
            let Some(el) = doc.elemento(id) else { continue };
            let mut c = ind.resto.len();
            if let Some(i) = el.atributo("id") {
                c += ind.por_id.get(i).map_or(0, |v| v.len());
            }
            if let Some(cl) = el.atributo("class") {
                for k in cl.split_ascii_whitespace() {
                    c += ind.por_clase.get(k).map_or(0, |v| v.len());
                }
            }
            c += ind.por_etiqueta.get(el.nombre.as_str()).map_or(0, |v| v.len());
            total += c;
            casan += ind.casan(Nodo { doc: &doc, id }).len();
            n += 1;
        }
        std::eprintln!("{n} elementos: {} candidatas de media, {} casan de media", total / n.max(1), casan / n.max(1));
        let t = Instant::now();
        let mut k = 0usize;
        for id in doc.descendientes(Documento::RAIZ) {
            if doc.elemento(id).is_some() {
                k += ind.casan(Nodo { doc: &doc, id }).len();
            }
        }
        std::eprintln!("casan de todos los elementos ({k} entradas): {:?}", t.elapsed());
    }
    for vuelta in 0..3 {
        let t = Instant::now();
        let hs = hojas(&doc, &externa, &cache);
        let t_hojas = t.elapsed();
        let t = Instant::now();
        let indice = Indice::nuevo(&hs, &op);
        let t_indice = t.elapsed();
        let t = Instant::now();
        let estilos = calcular(&doc, &op, &externa, &cache);
        let t_total = t.elapsed();
        let n = estilos.estilos.iter().flatten().count();
        std::eprintln!(
            "vuelta {vuelta}: {} nodos, {n} con estilo, {} hojas, {} reglas: hojas {:?}, índice {:?}, calcular entero {:?}",
            doc.total(),
            hs.len(),
            indice.entradas.len(),
            t_hojas,
            t_indice,
            t_total
        );
    }
}
