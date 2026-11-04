//! La ventana y la página, cada una en su hilo, para que desplazar no espere nunca al JavaScript:
//!
//! - **El hilo de la ventana** (el de `winit`) solo reparte eventos. La rueda la atiende él
//!   mismo: calcula el desplazamiento y se lo manda al compositor, que repinta al momento.
//! - **El hilo de la página** tiene el `Navegador`: JavaScript, estilo, maquetación, clics,
//!   teclado y red. Recibe los demás eventos por un canal y, entre uno y otro, corre los
//!   temporizadores de la página.
//! - El desplazamiento y el alto del documento son de los dos ([`Desplazamiento`], atómicos).

use crate::compositor::Compositor;
use crate::navegador::{Evento, Navegador};
use quimera_gpu::Gpu;
use quimera_pintar::Pintor;
use quimera_red::Url;
use quimera_texto::Fuentes;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

/// Lo que el hilo de la ventana le pasa al de la página.
pub enum Mensaje {
    /// Un evento de la ventana (todos menos la rueda y cerrar).
    Ventana(WindowEvent),
    /// Algo que llegó de la red (o de otro hilo).
    Red(Evento),
    /// La ventana desplazó la página (para decodificar las imágenes que entran en la vista).
    Desplazado,
}

/// El desplazamiento de la página y el alto del documento, en píxeles de pantalla.
#[derive(Default)]
pub struct Desplazamiento {
    desp: AtomicU32,
    alto: AtomicU32,
}

impl Desplazamiento {
    /// El desplazamiento.
    pub fn valor(&self) -> f32 {
        f32::from_bits(self.desp.load(Ordering::Relaxed))
    }

    /// Pone el alto del documento (tras maquetar).
    pub fn poner_alto(&self, alto: f32) {
        self.alto.store(alto.to_bits(), Ordering::Relaxed);
    }

    /// Desplaza `cuanto` (sin pasarse de los bordes, con `visible` píxeles de vista) y devuelve
    /// el desplazamiento nuevo. Desde cualquiera de los dos hilos.
    pub fn mover(&self, cuanto: f32, visible: f32) -> f32 {
        let alto = f32::from_bits(self.alto.load(Ordering::Relaxed));
        let nuevo = (self.valor() + cuanto).clamp(0.0, (alto - visible).max(0.0));
        self.desp.store(nuevo.to_bits(), Ordering::Relaxed);
        nuevo
    }

    /// Vuelve arriba (otra página).
    pub fn arriba(&self) {
        self.desp.store(0.0f32.to_bits(), Ordering::Relaxed);
    }
}

/// El hilo de la ventana.
pub struct Aplicacion {
    /// Lo necesario para crear el `Navegador` en su hilo (hasta que se abre la ventana).
    inicio: Option<(Url, Fuentes)>,
    proxy: EventLoopProxy<Evento>,
    ventana: Option<Arc<Window>>,
    pagina: Option<Sender<Mensaje>>,
    compositor: Option<Compositor>,
    desp: Arc<Desplazamiento>,
}

impl Aplicacion {
    pub fn nueva(url: Url, fuentes: Fuentes, proxy: EventLoopProxy<Evento>) -> Aplicacion {
        Aplicacion {
            inicio: Some((url, fuentes)),
            proxy,
            ventana: None,
            pagina: None,
            compositor: None,
            desp: Arc::default(),
        }
    }

    fn mandar(&self, m: Mensaje) {
        if let Some(p) = &self.pagina {
            let _ = p.send(m);
        }
    }

    /// La rueda, aquí mismo: el compositor repinta sin esperar a la página.
    fn rueda(&self, delta: MouseScrollDelta) {
        let (Some(v), Some(c)) = (&self.ventana, &self.compositor) else { return };
        let escala = v.scale_factor() as f32;
        let cuanto = match delta {
            MouseScrollDelta::LineDelta(_, y) => -y * 48.0 * escala,
            MouseScrollDelta::PixelDelta(p) => -p.y as f32,
        };
        let barra = crate::barra::ALTO * escala;
        let visible = v.inner_size().height as f32 - barra;
        let desp = self.desp.mover(cuanto, visible);
        c.desplazar(desp - barra);
        self.mandar(Mensaje::Desplazado);
    }
}

impl ApplicationHandler<Evento> for Aplicacion {
    fn resumed(&mut self, bucle: &ActiveEventLoop) {
        let Some((url, fuentes)) = self.inicio.take() else { return };
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
        let gpu = match pollster::block_on(Gpu::nueva(ventana.clone(), tam.width, tam.height, crate::TAM_ATLAS)) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("no se puede usar la GPU: {e}");
                bucle.exit();
                return;
            }
        };
        let compositor = Compositor::nuevo(gpu);
        let (tx, rx) = channel();
        let (v, c, proxy, desp) = (ventana.clone(), compositor.clone(), self.proxy.clone(), self.desp.clone());
        let hilo = std::thread::Builder::new().name(String::from("pagina")).spawn(move || {
            let mut nav = Navegador::nuevo(url, Pintor::nuevo(fuentes, crate::TAM_ATLAS), proxy, desp);
            nav.abrir(v, c);
            bucle_de_pagina(nav, &rx);
        });
        if let Err(e) = hilo {
            eprintln!("no se pudo arrancar el hilo de la página: {e}");
            bucle.exit();
            return;
        }
        (self.ventana, self.compositor, self.pagina) = (Some(ventana), Some(compositor), Some(tx));
    }

    fn user_event(&mut self, _bucle: &ActiveEventLoop, evento: Evento) {
        self.mandar(Mensaje::Red(evento));
    }

    fn window_event(&mut self, bucle: &ActiveEventLoop, _id: WindowId, evento: WindowEvent) {
        match evento {
            WindowEvent::CloseRequested => bucle.exit(),
            WindowEvent::MouseWheel { delta, .. } => self.rueda(delta),
            otro => self.mandar(Mensaje::Ventana(otro)),
        }
    }
}

/// El hilo de la página: atiende lo que llega y, entre medias, corre sus temporizadores.
fn bucle_de_pagina(mut nav: Navegador, rx: &Receiver<Mensaje>) {
    let mut proximo: Option<Instant> = None;
    loop {
        let primero = match proximo {
            Some(t) => match rx.recv_timeout(t.saturating_duration_since(Instant::now())) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            },
        };
        for m in primero.into_iter().chain(rx.try_iter()) {
            atender(&mut nav, m);
        }
        proximo = nav.latido();
    }
}

fn atender(nav: &mut Navegador, m: Mensaje) {
    match m {
        Mensaje::Red(e) => nav.evento(e),
        Mensaje::Desplazado => nav.tras_desplazar(),
        Mensaje::Ventana(ev) => match ev {
            WindowEvent::Resized(tam) => {
                nav.maq_todo = true;
                if let Some(a) = &nav.abierta {
                    a.gpu.redimensionar(tam.width, tam.height);
                }
                nav.maquetar();
                nav.pintar_barra();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                nav.maq_todo = true;
                nav.maquetar();
            }
            WindowEvent::CursorMoved { position, .. } => nav.raton(position.x as f32, position.y as f32),
            WindowEvent::MouseInput { state: ElementState::Released, button, .. } => match button {
                MouseButton::Left => nav.clic(),
                MouseButton::Back => nav.atras(),
                MouseButton::Forward => nav.adelante(),
                _ => {}
            },
            WindowEvent::ModifiersChanged(m) => nav.teclas = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => nav.tecla(&event),
            WindowEvent::RedrawRequested => nav.dibujar(),
            _ => {}
        },
    }
}
