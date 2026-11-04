//! Una pasada de maquetación: estilo (incremental: solo lo que tocó JavaScript), maquetación,
//! escena y GPU. Con `QUIMERA_TIEMPOS`, su desglose por fases; con `QUIMERA_ESTILO_COMPROBAR`, el
//! estilo incremental comparado con el cálculo entero.

use super::{Evento, Navegador, bajo_la_barra, color_gpu};
use crate::imagenes::{self, Imagenes};
use crate::pagina::Pagina;
use quimera_estilo::{CacheHojas, Estilos, Memoria, Opciones};
use quimera_html::{Documento, Toques};
use quimera_maquetacion::{Maquetacion, maquetar_pagina};
use std::time::Instant;
use winit::window::Window;

/// Cuándo empezó cada fase de una pasada.
struct Fases {
    copia: Instant,
    seudo: Instant,
    estilo: Instant,
    maquetar: Instant,
    escena: Instant,
}

impl Fases {
    fn nuevas() -> Fases {
        let t = Instant::now();
        Fases { copia: t, seudo: t, estilo: t, maquetar: t, escena: t }
    }

    /// El desglose (`QUIMERA_TIEMPOS`), con cuántos elementos recalculó el estilo.
    fn imprimir(&self, memoria: &Memoria) {
        if std::env::var_os("QUIMERA_TIEMPOS").is_none() {
            return;
        }
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1e3;
        let (recalculados, con_estilo, entero) = memoria.ultimo;
        eprintln!(
            "maquetar: copiar {:.1} ms, seudoelementos {:.1}, estilo {:.1} ({recalculados} de {con_estilo} elementos{}), maquetación {:.1}, escena {:.1}",
            ms(self.copia, self.seudo),
            ms(self.seudo, self.estilo),
            ms(self.estilo, self.maquetar),
            if entero { ", entero" } else { "" },
            ms(self.maquetar, self.escena),
            ms(self.escena, Instant::now())
        );
    }
}

impl Navegador {
    /// Estilo, maquetación y cuadros de la página para el tamaño actual de la ventana.
    pub fn maquetar(&mut self) {
        let barra_px = self.alto_barra();
        let (Some(a), Some(pagina)) = (&mut self.abierta, &self.pagina) else { return };
        let inicio = Instant::now();
        let mut f = Fases::nuevas();
        let tam = a.ventana.inner_size();
        let escala = a.ventana.scale_factor() as f32;
        let op = Opciones { ancho: tam.width as f32 / escala, alto: (tam.height as f32 - barra_px).max(1.0) / escala };
        // Qué tocó JavaScript desde la última vez (el documento de la página sin scripts no cambia).
        let toques = self.guion.as_ref().map(|g| g.dom.tomar_toques()).unwrap_or_default();
        let mut doc = match (&self.guion, &self.estatico) {
            (Some(g), _) => g.dom.documento().clone(),
            (None, Some(d)) => d.clone(),
            (None, None) => return,
        };
        let externa = |href: &str| pagina.hoja(href);
        let estilos = estilo(&mut doc, &op, &externa, (&self.hojas, &mut self.memoria_estilo), &toques, &mut f);
        let imagenes = &mut self.imagenes;
        let medidor = self.pintor.medidor();
        let m = {
            let _t = crate::trazas::tramo("web/maquetacion", Some("web/estilo"));
            maquetar_pagina(&doc, &estilos, &medidor, (op.ancho, op.alto), &|s| imagenes.tam(s), &self.formulario)
        };
        f.escena = Instant::now();
        imagenes.en_linea(&self.cliente, &pagina.base, &m);
        let mut cuadros = {
            let _t = crate::trazas::tramo("grafico/pintar", Some("web/maquetacion"));
            self.pintor.escena(&m, escala, &|s| imagenes.id(s))
        };
        bajo_la_barra(&mut cuadros, barra_px);
        f.imprimir(&self.memoria_estilo);
        let nuevas = imagenes.nuevas(&doc);
        a.gpu.cargar(&cuadros);
        a.fondo = color_gpu(m.fondo);
        rotular(&a.ventana, &doc, pagina);
        informe(pagina, &doc, cuadros.len(), &m, imagenes, inicio);
        a.maq = Some(m);
        drop(doc);
        if !nuevas.is_empty() {
            let (id, proxy) = (self.id_carga, self.proxy.clone());
            let base = pagina.base.clone();
            imagenes::descargar_todas(self.cliente.clone(), base, nuevas, move |src, imagen| {
                let _ = proxy.send_event(Evento::Imagen { id, src, imagen });
            });
        }
        self.desplazar(0.0);
        self.pintar_barra();
    }
}

/// Los pseudoelementos y el estilo de la pasada (incremental con la memoria del cálculo anterior).
fn estilo(
    doc: &mut Documento,
    op: &Opciones,
    externa: &dyn Fn(&str) -> Option<String>,
    (hojas, memoria): (&CacheHojas, &mut Memoria),
    toques: &Toques,
    f: &mut Fases,
) -> Estilos {
    let _t = crate::trazas::tramo("web/estilo", Some("web/dom"));
    let originales = doc.total();
    f.seudo = Instant::now();
    quimera_estilo::anadir_seudoelementos(doc, op, externa, hojas);
    f.estilo = Instant::now();
    let e = quimera_estilo::calcular_incremental(doc, op, externa, hojas, memoria, originales, toques);
    comprobar_estilo(doc, op, externa, &e);
    f.maquetar = Instant::now();
    e
}

/// El título de la ventana: el de la página y su sitio.
fn rotular(ventana: &Window, doc: &Documento, pagina: &Pagina) {
    let titulo = doc.buscar("title").map(|t| doc.texto(t).trim().to_string()).filter(|t| !t.is_empty());
    let sitio = if pagina.url.anfitrion.is_empty() { String::new() } else { format!(" · {}", pagina.url.anfitrion) };
    ventana.set_title(&format!("{}{sitio} — QUIMERA", titulo.as_deref().unwrap_or("sin título")));
}

/// Una línea por pasada con lo que salió.
fn informe(pagina: &Pagina, doc: &Documento, cuadros: usize, m: &Maquetacion, imagenes: &Imagenes, inicio: Instant) {
    eprintln!(
        "{}: red {:.0} ms · {} nodos, {cuadros} cuadros, {} enlaces, {} imágenes ({} en la GPU), maquetado en {:.1} ms",
        pagina.url,
        pagina.ms_red,
        doc.total(),
        m.enlaces.len(),
        imagenes.total(),
        imagenes.en_gpu(),
        inicio.elapsed().as_secs_f64() * 1e3
    );
}

/// Con `QUIMERA_ESTILO_COMPROBAR=1`, compara el estilo incremental con el cálculo entero y dice
/// cuántos elementos salen distintos (debe ser 0).
fn comprobar_estilo(doc: &Documento, op: &Opciones, externa: &dyn Fn(&str) -> Option<String>, inc: &Estilos) {
    if std::env::var_os("QUIMERA_ESTILO_COMPROBAR").is_none() {
        return;
    }
    let entero = quimera_estilo::calcular_con_cache(doc, op, externa, &CacheHojas::default());
    let distintos =
        (0..doc.total() as u32).map(quimera_html::NodoId).filter(|&id| inc.de(id) != entero.de(id)).collect::<Vec<_>>();
    eprintln!(
        "estilo incremental: {} elementos distintos del cálculo entero {:?}",
        distintos.len(),
        &distintos[..distintos.len().min(5)]
    );
}
