//! Cachés en línea al estilo **CacheIR** de SpiderMonkey: cada sitio (`o.x`, `o.x = v`) aprende
//! los casos que ve como *stubs*, y cada stub es una lista de **guardas** y una **acción**.
//!
//! - **Guardas**: la forma del receptor y la de cada prototipo de la cadena hasta el que tiene la
//!   propiedad. Si las formas son las de siempre, la propiedad está donde estaba: las formas dicen
//!   qué claves tiene cada objeto y en qué hueco.
//! - **Acciones**: leer un hueco (del receptor o de un prototipo: los métodos), la longitud de un
//!   arreglo, escribir un hueco, o **añadir** la propiedad con la transición de forma ya
//!   calculada (`this.x = …` en un constructor).
//! - También se aprende que una propiedad **no existe** en toda la cadena (`o.x === undefined`).
//! - Hasta [`POR_SITIO`] stubs por sitio (polimórfico): un sitio por el que pasan objetos de
//!   varias clases no se pisa la caché. Con los cuatro llenos el sitio es **megamórfico** y deja
//!   de aprender (como en SpiderMonkey): esos accesos van por el camino lento.
//!
//! Los stubs guardan solo **números de forma**, nunca punteros: el GC puede mover los objetos
//! sin invalidar nada, y la cadena de prototipos se recorre de verdad en cada uso. Las formas
//! de la cadena de cada stub van seguidas en una tabla compartida (`cadenas_ic`), así que la
//! profundidad no tiene límite y un stub corto no ocupa lo de uno largo. La caché de un solo caso propio (`Cache` en `lib.rs`) sigue siendo el primer
//! nivel, igual de rápido.

use crate::Motor;
use alloc::vec::Vec;
use quimera_objetos::{Clase, Valor};

/// Cuántos prototipos se miran como mucho (una cadena más larga se trata como un error del
/// programa y va por el camino lento).
const MAX_SALTOS: usize = 64;

/// Cuántos stubs aprende un sitio.
pub(crate) const POR_SITIO: usize = 4;

/// Guarda especial: «cualquier arreglo» (la longitud no depende de la forma).
const CUALQUIER_ARREGLO: u32 = u32::MAX - 1;

/// Lo que hace un stub cuando sus guardas pasan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Accion {
    /// Leer el hueco del último objeto de la cadena (el receptor, o el prototipo que la tiene).
    LeerHueco(u32),
    /// La longitud de un arreglo.
    Longitud,
    /// Escribir el hueco del receptor.
    EscribirHueco(u32),
    /// Añadir la propiedad al receptor: pasar a la forma `nueva` y escribir su `hueco`. `epoca`:
    /// cuántos accesores había definidos al aprenderlo (si no ha cambiado, no hay `set` nuevo
    /// en la cadena y las guardas de los prototipos sobran).
    Anadir { nueva: u32, hueco: u32, epoca: u32 },
    /// No está en ningún objeto de la cadena (que acaba después del último): `undefined`.
    Ausente,
}

/// Un caso aprendido: guardas de forma por la cadena de prototipos y una acción.
#[derive(Debug, Clone, Copy)]
struct Stub {
    /// La forma del receptor (la primera guarda, la que más falla: se mira sin salir del stub).
    forma: u32,
    /// Las formas de los prototipos: `saltos` números seguidos desde `cadena` en `cadenas_ic`.
    cadena: u32,
    saltos: u8,
    accion: Accion,
}

/// Los stubs de un sitio.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Sitio {
    stubs: [Option<Stub>; POR_SITIO],
}

impl Sitio {
    fn lleno(&self) -> bool {
        self.stubs.iter().all(Option::is_some)
    }

    fn anadir(&mut self, s: Stub) {
        if let Some(libre) = self.stubs.iter_mut().find(|x| x.is_none()) {
            *libre = Some(s);
        }
    }
}

impl Motor {
    /// El sitio de la instrucción `pc` de la función `fi`, creándolo si hace falta.
    fn sitio_mut(&mut self, fi: usize, pc: usize) -> Option<&mut Sitio> {
        let c = self.funciones.get_mut(fi)?.caches.get_mut(pc)?;
        if c.sitio == u32::MAX {
            c.sitio = self.sitios.len() as u32;
            self.sitios.push(Sitio::default());
        }
        let i = c.sitio as usize;
        self.sitios.get_mut(i)
    }

    fn sitio(&self, fi: usize, pc: usize) -> Option<&Sitio> {
        let i = self.funciones.get(fi)?.caches.get(pc)?.sitio;
        self.sitios.get(i as usize)
    }

    /// Si el sitio ya no aprende más (tiene sus stubs llenos).
    fn megamorfico(&self, fi: usize, pc: usize) -> bool {
        self.sitio(fi, pc).is_some_and(Sitio::lleno)
    }

    /// Añade un stub al sitio con las formas de los prototipos de su cadena.
    fn aprender(&mut self, (fi, pc): (usize, usize), forma: u32, protos: &[u32], accion: Accion) {
        // Nada que dependa de una forma diccionario: gana y pierde claves sin cambiar de número
        // (y la de un «añadir» que acaba en una es propia de un solo objeto).
        let f = &self.mundo.formas;
        let nueva = if let Accion::Anadir { nueva, .. } = accion { nueva } else { forma };
        if f.es_diccionario(forma) || f.es_diccionario(nueva) || protos.iter().any(|p| f.es_diccionario(*p)) {
            return;
        }
        let cadena = self.cadenas_ic.len() as u32;
        self.cadenas_ic.extend_from_slice(protos);
        let s = Stub { forma, cadena, saltos: protos.len() as u8, accion };
        if let Some(sitio) = self.sitio_mut(fi, pc) {
            sitio.anadir(s);
        }
    }

    /// Las guardas de un stub: si pasan, el último objeto de la cadena (el que tiene la
    /// propiedad, o el último mirado si no la tiene nadie).
    fn guardas(&self, obj: Valor, forma: u32, s: &Stub) -> Option<Valor> {
        if s.forma != forma {
            return None;
        }
        let mut actual = obj;
        let inicio = s.cadena as usize;
        for &f in self.cadenas_ic.get(inicio..inicio + s.saltos as usize)? {
            actual = self.mundo.prototipo(actual);
            if self.mundo.forma(actual) != Some(f) {
                return None;
            }
        }
        Some(actual)
    }

    /// Una lectura con los stubs del sitio (`None`: ninguno sirve).
    pub(crate) fn ic_leer(&self, obj: Valor, forma: u32, fi: usize, pc: usize) -> Option<Valor> {
        let sitio = self.sitio(fi, pc)?;
        for s in sitio.stubs.iter().map_while(|s| s.as_ref()) {
            match s.accion {
                Accion::Longitud if s.forma == CUALQUIER_ARREGLO => {
                    if self.mundo.clase(obj) == Some(Clase::Arreglo) {
                        return self.mundo.longitud(obj).map(|n| Valor::numero(f64::from(n)));
                    }
                }
                Accion::LeerHueco(h) => {
                    if let Some(dueno) = self.guardas(obj, forma, s) {
                        return Some(self.mundo.leer_hueco(dueno, h).unwrap_or(Valor::INDEFINIDO));
                    }
                }
                Accion::Ausente => {
                    let ultimo = self.guardas(obj, forma, s);
                    if ultimo.is_some_and(|u| !self.mundo.es_objeto(self.mundo.prototipo(u))) {
                        return Some(Valor::INDEFINIDO);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Aprende cómo leer `clave` de `obj` (en él o en su cadena de prototipos) y la lee.
    /// `None` si no se puede aprender (sitio megamórfico, `length` de un arreglo heredado,
    /// cadena demasiado larga): queda para el camino lento.
    pub(crate) fn ic_aprender_lectura(&mut self, obj: Valor, clave: u32, fi: usize, pc: usize) -> Option<Valor> {
        // `F.prototype` se crea al leerlo por primera vez: no se aprende.
        // `F.prototype` se crea al leerlo, y `name`/`length` de una función salen de su bytecode:
        // ninguno está en la forma, así que no se aprenden.
        let es_funcion = matches!(self.mundo.clase(obj), Some(Clase::Funcion | Clase::Nativa));
        let prototipo_de_funcion = es_funcion
            && (clave == self.mundo.clave_prototype
                || clave == self.mundo.clave_length
                || self.mundo.cadenas.texto(clave) == "name");
        if self.megamorfico(fi, pc) || prototipo_de_funcion {
            return None;
        }
        let es_length = |m: &Self, o: Valor| clave == m.mundo.clave_length && m.mundo.clase(o) == Some(Clase::Arreglo);
        if es_length(self, obj) {
            self.aprender((fi, pc), CUALQUIER_ARREGLO, &[], Accion::Longitud);
            return self.mundo.longitud(obj).map(|n| Valor::numero(f64::from(n)));
        }
        let forma = self.mundo.forma(obj)?;
        let mut protos: Vec<u32> = Vec::new();
        let mut actual = obj;
        let mut forma_actual = forma;
        loop {
            if let Some(h) = self.mundo.formas.buscar(forma_actual, clave) {
                self.aprender((fi, pc), forma, &protos, Accion::LeerHueco(h));
                return Some(self.mundo.leer_hueco(actual, h).unwrap_or(Valor::INDEFINIDO));
            }
            let siguiente = self.mundo.prototipo(actual);
            if !self.mundo.es_objeto(siguiente) {
                self.aprender((fi, pc), forma, &protos, Accion::Ausente);
                return Some(Valor::INDEFINIDO);
            }
            if protos.len() >= MAX_SALTOS || es_length(self, siguiente) {
                return None;
            }
            actual = siguiente;
            forma_actual = self.mundo.forma(actual)?;
            protos.push(forma_actual);
        }
    }

    /// Una escritura con los stubs del sitio. `None`: ninguno sirve; `Some(ok)`: hecha.
    pub(crate) fn ic_escribir(&mut self, obj: Valor, forma: u32, v: Valor, fi: usize, pc: usize) -> Option<bool> {
        let sitio = *self.sitio(fi, pc)?;
        for s in sitio.stubs.iter().map_while(|s| s.as_ref()).filter(|s| s.forma == forma) {
            match s.accion {
                Accion::EscribirHueco(h) => return Some(self.mundo.escribir_hueco(obj, h, v)),
                // Añadir mira antes la cadena de prototipos: un `set` nuevo en uno de ellos
                // cambia su forma y el stub deja de valer (sin vaciar las cachés de nadie).
                Accion::Anadir { nueva, hueco, epoca }
                    if epoca == self.epoca_accesores || self.guardas(obj, forma, s).is_some() =>
                {
                    return Some(self.mundo.anadir_propiedad(obj, nueva, hueco, v));
                }
                _ => {}
            }
        }
        None
    }

    /// Aprende cómo escribir `clave` en `obj` (un hueco que ya tiene, o añadirla) y la escribe.
    /// `None` si no es un caso que se pueda aprender (la longitud de un arreglo).
    pub(crate) fn ic_aprender_escritura(
        &mut self,
        obj: Valor,
        forma: u32,
        clave: u32,
        v: Valor,
        (fi, pc): (usize, usize),
    ) -> Option<bool> {
        if self.megamorfico(fi, pc)
            || (clave == self.mundo.clave_length && self.mundo.clase(obj) == Some(Clase::Arreglo))
        {
            return None;
        }
        let mut protos: Vec<u32> = Vec::new();
        let accion = match self.mundo.formas.buscar(forma, clave) {
            Some(h) => Accion::EscribirHueco(h),
            None => {
                // Las formas de la cadena de prototipos, para las guardas.
                let mut actual = self.mundo.prototipo(obj);
                while self.mundo.es_objeto(actual) {
                    if protos.len() >= MAX_SALTOS {
                        return None;
                    }
                    protos.push(self.mundo.forma(actual)?);
                    actual = self.mundo.prototipo(actual);
                }
                let nueva = self.mundo.formas.anadir(forma, clave);
                Accion::Anadir { nueva, hueco: self.mundo.formas.huecos(nueva) - 1, epoca: self.epoca_accesores }
            }
        };
        self.aprender((fi, pc), forma, &protos, accion);
        Some(match accion {
            Accion::EscribirHueco(h) => self.mundo.escribir_hueco(obj, h, v),
            Accion::Anadir { nueva, hueco, .. } => self.mundo.anadir_propiedad(obj, nueva, hueco, v),
            _ => false,
        })
    }

    /// Cuántos sitios tienen stubs y cuántos stubs hay en total (para medir).
    pub fn sitios_aprendidos(&self) -> (usize, usize) {
        let stubs = self.sitios.iter().map(|s| s.stubs.iter().flatten().count()).sum();
        (self.sitios.len(), stubs)
    }
}
