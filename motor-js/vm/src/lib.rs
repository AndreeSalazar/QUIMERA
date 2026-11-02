//! Máquina virtual de QUIMERA: ejecuta el bytecode de registros del frontend.
//!
//! - **Registros en la pila de raíces del GC**: cada llamada abre un marco en esa pila
//!   (`[cierre, this, entorno, objeto nuevo, r0, r1...]`). Si el GC mueve un objeto, los
//!   registros que lo apuntan se actualizan solos; nunca queda un puntero viejo.
//! - **Sin recursión de Rust**: las llamadas de JavaScript apilan marcos en un `Vec`, así
//!   que una recursión profunda de JavaScript no agota la pila nativa.
//! - **Cachés en línea**: cada `LeerProp`/`EscribirProp`/`LeerGlobal` recuerda la forma y el
//!   hueco de la última vez (como el Baseline Interpreter de SpiderMonkey, en simple).
//! - **Enteros rápidos**: la aritmética entre enteros no pasa por `f64`.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod accesores;
pub mod anfitrion;
mod conversion;
mod generadores;
mod ic;
mod interprete;
mod iteracion;
mod llamadas;
pub mod matematicas;
pub(crate) mod nativas;
pub mod operaciones;
#[cfg(feature = "perfil")]
mod perfil;
mod propiedades;
mod proxy;
mod trazas;
pub mod trigonometria;

use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
pub use anfitrion::{Anfitrion, Argumentos};
use quimera_frontend::Stencil;
use quimera_frontend::bytecode::{Constante, Funcion};
pub use quimera_objetos::Valor;
use quimera_objetos::{Config, Estadisticas, Mundo};

/// Raíces fijas al fondo de la pila: el objeto global, los prototipos y la excepción en curso.
const R_GLOBAL: usize = 0;
const R_OBJETO: usize = 1;
const R_FUNCION: usize = 2;
const R_ARREGLO: usize = 3;
const R_EXCEPCION: usize = 4;
const R_NUMERO: usize = 5;
const R_CADENA: usize = 6;
const R_FECHA: usize = 7;
const R_ERROR: usize = 8;
const R_REGEXP: usize = 9;
/// Lo que el anfitrión guarda para que dure (ver `anfitrion.rs`).
const R_ANFITRION: usize = 10;
const N_FIJAS: usize = 11;
/// Huecos de cada marco antes de sus registros: cierre, this, entorno, objeto nuevo.
const CABECERA: usize = 4;

/// Un error que llega a quien usa el motor: de sintaxis o una excepción que nadie capturó.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorJs {
    /// El mensaje (para una excepción, su valor convertido a texto).
    pub mensaje: String,
    /// La pila de llamadas donde nació (las líneas `    at ...` de su `stack`), o vacío.
    pub traza: String,
}

impl ErrorJs {
    /// Un error sin traza (de sintaxis, del anfitrión).
    pub fn nuevo(mensaje: String) -> ErrorJs {
        ErrorJs { mensaje, traza: String::new() }
    }

    /// El mensaje seguido de su traza, como lo muestra la consola de un navegador.
    pub fn completo(&self) -> String {
        alloc::format!("{}{}", self.mensaje, self.traza)
    }
}

/// El primer nivel de la caché en línea de una instrucción: la forma vista y el hueco de una
/// propiedad propia. Si no basta, `sitio` apunta a sus stubs (`ic.rs`).
#[derive(Debug, Clone, Copy)]
struct Cache {
    forma: u32,
    hueco: u32,
    sitio: u32,
}

const CACHE_VACIA: Cache = Cache { forma: u32::MAX, hueco: 0, sitio: u32::MAX };

/// Una función del Stencil, lista para ejecutar.
struct Cargada {
    f: Funcion,
    /// Sus constantes ya como valores.
    constantes: Vec<Valor>,
    /// Átomo del Stencil → cadena del mundo (compartido por las funciones de un mismo programa).
    atomos: Rc<Vec<u32>>,
    /// Primera función de su programa en la tabla global (los índices del Stencil son relativos).
    desplazamiento: u32,
    /// Una caché por instrucción.
    caches: Vec<Cache>,
    /// De dónde viene su programa (la URL del script), para las trazas.
    origen: Rc<str>,
}

/// Una llamada en curso.
#[derive(Debug, Clone, Copy)]
struct Marco {
    funcion: u32,
    pc: usize,
    /// Dónde empiezan sus registros en la pila de raíces.
    base: usize,
    /// Dónde escribir el resultado (posición absoluta en la pila), si hay que escribirlo.
    destino: Option<usize>,
    /// Es un `new`: si la función no devuelve un objeto, el resultado es el objeto nuevo.
    construir: bool,
    /// Desde dónde se sueltan las raíces al cerrarlo: el principio de su marco o, si una llamada
    /// sin recursión (`apply`) puso antes los argumentos en la pila, el principio de esos.
    soltar: usize,
    /// Es el marco de un generador (su objeto va en el hueco 3 de la cabecera).
    generador: bool,
}

/// Un `try` activo.
#[derive(Debug, Clone, Copy)]
struct Manejador {
    marcos: usize,
    captura: usize,
}

/// El motor de JavaScript de QUIMERA.
pub struct Motor {
    /// Objetos, formas y cadenas.
    pub mundo: Mundo,
    funciones: Vec<Cargada>,
    marcos: Vec<Marco>,
    manejadores: Vec<Manejador>,
    salida: Vec<String>,
    azar: u64,
    /// Reloj del anfitrión en milisegundos (para `Date`). Sin reloj, cada lectura avanza 1 ms.
    reloj: Option<fn() -> f64>,
    /// Azar del sistema que da el anfitrión (para `crypto.getRandomValues`).
    entropia: Option<fn(&mut [u8])>,
    tiempo_falso: f64,
    /// Cuántas claves de cada raíz fija son de la biblioteca (no enumerables en `for-in`).
    ocultas: [usize; N_FIJAS],
    /// Lo mismo para los prototipos del preludio en JavaScript, por su forma.
    formas_ocultas: alloc::collections::BTreeMap<u32, usize>,
    /// Expresiones regulares compiladas, una por cada par distinto (patrón, banderas).
    regexes: Vec<RegexCompilada>,
    /// (cadena del patrón, cadena de las banderas) → posición en `regexes`.
    indice_regex: alloc::collections::BTreeMap<(u32, u32), u32>,
    /// Memoria de trabajo del motor de expresiones regulares, reutilizada.
    memoria_regex: quimera_regex::Memoria,
    /// Los stubs de las cachés en línea (cada instrucción que los necesita apunta a uno).
    sitios: Vec<ic::Sitio>,
    /// Las formas de los prototipos que vigila cada stub, seguidas.
    cadenas_ic: Vec<u32>,
    /// El navegador (o quien use el motor), con sus funciones y objetos.
    anfitrion: Option<Rc<dyn Anfitrion>>,
    /// Las claves que tienen algún accesor (`get`/`set`), como filtro de bits (`accesores.rs`).
    filtro_accesores: [u64; accesores::N_FILTRO],
    /// Cuántos accesores se han definido (los stubs de «añadir» la comparan, `ic.rs`).
    epoca_accesores: u32,
    /// El nombre de cada función nativa (su `name`), por su número.
    nombres_nativas: alloc::collections::BTreeMap<u32, u32>,
    /// Cuántos símbolos se han creado (para que cada uno sea único).
    simbolos: u32,
    /// De dónde viene lo que se compila ahora (ver [`Motor::fijar_origen`]).
    origen: Rc<str>,
    /// Cuántas veces y cuánto tiempo se lleva cada tipo de instrucción (característica `perfil`).
    #[cfg(feature = "perfil")]
    perfil: perfil::Perfil,
}

/// Una expresión regular compilada con su texto original.
struct RegexCompilada {
    re: quimera_regex::Regex,
    fuente: String,
    banderas: String,
}

impl Default for Motor {
    fn default() -> Self {
        Self::nuevo(Config::default())
    }
}

impl Motor {
    /// Un motor con su objeto global y sus funciones nativas.
    pub fn nuevo(config: Config) -> Motor {
        let mut m = Motor {
            mundo: Mundo::nuevo(config),
            funciones: Vec::new(),
            marcos: Vec::new(),
            manejadores: Vec::new(),
            salida: Vec::new(),
            azar: 0x2545_F491_4F6C_DD1D,
            reloj: None,
            entropia: None,
            tiempo_falso: 0.0,
            ocultas: [0; N_FIJAS],
            formas_ocultas: alloc::collections::BTreeMap::new(),
            regexes: Vec::new(),
            indice_regex: alloc::collections::BTreeMap::new(),
            memoria_regex: quimera_regex::Memoria::default(),
            sitios: Vec::new(),
            cadenas_ic: Vec::new(),
            anfitrion: None,
            filtro_accesores: [0; accesores::N_FILTRO],
            epoca_accesores: 0,
            nombres_nativas: alloc::collections::BTreeMap::new(),
            simbolos: 0,
            origen: Rc::from(""),
            #[cfg(feature = "perfil")]
            perfil: perfil::Perfil::default(),
        };
        m.mundo.m.abrir_raices(N_FIJAS, Valor::INDEFINIDO);
        nativas::preparar(&mut m);
        nativas::preludio(&mut m);
        m
    }

    /// Ejecuta las microtareas pendientes (las reacciones de las promesas, `queueMicrotask`)
    /// hasta que no quede ninguna. Devuelve los errores que lanzaron.
    pub fn correr_microtareas(&mut self) -> Vec<String> {
        let Ok(errores) = self.llamar_global("__quimera_microtareas", &[]) else { return Vec::new() };
        let n = self.mundo.longitud(errores).unwrap_or(0);
        (0..n).map(|i| self.texto(self.mundo.leer_elemento(errores, i).unwrap_or(Valor::INDEFINIDO))).collect()
    }

    fn fija(&self, i: usize) -> Valor {
        self.mundo.m.pila()[i]
    }

    /// Da al motor un reloj (milisegundos) para `Date`. `no_std`: el reloj lo pone quien lo usa.
    pub fn con_reloj(&mut self, reloj: fn() -> f64) {
        self.reloj = Some(reloj);
    }

    /// Da al motor una fuente de azar del sistema: la usa `crypto.getRandomValues` y siembra
    /// `Math.random` (sin ella, `Math.random` repite la misma secuencia en cada página).
    pub fn con_entropia(&mut self, entropia: fn(&mut [u8])) {
        self.entropia = Some(entropia);
        let mut semilla = [0u8; 8];
        entropia(&mut semilla);
        let s = u64::from_le_bytes(semilla);
        if s != 0 {
            self.azar = s; // xorshift no admite 0
        }
    }

    /// `n` bytes de azar del sistema, si el anfitrión dio una fuente.
    pub(crate) fn bytes_aleatorios(&self, n: usize) -> Option<Vec<u8>> {
        let f = self.entropia?;
        let mut v = alloc::vec![0u8; n];
        f(&mut v);
        Some(v)
    }

    /// La hora actual en milisegundos.
    pub(crate) fn ahora(&mut self) -> f64 {
        match self.reloj {
            Some(r) => r(),
            None => {
                self.tiempo_falso += 1.0;
                self.tiempo_falso
            }
        }
    }

    /// Lo escrito con `console.log` / `print`.
    pub fn salida(&self) -> &[String] {
        &self.salida
    }

    /// Lo que ha hecho el GC.
    pub fn estadisticas(&self) -> Estadisticas {
        self.mundo.m.estadisticas()
    }

    /// Un valor como texto (`ToString`).
    pub fn texto(&self, v: Valor) -> String {
        operaciones::a_texto(&self.mundo, v)
    }

    /// Carga un Stencil y devuelve el índice global de su programa (su función 0).
    fn cargar(&mut self, s: Stencil) -> u32 {
        let atomos: Rc<Vec<u32>> = Rc::new(s.atomos.iter().map(|a| self.mundo.cadenas.internar(a)).collect());
        let desplazamiento = self.funciones.len() as u32;
        for f in s.funciones {
            let constantes = f
                .constantes
                .iter()
                .map(|k| match k {
                    Constante::Numero(n) => Valor::numero(*n),
                    Constante::Cadena(a) => {
                        let id = atomos.get(*a as usize).copied().unwrap_or(0);
                        Valor::cadena(quimera_objetos::Ref::desde_u32(id))
                    }
                })
                .collect();
            let caches = alloc::vec![CACHE_VACIA; f.codigo.len()];
            let origen = self.origen.clone();
            self.funciones.push(Cargada { f, constantes, atomos: atomos.clone(), desplazamiento, caches, origen });
        }
        desplazamiento
    }

    /// De dónde viene lo que se compile a partir de ahora (la URL de un script): sale en las
    /// trazas de los errores (`stack`).
    pub fn fijar_origen(&mut self, origen: &str) {
        self.origen = Rc::from(origen);
    }

    /// Compila y ejecuta un programa. Devuelve `undefined` o el error.
    pub fn ejecutar(&mut self, fuente: &str) -> Result<Valor, ErrorJs> {
        let stencil =
            quimera_frontend::compilar(fuente).map_err(|e| ErrorJs::nuevo(alloc::format!("SyntaxError: {e}")))?;
        self.ejecutar_stencil(stencil)
    }

    /// Ejecuta un programa ya compilado.
    fn ejecutar_stencil(&mut self, stencil: Stencil) -> Result<Valor, ErrorJs> {
        let programa = self.cargar(stencil);
        let entorno = Valor::NULO;
        let global = self.fija(R_GLOBAL);
        let proto = self.fija(R_FUNCION);
        let f = self.mundo.crear_funcion(programa, entorno, global, proto);
        self.llamar_valor(f, global, &[])
    }

    /// Compila un módulo (`import`/`export`) y devuelve la función que lo ejecuta con
    /// `(exportaciones, importar, meta)` (la llama el cargador de módulos del DOM).
    pub fn compilar_modulo(&mut self, fuente: &str) -> Result<Valor, ErrorJs> {
        let stencil = quimera_frontend::compilar_modulo(fuente)
            .map_err(|e| ErrorJs::nuevo(alloc::format!("SyntaxError: {e}")))?;
        self.ejecutar_stencil(stencil)?;
        let clave = self.mundo.cadenas.internar("__quimera_funcion");
        let global = self.fija(R_GLOBAL);
        let f = self.mundo.leer(global, clave).unwrap_or(Valor::INDEFINIDO);
        self.mundo.borrar(global, clave);
        Ok(f)
    }

    /// Llama a la función global `nombre` con estos argumentos.
    pub fn llamar_global(&mut self, nombre: &str, args: &[Valor]) -> Result<Valor, ErrorJs> {
        let clave = self.mundo.cadenas.internar(nombre);
        let f = self.mundo.leer(self.fija(R_GLOBAL), clave).unwrap_or(Valor::INDEFINIDO);
        self.llamar_valor(f, Valor::INDEFINIDO, args)
    }

    /// Llama a `f` desde fuera de la máquina: deja `f`, `this` y los argumentos en la pila,
    /// ejecuta hasta que vuelve y recoge el resultado.
    fn llamar_valor(&mut self, f: Valor, este: Valor, args: &[Valor]) -> Result<Valor, ErrorJs> {
        let base = self.mundo.m.abrir_raices(3 + args.len(), Valor::INDEFINIDO);
        let pila = self.mundo.m.pila_mut();
        pila[base] = f;
        pila[base + 1] = este;
        pila[base + 3..base + 3 + args.len()].copy_from_slice(args);
        let hasta = self.marcos.len();
        let destino = base + 2;
        let llamada =
            llamadas::Llamada { f: base, este: base + 1, args: base + 3, n: args.len(), destino, construir: false };
        let r = match self.llamar(llamada) {
            Ok(true) => self.correr(hasta),
            Ok(false) => Ok(()),
            Err(e) => Err(e),
        };
        let resultado = match r {
            Ok(()) => Ok(self.mundo.m.pila()[destino]),
            Err(e) => Err(ErrorJs { mensaje: self.texto(e), traza: self.traza_de(e) }),
        };
        self.marcos.truncate(hasta);
        self.mundo.m.soltar_raices(base);
        resultado
    }
}
