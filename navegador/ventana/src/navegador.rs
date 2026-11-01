//! El navegador de una ventana: la página, la barra, el historial y lo que hace cada acción
//! (navegar, pulsar un enlace, escribir una dirección, desplazar).
//!
//! Las páginas se cargan en otro hilo: la ventana sigue respondiendo mientras llegan, y cuando
//! llega una se avisa al bucle de eventos (`Llegada`). Si mientras tanto se pidió otra, la vieja
//! se descarta.

use crate::barra::{self, Barra, Vista, Zona};
use crate::historial::Historial;
use crate::pagina::{Pagina, direccion_escrita};
use quimera_estilo::Opciones;
use quimera_gpu::Gpu;
use quimera_maquetacion::{Maquetacion, maquetar};
use quimera_pintar::{Pintor, color_gpu};
use quimera_red::{Cliente, Url};
use std::sync::Arc;
use std::time::Instant;
use winit::event::KeyEvent;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window};

/// Una página que terminó de cargar en su hilo.
pub struct Llegada {
    id: u64,
    pagina: Pagina,
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
    proxy: EventLoopProxy<Llegada>,
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
}

impl Navegador {
    pub fn nuevo(url: Url, pintor: Pintor, proxy: EventLoopProxy<Llegada>) -> Navegador {
        Navegador {
            cliente: Arc::new(Cliente::nuevo()),
            proxy,
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
            let pagina = Pagina::cargar(&cliente, &url);
            let _ = proxy.send_event(Llegada { id, pagina });
        });
        self.pintar_barra();
    }

    /// Llegó una página: si es la última pedida, se muestra (y se va a su `#ancla`).
    pub fn llegada(&mut self, l: Llegada) {
        if l.id != self.id_carga {
            return;
        }
        let fragmento = self.url.fragmento.clone();
        let mut final_ = l.pagina.url.clone();
        final_.fragmento = fragmento.clone();
        self.historial.sustituir(final_.clone());
        self.url = final_;
        self.pagina = Some(l.pagina);
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
        let doc = pagina.documento();
        let externa = |href: &str| pagina.hoja(href);
        let estilos = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho, alto }, &externa);
        let m = maquetar(&doc, &estilos, &self.pintor.medidor(), ancho, alto);
        let cuadros = self.pintor.escena(&m, escala);
        a.gpu.cargar(&cuadros);
        a.fondo = color_gpu(m.fondo);
        let titulo = doc.buscar("title").map(|t| doc.texto(t).trim().to_string()).filter(|t| !t.is_empty());
        let sitio =
            if pagina.url.anfitrion.is_empty() { String::new() } else { format!(" · {}", pagina.url.anfitrion) };
        a.ventana.set_title(&format!("{}{sitio} — QUIMERA", titulo.as_deref().unwrap_or("sin título")));
        eprintln!(
            "{}: red {:.0} ms · {} nodos, {} cuadros, {} enlaces, maquetado en {:.1} ms",
            pagina.url,
            pagina.ms_red,
            doc.total(),
            cuadros.len(),
            m.enlaces.len(),
            inicio.elapsed().as_secs_f64() * 1e3
        );
        a.maq = Some(m);
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
                let href = self
                    .abierta
                    .as_ref()
                    .and_then(|a| a.maq.as_ref())
                    .and_then(|m| m.enlace_en(x / e, (y - barra_px + desp) / e))
                    .map(String::from);
                if let Some(h) = href {
                    self.seguir(&h);
                }
            }
        }
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
