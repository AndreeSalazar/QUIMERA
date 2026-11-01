//! Propiedades con cachés en línea, índices de arreglos y variables globales.
//!
//! Dos niveles de caché por instrucción: el primero guarda una forma y su hueco (si el objeto
//! vuelve a tener esa forma, leer `o.x` es comparar un número y leer un hueco); el segundo, los
//! stubs al estilo CacheIR (`ic.rs`): varias formas, métodos del prototipo, `length` de los
//! arreglos y propiedades que se añaden. El perfil de Boa midió que la búsqueda de propiedades
//! se llevaba el 20-38 % del tiempo.

use crate::{Cache, Motor, R_GLOBAL, operaciones};
use alloc::format;
use quimera_objetos::{Cadenas, Clase, Tipo, Valor};

/// Sin caché (accesos con clave calculada).
pub(crate) const SIN_CACHE: usize = usize::MAX;

impl Motor {
    /// El id de cadena de un átomo del Stencil de la función `fi`.
    pub(crate) fn clave(&self, fi: usize, atomo: u32) -> u32 {
        self.funciones[fi].atomos.get(atomo as usize).copied().unwrap_or(0)
    }

    fn cache(&self, fi: usize, pc: usize) -> Cache {
        self.funciones.get(fi).and_then(|c| c.caches.get(pc)).copied().unwrap_or(crate::CACHE_VACIA)
    }

    /// Guarda el caso en el primer nivel si está libre. Devuelve `false` si ya tenía otro (el
    /// sitio es polimórfico: el caso va a los stubs).
    fn guardar_cache(&mut self, fi: usize, pc: usize, forma: u32, hueco: u32) -> bool {
        match self.funciones.get_mut(fi).and_then(|c| c.caches.get_mut(pc)) {
            Some(c) if c.forma == u32::MAX || c.forma == forma => {
                (c.forma, c.hueco) = (forma, hueco);
                true
            }
            _ => false,
        }
    }

    /// `obj.x` si acierta el primer nivel de la caché (sin buscar el nombre de la clave).
    #[inline]
    pub(crate) fn leer_cacheado(&self, obj: Valor, fi: usize, pc: usize) -> Option<Valor> {
        let c = self.cache(fi, pc);
        if c.forma == u32::MAX {
            return None;
        }
        self.mundo.leer_si_forma(obj, c.forma, c.hueco)
    }

    /// `obj.x = v` si acierta el primer nivel de la caché. `false`: hay que ir por `escribir_prop`.
    pub(crate) fn escribir_cacheado(&mut self, obj: Valor, v: Valor, fi: usize, pc: usize) -> bool {
        let c = self.cache(fi, pc);
        c.forma != u32::MAX
            && self.mundo.forma_de_objeto(obj) == Some(c.forma)
            && self.mundo.escribir_hueco(obj, c.hueco, v)
    }

    /// `obj.clave`.
    pub(crate) fn leer_prop(&mut self, obj: Valor, clave: u32, fi: usize, pc: usize) -> Result<Valor, Valor> {
        if let Some(dato) = self.mundo.anfitrion(obj) {
            if let Some(r) = self.leer_anfitrion(dato, clave) {
                return r;
            }
            return Ok(self.leer_lento(obj, clave));
        }
        if self.mundo.es_objeto(obj) {
            if let Some(forma) = self.mundo.forma(obj) {
                let c = self.cache(fi, pc);
                if c.forma == forma {
                    return Ok(self.mundo.leer_hueco(obj, c.hueco).unwrap_or(Valor::INDEFINIDO));
                }
                if pc != SIN_CACHE {
                    if let Some(v) = self.ic_leer(obj, forma, fi, pc) {
                        return Ok(v);
                    }
                    if let Some(hueco) = self.mundo.formas.buscar(forma, clave)
                        && self.guardar_cache(fi, pc, forma, hueco)
                    {
                        return Ok(self.mundo.leer_hueco(obj, hueco).unwrap_or(Valor::INDEFINIDO));
                    }
                    if let Some(v) = self.ic_aprender_lectura(obj, clave, fi, pc) {
                        return Ok(v);
                    }
                }
            }
            return Ok(self.leer_lento(obj, clave));
        }
        self.leer_primitivo(obj, clave)
    }

    /// Por la cadena de prototipos, y `F.prototype` creado al vuelo.
    fn leer_lento(&mut self, obj: Valor, clave: u32) -> Valor {
        if let Some(v) = self.mundo.leer(obj, clave) {
            return v;
        }
        let es_funcion = self.mundo.clase(obj) == Some(Clase::Funcion);
        if es_funcion && self.mundo.cadenas.texto(clave) == "prototype" {
            let marca = self.mundo.m.marca_raices();
            self.mundo.m.raiz(obj);
            let p = self.prototipo_de_funcion(marca);
            self.mundo.m.soltar_raices(marca);
            return p;
        }
        Valor::INDEFINIDO
    }

    fn leer_primitivo(&mut self, obj: Valor, clave: u32) -> Result<Valor, Valor> {
        match obj.tipo() {
            Tipo::Indefinido | Tipo::Nulo => {
                let (que, nombre) = (self.texto(obj), self.mundo.cadenas.texto(clave));
                let m = format!("no se puede leer la propiedad '{nombre}' de {que}");
                Err(self.error("TypeError", &m))
            }
            Tipo::Cadena if clave == self.mundo.clave_length => {
                let n = self.mundo.texto(obj).map_or(0, |s| s.encode_utf16().count());
                Ok(Valor::entero(n as i32))
            }
            // Los métodos de un primitivo están en su prototipo (`(1.5).toFixed`, `"a".charAt`).
            Tipo::Cadena => Ok(self.mundo.leer(self.pila(crate::R_CADENA), clave).unwrap_or(Valor::INDEFINIDO)),
            Tipo::Numero | Tipo::Entero => {
                Ok(self.mundo.leer(self.pila(crate::R_NUMERO), clave).unwrap_or(Valor::INDEFINIDO))
            }
            _ => Ok(Valor::INDEFINIDO),
        }
    }

    /// `obj.clave = v`.
    pub(crate) fn escribir_prop(
        &mut self,
        obj: Valor,
        clave: u32,
        v: Valor,
        fi: usize,
        pc: usize,
    ) -> Result<(), Valor> {
        if !self.mundo.es_objeto(obj) {
            if matches!(obj.tipo(), Tipo::Indefinido | Tipo::Nulo) {
                let (que, nombre) = (self.texto(obj), self.mundo.cadenas.texto(clave));
                let m = format!("no se puede escribir la propiedad '{nombre}' en {que}");
                return Err(self.error("TypeError", &m));
            }
            return Ok(()); // en un primitivo se ignora (modo no estricto)
        }
        if let Some(dato) = self.mundo.anfitrion(obj) {
            match self.escribir_anfitrion(dato, clave, v) {
                Some(r) => return r,
                None if self.mundo.escribir(obj, clave, v) => return Ok(()),
                None => {}
            }
        }
        if let Some(forma) = self.mundo.forma(obj) {
            let c = self.cache(fi, pc);
            if c.forma == forma && self.mundo.escribir_hueco(obj, c.hueco, v) {
                return Ok(());
            }
            if pc != SIN_CACHE {
                if let Some(true) = self.ic_escribir(obj, forma, v, fi, pc) {
                    return Ok(());
                }
                if let Some(hueco) = self.mundo.formas.buscar(forma, clave)
                    && self.guardar_cache(fi, pc, forma, hueco)
                    && self.mundo.escribir_hueco(obj, hueco, v)
                {
                    return Ok(());
                }
                if let Some(true) = self.ic_aprender_escritura(obj, forma, clave, v, (fi, pc)) {
                    return Ok(());
                }
            }
        }
        if self.mundo.escribir(obj, clave, v) {
            return Ok(());
        }
        let nombre = self.mundo.cadenas.texto(clave);
        let m = format!("no se puede escribir la propiedad '{nombre}'");
        Err(self.error("TypeError", &m))
    }

    /// La clave de propiedad de un valor (`o[k]`) para **leer**: su número de cadena. Un texto
    /// que no es cadena todavía (`o[5]`) solo se busca: si no existe, ninguna forma lo tiene.
    fn clave_de(&mut self, k: Valor) -> u32 {
        match Cadenas::id_de_valor(k) {
            Some(id) => id,
            None => {
                let s = operaciones::a_texto(&self.mundo, k);
                self.mundo.cadenas.buscar(&s).unwrap_or(u32::MAX)
            }
        }
    }

    /// La clave para **escribir** `o[k] = v`: un átomo, porque la forma guarda su número.
    fn clave_para_escribir(&mut self, k: Valor) -> u32 {
        match self.mundo.cadenas.clave_para_escribir(k) {
            Some(id) => id,
            None => {
                let s = operaciones::a_texto(&self.mundo, k);
                self.mundo.cadenas.internar(&s)
            }
        }
    }

    /// Índice de arreglo: un entero (o número entero) entre 0 y 2^32 - 2.
    pub(crate) fn indice(k: Valor) -> Option<u32> {
        if let Some(i) = k.como_entero() {
            return u32::try_from(i).ok();
        }
        let n = k.como_numero()?;
        ((0.0..4294967295.0).contains(&n) && (n as u32) as f64 == n).then_some(n as u32)
    }

    /// `obj[k]`.
    pub(crate) fn leer_indice(&mut self, obj: Valor, k: Valor) -> Result<Valor, Valor> {
        if let Some(i) = Self::indice(k) {
            if self.mundo.clase(obj) == Some(Clase::Arreglo) {
                return Ok(self.mundo.leer_elemento(obj, i).unwrap_or(Valor::INDEFINIDO));
            }
            if let Some(s) = self.mundo.texto(obj) {
                let caracter = s.chars().nth(i as usize);
                return Ok(match caracter {
                    Some(c) => self.mundo.cadena(c.encode_utf8(&mut [0; 4])),
                    None => Valor::INDEFINIDO,
                });
            }
        }
        let clave = self.clave_de(k);
        self.leer_prop(obj, clave, 0, crate::propiedades::SIN_CACHE)
    }

    /// `obj[k] = v`.
    pub(crate) fn escribir_indice(&mut self, obj: Valor, k: Valor, v: Valor) -> Result<(), Valor> {
        if let (Some(i), Some(Clase::Arreglo)) = (Self::indice(k), self.mundo.clase(obj)) {
            if self.mundo.escribir_elemento(obj, i, v) {
                return Ok(());
            }
            return Err(self.error("RangeError", "los arreglos dispersos todavía no están en QUIMERA"));
        }
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(obj);
        self.mundo.m.raiz(v);
        let clave = self.clave_para_escribir(k);
        let (obj, v) = (self.pila(marca), self.pila(marca + 1));
        self.mundo.m.soltar_raices(marca);
        self.escribir_prop(obj, clave, v, 0, SIN_CACHE)
    }

    /// Una variable global (con caché, como una propiedad del objeto global).
    pub(crate) fn leer_global(&mut self, clave: u32, fi: usize, pc: usize) -> Result<Valor, Valor> {
        let global = self.pila(R_GLOBAL);
        let c = self.cache(fi, pc);
        if c.forma != u32::MAX
            && let Some(v) = self.mundo.leer_si_forma(global, c.forma, c.hueco)
        {
            return Ok(v);
        }
        if let Some(forma) = self.mundo.forma(global) {
            let c = self.cache(fi, pc);
            if c.forma == forma {
                return Ok(self.mundo.leer_hueco(global, c.hueco).unwrap_or(Valor::INDEFINIDO));
            }
            if let Some(hueco) = self.mundo.formas.buscar(forma, clave) {
                self.guardar_cache(fi, pc, forma, hueco);
                return Ok(self.mundo.leer_hueco(global, hueco).unwrap_or(Valor::INDEFINIDO));
            }
        }
        match self.mundo.leer(global, clave) {
            Some(v) => Ok(v),
            None => {
                let m = format!("{} no está definida", self.mundo.cadenas.texto(clave));
                Err(self.error("ReferenceError", &m))
            }
        }
    }

    /// `typeof nombre` de una global que quizás no existe.
    pub(crate) fn tipo_global(&mut self, clave: u32) -> Valor {
        let v = self.mundo.leer(self.pila(R_GLOBAL), clave).unwrap_or(Valor::INDEFINIDO);
        let t = operaciones::tipo_de(&self.mundo, v);
        self.mundo.cadena(t)
    }

    /// `k in obj`.
    pub(crate) fn contiene(&mut self, k: Valor, obj: Valor) -> Result<bool, Valor> {
        if !self.mundo.es_objeto(obj) {
            return Err(self.error("TypeError", "'in' necesita un objeto a la derecha"));
        }
        if let (Some(i), Some(Clase::Arreglo)) = (Self::indice(k), self.mundo.clase(obj)) {
            return Ok(i < self.mundo.longitud(obj).unwrap_or(0));
        }
        let clave = self.clave_de(k);
        Ok(self.mundo.leer(obj, clave).is_some())
    }

    /// `a instanceof F`: ¿está `F.prototype` en la cadena de prototipos de `a`?
    pub(crate) fn instancia(&mut self, a: Valor, f_abs: usize) -> Result<bool, Valor> {
        if !matches!(self.clase_en(f_abs), Some(Clase::Funcion | Clase::Nativa)) {
            return Err(self.error("TypeError", "instanceof necesita una función a la derecha"));
        }
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(a);
        let proto = self.prototipo_de_funcion(f_abs);
        let mut actual = self.pila(marca);
        self.mundo.m.soltar_raices(marca);
        for _ in 0..10_000 {
            if !self.mundo.es_objeto(actual) {
                return Ok(false);
            }
            actual = self.mundo.prototipo(actual);
            if actual == proto {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
