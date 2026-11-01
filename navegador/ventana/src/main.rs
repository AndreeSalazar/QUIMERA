//! La ventana de QUIMERA: abre una página y la dibuja con wgpu.
//!
//! Uso: `cargo run --release -p quimera-ventana -- [https://... | pagina.html]` (sin argumento,
//! la primera página de ejemplo). La rueda del ratón, las flechas, Re Pág/Av Pág, Inicio y Fin desplazan;
//! al redimensionar se vuelve a maquetar con el ancho nuevo. F5 vuelve a leer la página.
//!
//! Sin ventana: `-- pagina.html --captura salida.png [ancho] [alto]` dibuja con la misma GPU en
//! una imagen y la guarda (para comprobar el pintado sin mirar la pantalla).
//!
//! Todo el camino es de QUIMERA: HTML → árbol → estilo → maquetación (con las medidas de las
//! fuentes de verdad) → cuadros → GPU.

mod pagina;
mod png;

use pagina::{Pagina, direccion};
use quimera_estilo::Opciones;
use quimera_gpu::Gpu;
use quimera_maquetacion::maquetar;
use quimera_pintar::{Pintor, color_gpu};
use quimera_red::{Cliente, Url};
use quimera_texto::Fuentes;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const TAM_ATLAS: u32 = 2048;

/// Lo que existe mientras la ventana está abierta.
struct Abierta {
    ventana: Arc<Window>,
    gpu: Gpu,
    /// Alto del documento y desplazamiento actual, en píxeles de pantalla.
    alto_documento: f32,
    desp: f32,
    fondo: [f32; 4],
}

struct Navegador {
    url: Url,
    cliente: Cliente,
    /// La página cargada (se vuelve a cargar con F5).
    pagina: Option<Pagina>,
    pintor: Pintor,
    abierta: Option<Abierta>,
}

impl Navegador {
    /// Lee, calcula estilos, maqueta y carga los cuadros en la GPU para el tamaño actual.
    fn maquetar(&mut self) {
        let Some(a) = &mut self.abierta else { return };
        let inicio = Instant::now();
        let tam = a.ventana.inner_size();
        let escala = a.ventana.scale_factor() as f32;
        let (ancho, alto) = (tam.width as f32 / escala, tam.height as f32 / escala);
        let pagina = self.pagina.get_or_insert_with(|| Pagina::cargar(&self.cliente, &self.url));
        let doc = pagina.documento();
        let externa = |href: &str| pagina.hoja(href);
        let estilos = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho, alto }, &externa);
        let m = maquetar(&doc, &estilos, &self.pintor.medidor(), ancho, alto);
        let cuadros = self.pintor.escena(&m, escala);
        a.gpu.cargar(&cuadros);
        if let Some((pixeles, desde, hasta)) = self.pintor.atlas_cambiado() {
            a.gpu.subir_atlas(pixeles, desde, hasta);
        }
        a.alto_documento = m.alto * escala;
        a.fondo = color_gpu(m.fondo);
        a.desp = a.desp.clamp(0.0, (a.alto_documento - tam.height as f32).max(0.0));
        let titulo = doc.buscar("title").map(|t| doc.texto(t).trim().to_string()).filter(|t| !t.is_empty());
        let sitio =
            if pagina.url.anfitrion.is_empty() { String::new() } else { format!(" · {}", pagina.url.anfitrion) };
        a.ventana.set_title(&format!("{}{sitio} — QUIMERA", titulo.as_deref().unwrap_or("sin título")));
        eprintln!(
            "{}: red {:.0} ms · {} nodos, {} cuadros, {:.0}×{:.0} px en {:.1} ms",
            pagina.url,
            pagina.ms_red,
            doc.total(),
            cuadros.len(),
            ancho,
            m.alto,
            inicio.elapsed().as_secs_f64() * 1e3
        );
        a.ventana.request_redraw();
    }

    fn desplazar(&mut self, cuanto: f32) {
        let Some(a) = &mut self.abierta else { return };
        let maximo = (a.alto_documento - a.ventana.inner_size().height as f32).max(0.0);
        a.desp = (a.desp + cuanto).clamp(0.0, maximo);
        a.ventana.request_redraw();
    }

    fn tecla(&mut self, tecla: &Key) {
        let Some(a) = &self.abierta else { return };
        let pagina = a.ventana.inner_size().height as f32 * 0.9;
        let linea = 40.0 * a.ventana.scale_factor() as f32;
        match tecla {
            Key::Named(NamedKey::ArrowDown) => self.desplazar(linea),
            Key::Named(NamedKey::ArrowUp) => self.desplazar(-linea),
            Key::Named(NamedKey::PageDown | NamedKey::Space) => self.desplazar(pagina),
            Key::Named(NamedKey::PageUp) => self.desplazar(-pagina),
            Key::Named(NamedKey::Home) => self.desplazar(f32::MIN),
            Key::Named(NamedKey::End) => self.desplazar(f32::MAX),
            Key::Named(NamedKey::F5) => {
                self.pagina = None;
                self.maquetar();
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for Navegador {
    fn resumed(&mut self, bucle: &ActiveEventLoop) {
        if self.abierta.is_some() {
            return;
        }
        let atributos =
            Window::default_attributes().with_title("QUIMERA").with_inner_size(LogicalSize::new(1024.0, 768.0));
        let ventana = match bucle.create_window(atributos) {
            Ok(v) => Arc::new(v),
            Err(e) => {
                eprintln!("no se puede abrir la ventana: {e}");
                bucle.exit();
                return;
            }
        };
        let tam = ventana.inner_size();
        match pollster::block_on(Gpu::nueva(ventana.clone(), tam.width, tam.height, TAM_ATLAS)) {
            Ok(gpu) => {
                self.abierta = Some(Abierta { ventana, gpu, alto_documento: 0.0, desp: 0.0, fondo: [1.0; 4] });
                self.maquetar();
            }
            Err(e) => {
                eprintln!("no se puede usar la GPU: {e}");
                bucle.exit();
            }
        }
    }

    fn window_event(&mut self, bucle: &ActiveEventLoop, _id: WindowId, evento: WindowEvent) {
        match evento {
            WindowEvent::CloseRequested => bucle.exit(),
            WindowEvent::Resized(tam) => {
                if let Some(a) = &mut self.abierta {
                    a.gpu.redimensionar(tam.width, tam.height);
                }
                self.maquetar();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.maquetar(),
            WindowEvent::MouseWheel { delta, .. } => {
                let escala = self.abierta.as_ref().map_or(1.0, |a| a.ventana.scale_factor() as f32);
                let cuanto = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * 48.0 * escala,
                    MouseScrollDelta::PixelDelta(p) => -p.y as f32,
                };
                self.desplazar(cuanto);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.tecla(&event.logical_key)
            }
            WindowEvent::RedrawRequested => {
                if let Some(a) = &mut self.abierta
                    && let Err(e) = a.gpu.dibujar(a.fondo, a.desp)
                {
                    eprintln!("no se pudo dibujar: {e}");
                }
            }
            _ => {}
        }
    }
}

/// Dibuja la página con la GPU, sin ventana, y la guarda como PNG.
fn capturar(url: &Url, salida: &str, ancho: u32, alto: u32) -> Result<(), String> {
    let mut pintor = Pintor::nuevo(Fuentes::del_sistema(), TAM_ATLAS);
    let mut gpu = pollster::block_on(Gpu::sin_ventana(ancho, alto, TAM_ATLAS))?;
    let pagina = Pagina::cargar(&Cliente::nuevo(), url);
    let doc = pagina.documento();
    let externa = |href: &str| pagina.hoja(href);
    let (w, h) = (ancho as f32, alto as f32);
    let estilos = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho: w, alto: h }, &externa);
    let m = maquetar(&doc, &estilos, &pintor.medidor(), w, h);
    let cuadros = pintor.escena(&m, 1.0);
    gpu.cargar(&cuadros);
    if let Some((pixeles, desde, hasta)) = pintor.atlas_cambiado() {
        gpu.subir_atlas(pixeles, desde, hasta);
    }
    let rgba = gpu.capturar(color_gpu(m.fondo), 0.0)?;
    std::fs::write(salida, png::codificar(ancho, alto, &rgba)).map_err(|e| format!("{salida}: {e}"))?;
    eprintln!("{}: red {:.0} ms · {} cuadros → {salida} ({ancho}×{alto})", pagina.url, pagina.ms_red, cuadros.len());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--captura") {
        let url = direccion(args.get(1).filter(|a| *a != "--captura").map_or("", String::as_str));
        let salida = args.get(i + 1).cloned().unwrap_or_else(|| String::from("captura.png"));
        let numero = |k: usize, d: u32| args.get(i + k).and_then(|a| a.parse().ok()).unwrap_or(d);
        if let Err(e) = capturar(&url, &salida, numero(2, 1024), numero(3, 768)) {
            eprintln!("no se pudo capturar: {e}");
            std::process::exit(1);
        }
        return;
    }
    let ejemplo = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/ejemplos/primera.html");
    let url = direccion(&std::env::args().nth(1).unwrap_or_else(|| String::from(ejemplo)));
    let fuentes = Fuentes::del_sistema();
    if fuentes.total() == 0 {
        eprintln!("aviso: no se encontraron fuentes del sistema; el texto no se verá");
    }
    let pintor = Pintor::nuevo(fuentes, TAM_ATLAS);
    let mut navegador = Navegador { url, cliente: Cliente::nuevo(), pagina: None, pintor, abierta: None };
    let bucle = match EventLoop::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("no se puede crear el bucle de eventos: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = bucle.run_app(&mut navegador) {
        eprintln!("la ventana terminó con un error: {e}");
        std::process::exit(1);
    }
}
