//! El navegador de una ventana: la página, la barra, el historial y lo que hace cada acción
//! (navegar, pulsar un enlace, escribir una dirección, desplazar).
//!
//! Las páginas se cargan en otro hilo: la ventana sigue respondiendo mientras llegan, y cuando
//! llega una se avisa al bucle de eventos (`Llegada`). Si mientras tanto se pidió otra, la vieja
//! se descarta.

use crate::barra::{self, Barra, Vista, Zona};
use crate::formularios;
use crate::guiones::Guion;
use crate::historial::Historial;
use crate::imagenes::{self, Imagenes};
use crate::pagina::{Pagina, direccion_escrita};
use crate::tipografias;
use quimera_estilo::Opciones;
use quimera_gpu::Gpu;
use quimera_imagen::Imagen;
use quimera_maquetacion::{Maquetacion, maquetar_pagina};
use quimera_pintar::{Pintor, color_gpu};
use quimera_red::{Cliente, Url};
use std::sync::Arc;
use std::time::Instant;
use winit::event::KeyEvent;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window};

/// Lo que llega de los hilos de red al bucle de eventos.
pub enum Evento {
    /// Una página que terminó de cargar.
    Pagina { id: u64, pagina: Box<Pagina> },
    /// Una imagen de la página `id` (descargada y decodificada, o por qué no).
    Imagen { id: u64, src: String, imagen: Result<Imagen, String> },
    /// Una fuente (`@font-face`) de la página `id`, con sus bytes o por qué no llegó.
    Fuente { id: u64, pedida: tipografias::Pedida, bytes: Result<Vec<u8>, String> },
}

/// Lo que existe mientras la ventana está abierta.
pub struct Abierta {
    pub ventana: Arc<Window>,
    pub gpu: Gpu,
    /// La última maquetación (para saber dónde están los enlaces y las anclas).
    maq: Option<Maquetacion>,
    /// Desplazamiento en píxeles de pantalla.
    desp: f32,
    fondo: [f32; 4],
}

pub struct Navegador {
    cliente: Arc<Cliente>,
    proxy: EventLoopProxy<Evento>,
    /// Las imágenes de la página actual.
    imagenes: Imagenes,
    /// Llegaron imágenes desde la última maquetación: se maqueta otra vez antes de dibujar.
    imagenes_nuevas: bool,
    /// La página que se ve o se está pidiendo.
    pub url: Url,
    pagina: Option<Pagina>,
    historial: Historial,
    barra: Barra,
    pintor: Pintor,
    pub abierta: Option<Abierta>,
    /// Número de la última carga pedida: las que lleguen con otro número se descartan.
    id_carga: u64,
    cargando: bool,
    /// La línea de estado (enlace bajo el ratón, «Cargando...»).
    estado: Option<String>,
    raton: (f32, f32),
    pub teclas: ModifiersState,
    /// Lo escrito y marcado en los formularios de la página.
    formulario: formularios::Estado,
    /// El JavaScript de la página: su DOM (el árbol que se maqueta) y sus temporizadores.
    pub guion: Option<Guion>,
}

impl Navegador {
    pub fn nuevo(url: Url, pintor: Pintor, proxy: EventLoopProxy<Evento>) -> Navegador {
        let cliente = Arc::new(Cliente::nuevo());
        crate::perfil::cargar_galletas(&cliente);
        Navegador {
            cliente,
            proxy,
            imagenes: Imagenes::default(),
            imagenes_nuevas: false,
            url,
            pagina: None,
            historial: Historial::default(),
            barra: Barra::default(),
            pintor,
            abierta: None,
            id_carga: 0,
            cargando: false,
            estado: None,
            raton: (0.0, 0.0),
            teclas: ModifiersState::empty(),
            formulario: formularios::Estado::default(),
            guion: None,
        }
    }

    /// La ventana está lista: se pone la GPU y se pide la primera página.
    pub fn abrir(&mut self, ventana: Arc<Window>, gpu: Gpu) {
        self.abierta = Some(Abierta { ventana, gpu, maq: None, desp: 0.0, fondo: [1.0; 4] });
        self.navegar(self.url.clone(), true);
    }

    fn escala(&self) -> f32 {
        self.abierta.as_ref().map_or(1.0, |a| a.ventana.scale_factor() as f32)
    }

    fn alto_barra(&self) -> f32 {
        barra::ALTO * self.escala()
    }

    /// Pide `url` en otro hilo. `nueva`: entra en el historial (no al ir atrás/adelante ni al recargar).
    pub fn navegar(&mut self, url: Url, nueva: bool) {
        self.navegar_con(url, nueva, None);
    }

    /// Como [`Navegador::navegar`], con el cuerpo de un formulario enviado por POST.
    fn navegar_con(&mut self, url: Url, nueva: bool, formulario: Option<Vec<u8>>) {
        if nueva {
            self.historial.ir(url.clone());
        }
        self.url = url.clone();
        self.id_carga += 1;
        self.cargando = true;
        self.estado = Some(format!("Cargando {url}…"));
        self.barra.editando = false;
        let (id, cliente, proxy) = (self.id_carga, self.cliente.clone(), self.proxy.clone());
        std::thread::spawn(move || {
            let pagina = Pagina::cargar(&cliente, &url, formulario.as_deref());
            crate::perfil::guardar_galletas(&cliente);
            let _ = proxy.send_event(Evento::Pagina { id, pagina: Box::new(pagina) });
        });
        self.pintar_barra();
    }

    /// Llegó algo de la red.
    pub fn evento(&mut self, e: Evento) {
        match e {
            Evento::Pagina { id, pagina } if id == self.id_carga => self.llegada(*pagina),
            Evento::Imagen { id, src, imagen } if id == self.id_carga => match imagen {
                Ok(i) => {
                    self.imagenes.llegada(src, i);
                    self.imagenes_nuevas = true;
                    if let Some(a) = &self.abierta {
                        a.ventana.request_redraw();
                    }
                }
                Err(e) => eprintln!("imagen {src}: {e}"),
            },
            Evento::Fuente { id, pedida, bytes } if id == self.id_carga => {
                let w = &pedida.web;
                match bytes.map(|b| self.pintor.anadir_fuente(&w.familia, w.negrita, w.cursiva, b)) {
                    Ok(true) => {
                        self.imagenes_nuevas = true; // se vuelve a maquetar con ella
                        if let Some(a) = &self.abierta {
                            a.ventana.request_redraw();
                        }
                    }
                    Ok(false) => eprintln!("fuente {}: formato que QUIMERA no lee", pedida.url),
                    Err(e) => eprintln!("fuente {}: {e}", pedida.url),
                }
            }
            _ => {} // de una carga anterior
        }
    }

    /// Llegó la página pedida: se muestra (y se va a su `#ancla`), y se piden sus imágenes.
    fn llegada(&mut self, pagina: Pagina) {
        self.imagenes.vaciar();
        self.imagenes_nuevas = false;
        self.formulario = formularios::Estado::default();
        if let Some(a) = &mut self.abierta {
            a.gpu.olvidar_imagenes();
        }
        let (id, proxy) = (self.id_carga, self.proxy.clone());
        let fuentes = imagenes::fuentes(&pagina.documento());
        let proxy_imagenes = proxy.clone();
        imagenes::descargar_todas(self.cliente.clone(), pagina.base.clone(), fuentes, move |src, imagen| {
            let _ = proxy_imagenes.send_event(Evento::Imagen { id, src, imagen });
        });
        self.pintor.olvidar_fuentes();
        tipografias::descargar_todas(self.cliente.clone(), tipografias::pedidas(&pagina), move |pedida, bytes| {
            let _ = proxy.send_event(Evento::Fuente { id, pedida, bytes });
        });
        let fragmento = self.url.fragmento.clone();
        let mut final_ = pagina.url.clone();
        final_.fragmento = fragmento.clone();
        self.historial.sustituir(final_.clone());
        self.url = final_;
        self.guion = Some(Guion::preparar(&pagina));
        self.pagina = Some(pagina);
        self.cargando = false;
        self.estado = None;
        if let Some(a) = &mut self.abierta {
            a.desp = 0.0;
        }
        self.maquetar();
        if let Some(f) = fragmento {
            self.ir_a_ancla(&f);
        }
        self.pintar_barra();
    }

    /// Estilo, maquetación y cuadros de la página para el tamaño actual de la ventana.
    pub fn maquetar(&mut self) {
        let barra_px = self.alto_barra();
        let (Some(a), Some(pagina)) = (&mut self.abierta, &self.pagina) else { return };
        let inicio = Instant::now();
        let tam = a.ventana.inner_size();
        let escala = a.ventana.scale_factor() as f32;
        let (ancho, alto) = (tam.width as f32 / escala, (tam.height as f32 - barra_px).max(1.0) / escala);
        let Some(guion) = &self.guion else { return };
        let doc = guion.dom.documento();
        let externa = |href: &str| pagina.hoja(href);
        let estilos = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho, alto }, &externa);
        let imagenes = &mut self.imagenes;
        let medidor = self.pintor.medidor();
        let m = maquetar_pagina(&doc, &estilos, &medidor, (ancho, alto), &|s| imagenes.tam(s), &self.formulario);
        let cuadros = self.pintor.escena(&m, escala, &|s| imagenes.id(s));
        for (id, w, h, rgba) in imagenes.por_subir() {
            a.gpu.subir_imagen(id, w, h, &rgba);
        }
        a.gpu.cargar(&cuadros);
        a.fondo = color_gpu(m.fondo);
        let titulo = doc.buscar("title").map(|t| doc.texto(t).trim().to_string()).filter(|t| !t.is_empty());
        let sitio =
            if pagina.url.anfitrion.is_empty() { String::new() } else { format!(" · {}", pagina.url.anfitrion) };
        a.ventana.set_title(&format!("{}{sitio} — QUIMERA", titulo.as_deref().unwrap_or("sin título")));
        eprintln!(
            "{}: red {:.0} ms · {} nodos, {} cuadros, {} enlaces, {} imágenes, maquetado en {:.1} ms",
            pagina.url,
            pagina.ms_red,
            doc.total(),
            cuadros.len(),
            m.enlaces.len(),
            imagenes.total(),
            inicio.elapsed().as_secs_f64() * 1e3
        );
        a.maq = Some(m);
        drop(doc);
        self.desplazar(0.0);
        self.pintar_barra();
    }

    /// Vuelve a dibujar la barra y la línea de estado (capa fija) y pide un cuadro nuevo.
    pub fn pintar_barra(&mut self) {
        let Some(a) = &mut self.abierta else { return };
        let tam = a.ventana.inner_size();
        let url = if self.barra.editando { String::new() } else { self.url.to_string() };
        let vista = Vista {
            ancho: tam.width as f32,
            alto: tam.height as f32,
            escala: a.ventana.scale_factor() as f32,
            url: &url,
            atras: self.historial.puede_atras(),
            adelante: self.historial.puede_adelante(),
            estado: self.estado.as_deref(),
        };
        let cuadros = self.barra.cuadros(&mut self.pintor, &vista);
        a.gpu.cargar_fijos(&cuadros);
        if let Some((pixeles, desde, hasta)) = self.pintor.atlas_cambiado() {
            a.gpu.subir_atlas(pixeles, desde, hasta);
        }
        a.ventana.request_redraw();
    }

    /// Dibuja (la página va debajo de la barra).
    pub fn dibujar(&mut self) {
        if std::mem::take(&mut self.imagenes_nuevas) {
            // Todas las que llegaron desde el último cuadro, en una sola maquetación.
            self.maquetar();
        }
        let barra_px = self.alto_barra();
        if let Some(a) = &mut self.abierta
            && let Err(e) = a.gpu.dibujar(a.fondo, a.desp - barra_px)
        {
            eprintln!("no se pudo dibujar: {e}");
        }
    }

    /// Desplaza la página `cuanto` píxeles de pantalla (sin pasarse de sus bordes).
    pub fn desplazar(&mut self, cuanto: f32) {
        let barra_px = self.alto_barra();
        let Some(a) = &mut self.abierta else { return };
        let alto_doc = a.maq.as_ref().map_or(0.0, |m| m.alto) * a.ventana.scale_factor() as f32;
        let visible = a.ventana.inner_size().height as f32 - barra_px;
        a.desp = (a.desp + cuanto).clamp(0.0, (alto_doc - visible).max(0.0));
        a.ventana.request_redraw();
    }

    fn ir_a_ancla(&mut self, id: &str) {
        let Some(a) = &self.abierta else { return };
        let Some(y) = a.maq.as_ref().and_then(|m| m.ancla(id)) else { return };
        let objetivo = y * a.ventana.scale_factor() as f32;
        let actual = a.desp;
        self.desplazar(objetivo - actual);
    }

    /// El `href` resuelto contra la página (lo que se muestra y a donde se va).
    fn resolver(&self, href: &str) -> Option<Url> {
        self.pagina.as_ref()?.base.unir(href).ok()
    }

    /// El ratón se movió a (x, y) en píxeles de pantalla.
    pub fn raton(&mut self, x: f32, y: f32) {
        self.raton = (x, y);
        let (e, barra_px) = (self.escala(), self.alto_barra());
        let ancho_css = self.abierta.as_ref().map_or(0.0, |a| a.ventana.inner_size().width as f32 / e);
        let (cursor, estado) = match Barra::zona(x / e, y / e, ancho_css) {
            Some(Zona::Campo) => (CursorIcon::Text, None),
            Some(_) => (CursorIcon::Pointer, None),
            None => {
                let desp = self.abierta.as_ref().map_or(0.0, |a| a.desp);
                let href = self
                    .abierta
                    .as_ref()
                    .and_then(|a| a.maq.as_ref())
                    .and_then(|m| m.enlace_en(x / e, (y - barra_px + desp) / e));
                match href.map(String::from) {
                    Some(h) => (CursorIcon::Pointer, Some(self.resolver(&h).map_or(h, |u| u.to_string()))),
                    None => (CursorIcon::Default, None),
                }
            }
        };
        if let Some(a) = &self.abierta {
            a.ventana.set_cursor(cursor);
        }
        let estado = if self.cargando { self.estado.clone() } else { estado };
        if estado != self.estado {
            self.estado = estado;
            self.pintar_barra();
        }
    }

    /// Clic (botón izquierdo soltado) donde está el ratón.
    pub fn clic(&mut self) {
        let (x, y) = self.raton;
        let (e, barra_px) = (self.escala(), self.alto_barra());
        let ancho_css = self.abierta.as_ref().map_or(0.0, |a| a.ventana.inner_size().width as f32 / e);
        let zona = Barra::zona(x / e, y / e, ancho_css);
        if zona != Some(Zona::Campo) && self.barra.editando {
            self.barra.editando = false;
            self.pintar_barra();
        }
        match zona {
            Some(Zona::Atras) => self.atras(),
            Some(Zona::Adelante) => self.adelante(),
            Some(Zona::Recargar) => self.navegar(self.url.clone(), false),
            Some(Zona::Campo) => {
                let url = self.url.to_string();
                self.barra.enfocar(&url);
                self.pintar_barra();
            }
            None => {
                let desp = self.abierta.as_ref().map_or(0.0, |a| a.desp);
                let (px, py) = (x / e, (y - barra_px + desp) / e);
                let maq = self.abierta.as_ref().and_then(|a| a.maq.as_ref());
                let control = maq.and_then(|m| m.control_en(px, py));
                let enlace = maq.and_then(|m| m.trozo_de_enlace_en(px, py)).map(|e| (e.href.clone(), e.nodo));
                if self.evento_clic(control.or(enlace.as_ref().map(|e| e.1))) {
                    return; // un script llamó a preventDefault()
                }
                let href = enlace.map(|e| e.0);
                match (control, href) {
                    (Some(c), _) => self.pulsar_control(c),
                    (None, Some(h)) => {
                        self.soltar_foco();
                        self.seguir(&h);
                    }
                    (None, None) => self.soltar_foco(),
                }
            }
        }
    }

    /// El árbol actual de la página (con lo que hayan cambiado sus scripts), copiado.
    fn documento(&self) -> Option<quimera_html::Documento> {
        Some(self.guion.as_ref()?.dom.documento().clone())
    }

    /// Lanza `click` en el nodo pulsado (o en `<body>` si no es un enlace ni un control) y vuelve
    /// a maquetar si el script cambió la página. Devuelve si llamó a `preventDefault()`.
    fn evento_clic(&mut self, nodo: Option<quimera_html::NodoId>) -> bool {
        let Some(g) = &mut self.guion else { return false };
        let nodo = nodo.or_else(|| g.dom.documento().buscar("body"));
        let Some(nodo) = nodo else { return false };
        let prevenido = g.dom.clic(nodo);
        g.informar();
        self.tras_javascript();
        prevenido
    }

    /// Si JavaScript cambió el árbol, se vuelve a maquetar.
    pub fn tras_javascript(&mut self) {
        if self.guion.as_ref().is_some_and(|g| g.dom.tomar_cambios()) {
            self.maquetar();
        }
    }

    /// Ejecuta los temporizadores de la página que ya tocan; devuelve cuándo toca el siguiente.
    pub fn latido(&mut self) -> Option<Instant> {
        let proximo = self.guion.as_mut()?.latido();
        self.tras_javascript();
        proximo
    }

    /// Se pulsó un control de formulario: enfocar, marcar, elegir o enviar.
    fn pulsar_control(&mut self, control: quimera_html::NodoId) {
        let Some(doc) = self.documento() else { return };
        match self.formulario.pulsar(&doc, control) {
            Some(boton) => self.enviar(&doc, boton),
            None => self.maquetar(),
        }
    }

    /// Quita el foco del campo que lo tenga.
    fn soltar_foco(&mut self) {
        if self.formulario.foco.take().is_some() {
            self.maquetar();
        }
    }

    /// Envía el formulario de `control` (un botón, o el campo donde se dio Enter).
    fn enviar(&mut self, doc: &quimera_html::Documento, control: quimera_html::NodoId) {
        let Some(base) = self.pagina.as_ref().map(|p| p.base.clone()) else { return };
        match self.formulario.envio(doc, control, &base) {
            Some((url, cuerpo)) => self.navegar_con(url, true, cuerpo),
            None => {
                self.estado = Some(String::from("Ese control no está dentro de un formulario"));
                self.pintar_barra();
            }
        }
    }

    /// Una tecla con un campo de la página enfocado.
    fn tecla_en_campo(&mut self, evento: &KeyEvent) {
        let Some(doc) = self.documento() else { return };
        match &evento.logical_key {
            Key::Named(NamedKey::Enter) => {
                let foco = self.formulario.foco.take();
                if let Some(c) = foco {
                    return self.enviar(&doc, c);
                }
            }
            Key::Named(NamedKey::Escape | NamedKey::Tab) => self.formulario.foco = None,
            Key::Named(NamedKey::Backspace) => self.formulario.borrar(&doc),
            _ => match evento.text.as_deref().filter(|_| !self.teclas.control_key() && !self.teclas.alt_key()) {
                Some(t) if !t.chars().any(char::is_control) => self.formulario.escribir(&doc, t),
                _ => return,
            },
        }
        self.maquetar();
    }

    /// Seguir un enlace: otra página, o un `#ancla` de esta (sin volver a cargar).
    fn seguir(&mut self, href: &str) {
        let Some(destino) = self.resolver(href) else { return };
        if matches!(destino.esquema.as_str(), "javascript" | "mailto" | "tel") {
            self.estado = Some(format!("QUIMERA todavía no abre enlaces {}:", destino.esquema));
            self.pintar_barra();
            return;
        }
        let sin_fragmento = |u: &Url| Url { fragmento: None, ..u.clone() };
        let misma = self.pagina.as_ref().is_some_and(|p| sin_fragmento(&p.url) == sin_fragmento(&destino));
        match (&destino.fragmento, misma) {
            (Some(f), true) => {
                let f = f.clone();
                self.historial.ir(destino.clone());
                self.url = destino;
                self.ir_a_ancla(&f);
                self.pintar_barra();
            }
            _ => self.navegar(destino, true),
        }
    }

    pub fn atras(&mut self) {
        if let Some(u) = self.historial.atras() {
            self.navegar(u, false);
        }
    }

    pub fn adelante(&mut self) {
        if let Some(u) = self.historial.adelante() {
            self.navegar(u, false);
        }
    }

    /// Una tecla pulsada.
    pub fn tecla(&mut self, evento: &KeyEvent) {
        if self.barra.editando {
            return self.tecla_en_barra(evento);
        }
        if self.formulario.foco.is_some() {
            return self.tecla_en_campo(evento);
        }
        let (ctrl, alt) = (self.teclas.control_key(), self.teclas.alt_key());
        let alto = self.abierta.as_ref().map_or(600.0, |a| a.ventana.inner_size().height as f32) - self.alto_barra();
        let linea = 40.0 * self.escala();
        match &evento.logical_key {
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("l") => self.enfocar_barra(),
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("r") => self.navegar(self.url.clone(), false),
            Key::Named(NamedKey::F6) => self.enfocar_barra(),
            Key::Named(NamedKey::F5) => self.navegar(self.url.clone(), false),
            Key::Named(NamedKey::ArrowLeft) if alt => self.atras(),
            Key::Named(NamedKey::ArrowRight) if alt => self.adelante(),
            Key::Named(NamedKey::BrowserBack) => self.atras(),
            Key::Named(NamedKey::BrowserForward) => self.adelante(),
            Key::Named(NamedKey::ArrowDown) => self.desplazar(linea),
            Key::Named(NamedKey::ArrowUp) => self.desplazar(-linea),
            Key::Named(NamedKey::PageDown | NamedKey::Space) => self.desplazar(alto * 0.9),
            Key::Named(NamedKey::PageUp) => self.desplazar(-alto * 0.9),
            Key::Named(NamedKey::Home) => self.desplazar(f32::MIN),
            Key::Named(NamedKey::End) => self.desplazar(f32::MAX),
            _ => {}
        }
    }

    fn enfocar_barra(&mut self) {
        let url = self.url.to_string();
        self.barra.enfocar(&url);
        self.pintar_barra();
    }

    fn tecla_en_barra(&mut self, evento: &KeyEvent) {
        match &evento.logical_key {
            Key::Named(NamedKey::Enter) => {
                let texto = self.barra.texto.clone();
                self.barra.editando = false;
                match direccion_escrita(&texto) {
                    Some(u) => return self.navegar(u, true),
                    None => self.estado = Some(format!("«{texto}» no es una dirección")),
                }
            }
            Key::Named(NamedKey::Escape) => self.barra.editando = false,
            Key::Named(NamedKey::Backspace) => self.barra.borrar(),
            _ => {
                if let Some(t) = evento.text.as_deref().filter(|_| !self.teclas.control_key() && !self.teclas.alt_key())
                {
                    self.barra.escribir(t);
                }
            }
        }
        self.pintar_barra();
    }
}
