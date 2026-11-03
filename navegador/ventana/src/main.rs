//! La ventana de QUIMERA: un navegador con barra de dirección, enlaces e historial.
//!
//! Uso: `cargo run --release -p quimera-ventana -- [https://... | pagina.html]` (sin argumento,
//! la primera página de ejemplo).
//!
//! - **Enlaces**: el cursor cambia sobre ellos, abajo a la izquierda se ve a dónde llevan, y un
//!   clic los sigue (los `#ancla` de la misma página desplazan sin recargar).
//! - **Barra**: ← → ⟳ y el campo de dirección (clic, Ctrl+L o F6; Enter para ir, Esc para salir).
//! - **Historial**: Alt+← / Alt+→ y los botones laterales del ratón.
//! - **Desplazar**: rueda, flechas, Re Pág/Av Pág, espacio, Inicio, Fin. F5 o Ctrl+R recargan.
//! - Las páginas se cargan en otro hilo: la ventana no se congela mientras llegan.
//!
//! Sin ventana: `-- pagina.html --captura salida.png [ancho] [alto]` dibuja con la misma GPU en
//! una imagen (para comprobar el pintado sin mirar la pantalla). Con `--seguir N`, antes sigue
//! el enlace número N de la página; con `--escribir "texto"`, escribe en el primer campo de la
//! página y envía su formulario (una búsqueda, por ejemplo).

mod barra;
mod formularios;
mod guiones;
mod historial;
mod imagenes;
mod navegador;
mod pagina;
mod perfil;
mod peticiones;
mod png;
mod tipografias;

use imagenes::Imagenes;
use navegador::{Evento, Navegador};
use pagina::{Pagina, direccion};
use quimera_estilo::Opciones;
use quimera_gpu::Gpu;
use quimera_maquetacion::maquetar_con_imagenes;
use quimera_pintar::{Pintor, color_gpu};
use quimera_red::{Cliente, Url};
use quimera_texto::Fuentes;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const TAM_ATLAS: u32 = 2048;

impl ApplicationHandler<Evento> for Navegador {
    fn resumed(&mut self, bucle: &ActiveEventLoop) {
        if self.abierta.is_some() {
            return;
        }
        let atributos =
            Window::default_attributes().with_title("QUIMERA").with_inner_size(LogicalSize::new(1100.0, 800.0));
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
            Ok(gpu) => self.abrir(ventana, gpu),
            Err(e) => {
                eprintln!("no se puede usar la GPU: {e}");
                bucle.exit();
            }
        }
    }

    fn user_event(&mut self, _bucle: &ActiveEventLoop, evento: Evento) {
        self.evento(evento);
    }

    /// Antes de dormir: los temporizadores de la página que ya tocan, y despertar para el siguiente.
    fn about_to_wait(&mut self, bucle: &ActiveEventLoop) {
        match self.latido() {
            Some(cuando) => bucle.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(cuando)),
            None => bucle.set_control_flow(winit::event_loop::ControlFlow::Wait),
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
                self.pintar_barra();
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
            WindowEvent::CursorMoved { position, .. } => self.raton(position.x as f32, position.y as f32),
            WindowEvent::MouseInput { state: ElementState::Released, button, .. } => match button {
                MouseButton::Left => self.clic(),
                MouseButton::Back => self.atras(),
                MouseButton::Forward => self.adelante(),
                _ => {}
            },
            WindowEvent::ModifiersChanged(m) => self.teclas = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => self.tecla(&event),
            WindowEvent::RedrawRequested => self.dibujar(),
            _ => {}
        }
    }
}

/// Lo que se hace en la página antes de capturar.
enum Antes {
    /// Seguir el enlace número `n` de su lista de enlaces.
    Seguir(usize),
    /// Escribir en el primer campo de texto y enviar su formulario.
    Escribir(String),
}

/// A dónde lleva `antes` desde la página: la dirección y, si es un POST, su cuerpo.
fn destino_previo(
    pagina: &Pagina,
    m: &quimera_maquetacion::Maquetacion,
    antes: &Antes,
) -> Result<(Url, Option<Vec<u8>>), String> {
    match antes {
        Antes::Seguir(n) => {
            let enlace = m.enlaces.get(*n).ok_or_else(|| format!("la página tiene {} enlaces", m.enlaces.len()))?;
            let destino = pagina.base.unir(&enlace.href).map_err(|e| e.to_string())?;
            eprintln!("enlace {n}: {} → {destino}", enlace.href);
            Ok((destino, None))
        }
        Antes::Escribir(texto) => {
            let doc = pagina.documento();
            let campo = m
                .controles
                .iter()
                .map(|c| c.nodo)
                .find(|&c| formularios::es_campo(&doc, c))
                .ok_or("la página no tiene campos donde escribir")?;
            let mut f = formularios::Estado::default();
            f.pulsar(&doc, campo);
            f.escribir(&doc, texto);
            let (url, cuerpo) = f.envio(&doc, campo, &pagina.base).ok_or("el campo no está en un formulario")?;
            let metodo = if cuerpo.is_some() { "POST" } else { "GET" };
            let enviado = cuerpo.as_deref().map(String::from_utf8_lossy).unwrap_or_default();
            eprintln!("formulario: {metodo} {url} {enviado}");
            Ok((url, cuerpo))
        }
    }
}

/// Dibuja la página con la GPU, sin ventana y con la barra, y la guarda como PNG. Con
/// `antes`, primero sigue un enlace o envía un formulario.
fn capturar(url: &Url, salida: &str, (ancho, alto): (u32, u32), antes: Option<Antes>) -> Result<(), String> {
    let mut pintor = Pintor::nuevo(Fuentes::del_sistema(), TAM_ATLAS);
    let mut gpu = pollster::block_on(Gpu::sin_ventana(ancho, alto, TAM_ATLAS))?;
    let cliente = Arc::new(Cliente::nuevo());
    let mut pagina = Pagina::cargar(&cliente, url, None);
    let (w, h) = (ancho as f32, alto as f32 - barra::ALTO);
    let mut imagenes = cargar_imagenes(&cliente, &pagina);
    cargar_fuentes(&cliente, &pagina, &mut pintor);
    let mut m = maquetar_pagina(&pagina, &pintor, (w, h), &mut imagenes, &cliente);
    if let Some(a) = &antes {
        let (destino, cuerpo) = destino_previo(&pagina, &m, a)?;
        pagina = Pagina::cargar(&cliente, &destino, cuerpo.as_deref());
        imagenes = cargar_imagenes(&cliente, &pagina);
        pintor.olvidar_fuentes();
        cargar_fuentes(&cliente, &pagina, &mut pintor);
        m = maquetar_pagina(&pagina, &pintor, (w, h), &mut imagenes, &cliente);
    }
    gpu.cargar(&pintor.escena(&m, 1.0, &|s| imagenes.id(s)));
    // Solo se decodifican las imágenes que salen en la captura.
    for (id, bytes) in imagenes.por_decodificar(&imagenes::entre(&m, 0.0, h)) {
        match quimera_imagen::decodificar(&bytes) {
            Ok(i) => {
                imagenes.decodificada(id, true);
                gpu.subir_imagen(id, i.ancho, i.alto, &i.rgba);
            }
            Err(e) => eprintln!("imagen {id}: {e}"),
        }
    }
    let url_texto = pagina.url.to_string();
    let vista = barra::Vista {
        ancho: w,
        alto: alto as f32,
        escala: 1.0,
        url: &url_texto,
        atras: antes.is_some(),
        adelante: false,
        estado: None,
    };
    gpu.cargar_fijos(&barra::Barra::default().cuadros(&mut pintor, &vista));
    if let Some((pixeles, desde, hasta)) = pintor.atlas_cambiado() {
        gpu.subir_atlas(pixeles, desde, hasta);
    }
    let rgba = gpu.capturar(color_gpu(m.fondo), -barra::ALTO)?;
    std::fs::write(salida, png::codificar(ancho, alto, &rgba)).map_err(|e| format!("{salida}: {e}"))?;
    eprintln!(
        "{}: red {:.0} ms · {} enlaces, {} imágenes → {salida} ({ancho}×{alto})",
        pagina.url,
        pagina.ms_red,
        m.enlaces.len(),
        imagenes.total()
    );
    Ok(())
}

/// `QUIMERA_DEPURAR=nombre`: el estilo calculado del primer elemento con esa clase, id o etiqueta
/// (tras el JavaScript) y de sus antepasados, para entender por qué algo no se ve como en otro
/// navegador.
fn depurar(doc: &quimera_html::Documento, estilos: &quimera_estilo::Estilos, que: &str) {
    let tiene = |id| {
        doc.elemento(id).is_some_and(|e| {
            e.nombre == que
                || e.atributo("id") == Some(que)
                || e.atributo("class").is_some_and(|c| c.split_whitespace().any(|x| x == que))
        })
    };
    let Some(id) = doc.descendientes(quimera_html::Documento::RAIZ).find(|&id| tiene(id)) else {
        return eprintln!("ningún elemento es {que}");
    };
    let mut actual = Some(id);
    while let Some(n) = actual {
        if let (Some(el), Some(e)) = (doc.elemento(n), estilos.de(n)) {
            eprintln!(
                "<{} class={:?}> display={:?} ancho={:?} alto={:?} flotar={:?} posicion={:?} visible={} fondo={:?} color={:?} letra={}",
                el.nombre,
                el.atributo("class").unwrap_or(""),
                e.display,
                e.ancho,
                e.alto,
                e.flotar,
                e.colocacion.posicion,
                e.visible,
                e.fondo,
                e.color,
                e.tam_letra
            );
        }
        actual = doc.nodo(n).padre;
    }
}

/// Las fuentes de la página (`@font-face`), esperando a que lleguen (para capturar).
fn cargar_fuentes(cliente: &Cliente, pagina: &Pagina, pintor: &mut Pintor) {
    for p in tipografias::pedidas(pagina) {
        let w = &p.web;
        match tipografias::descargar(cliente, &p.url).map(|b| pintor.anadir_fuente(&w.familia, w.negrita, w.cursiva, b))
        {
            Ok(true) => {
                eprintln!(
                    "fuente {} ({}{}) ← {}",
                    w.familia,
                    if w.negrita { "negrita" } else { "normal" },
                    if w.cursiva { ", cursiva" } else { "" },
                    p.url
                );
            }
            Ok(false) => eprintln!("fuente {}: formato que QUIMERA no lee", p.url),
            Err(e) => eprintln!("fuente {}: {e}", p.url),
        }
    }
}

/// Todas las imágenes de la página, esperando a que lleguen (para capturar).
fn cargar_imagenes(cliente: &Arc<Cliente>, pagina: &Pagina) -> Imagenes {
    let mut imagenes = Imagenes::default();
    let fuentes = imagenes.nuevas(&pagina.documento());
    esperar_imagenes(cliente, &pagina.base, fuentes, &mut imagenes);
    imagenes
}

/// Descarga `fuentes` (comprimidas, con su tamaño) y espera a que lleguen todas.
fn esperar_imagenes(cliente: &Arc<Cliente>, base: &Url, fuentes: Vec<String>, imagenes: &mut Imagenes) {
    let total = fuentes.len();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    imagenes::descargar_todas(cliente.clone(), base.clone(), fuentes, move |src, r| {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send((src, r));
        }
    });
    for (src, r) in rx.iter().take(total) {
        match r {
            Ok(i) => imagenes.llegada(src, i),
            Err(e) => eprintln!("imagen {src}: {e}"),
        }
    }
}

fn maquetar_pagina(
    pagina: &Pagina,
    pintor: &Pintor,
    (ancho, alto): (f32, f32),
    imagenes: &mut Imagenes,
    cliente: &Arc<Cliente>,
) -> quimera_maquetacion::Maquetacion {
    // Con su JavaScript: los scripts, los temporizadores del primer segundo y lo que pidan a la red.
    let mut guion = guiones::Guion::preparar(pagina, (ancho, alto));
    // `QUIMERA_ESPERA=ms`: cuánto tiempo de la página se adelanta (1 s si no se dice).
    let espera = std::env::var("QUIMERA_ESPERA").ok().and_then(|e| e.parse().ok()).unwrap_or(1000.0);
    guion.adelantar(espera, &|p| peticiones::atender(cliente, &pagina.url, &pagina.base, p));
    // `QUIMERA_GUION=archivo.js`: un guion propio tras la carga, para mirar la página por dentro.
    if let Some(codigo) = std::env::var("QUIMERA_GUION").ok().and_then(|f| std::fs::read_to_string(f).ok()) {
        guion.dom.ejecutar(&codigo);
        guion.informar();
    }
    let doc = guion.dom.documento();
    // Las imágenes que añadieron los scripts.
    let nuevas = imagenes.nuevas(&doc);
    esperar_imagenes(cliente, &pagina.base, nuevas, imagenes);
    let externa = |href: &str| pagina.hoja(href);
    let estilos = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho, alto }, &externa);
    if let Ok(que) = std::env::var("QUIMERA_DEPURAR") {
        depurar(&doc, &estilos, &que);
    }
    maquetar_con_imagenes(&doc, &estilos, &pintor.medidor(), (ancho, alto), &|s| imagenes.tam(s))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--captura") {
        let url = direccion(args.get(1).filter(|a| *a != "--captura").map_or("", String::as_str));
        let salida = args.get(i + 1).cloned().unwrap_or_else(|| String::from("captura.png"));
        let numero = |k: usize, d: u32| args.get(i + k).and_then(|a| a.parse().ok()).unwrap_or(d);
        let tras = |k: &str| args.iter().position(|a| a == k).and_then(|k| args.get(k + 1));
        let antes = tras("--seguir")
            .and_then(|n| n.parse().ok())
            .map(Antes::Seguir)
            .or_else(|| tras("--escribir").map(|t| Antes::Escribir(t.clone())));
        if let Err(e) = capturar(&url, &salida, (numero(2, 1024), numero(3, 768)), antes) {
            eprintln!("no se pudo capturar: {e}");
            std::process::exit(1);
        }
        return;
    }
    let ejemplo = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/ejemplos/primera.html");
    let url = direccion(&args.get(1).cloned().unwrap_or_else(|| String::from(ejemplo)));
    let fuentes = Fuentes::del_sistema();
    if fuentes.total() == 0 {
        eprintln!("aviso: no se encontraron fuentes del sistema; el texto no se verá");
    }
    let bucle = match EventLoop::<Evento>::with_user_event().build() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("no se puede crear el bucle de eventos: {e}");
            std::process::exit(1);
        }
    };
    let mut navegador = Navegador::nuevo(url, Pintor::nuevo(fuentes, TAM_ATLAS), bucle.create_proxy());
    if let Err(e) = bucle.run_app(&mut navegador) {
        eprintln!("la ventana terminó con un error: {e}");
        std::process::exit(1);
    }
}
