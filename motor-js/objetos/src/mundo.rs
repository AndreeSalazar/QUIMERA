//! El mundo de objetos: el montón del GC, las formas y las cadenas, y las operaciones sobre ellos.

use crate::cadenas::Cadenas;
use crate::formas::{FORMA_VACIA, Formas, IdForma};
use quimera_gc::{Config, Monton};
use quimera_valor::{Tipo, Valor};

const C_TIPO: usize = 0;
const C_FORMA: usize = 1;
const C_PROTO: usize = 2;
const C_PROPS: usize = 3;
const C_EXTRA: usize = 4; // elementos, índice de función o id de nativa
const C_EXTRA2: usize = 5; // longitud o entorno
const C_ESTE: usize = 6;
const E_PADRE: usize = 1;
const E_HUECOS: usize = 2;

/// Hasta cuántos elementos más allá del final se acepta escribir en un arreglo (lo demás sería disperso).
const MAX_SALTO_ARREGLO: u32 = 1 << 20;

/// La clase de una celda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clase {
    /// Objeto normal.
    Objeto = 1,
    /// Arreglo con elementos densos.
    Arreglo = 2,
    /// Función de JavaScript (un cierre).
    Funcion = 3,
    /// Función hecha en Rust.
    Nativa = 4,
    /// Entorno de variables capturadas.
    Entorno = 5,
    /// Expresión regular: un objeto con el número de su programa compilado (en la máquina).
    RegExp = 6,
}

impl Clase {
    fn desde(n: i32) -> Option<Clase> {
        Some(match n {
            1 => Clase::Objeto,
            2 => Clase::Arreglo,
            3 => Clase::Funcion,
            4 => Clase::Nativa,
            5 => Clase::Entorno,
            6 => Clase::RegExp,
            _ => return None,
        })
    }
}

/// Montón, formas y cadenas juntos.
pub struct Mundo {
    /// El montón del GC (su pila de raíces son los registros de la máquina virtual).
    pub m: Monton,
    /// Las formas de los objetos.
    pub formas: Formas,
    /// Las cadenas internadas.
    pub cadenas: Cadenas,
    /// Id de la cadena `"length"`.
    pub clave_length: u32,
}

impl Default for Mundo {
    fn default() -> Self {
        Self::nuevo(Config::default())
    }
}

fn entero(v: Option<Valor>) -> Option<i32> {
    v?.como_entero()
}

impl Mundo {
    /// Un mundo vacío.
    pub fn nuevo(config: Config) -> Mundo {
        let mut cadenas = Cadenas::default();
        let clave_length = cadenas.internar("length");
        Mundo { m: Monton::nuevo(config), formas: Formas::default(), cadenas, clave_length }
    }

    /// La clase de un valor, si es una celda del montón.
    pub fn clase(&self, v: Valor) -> Option<Clase> {
        v.como_referencia()?;
        Clase::desde(entero(self.m.leer(v, C_TIPO))?)
    }

    /// Es un objeto (de cualquier clase menos entorno): puede tener propiedades.
    pub fn es_objeto(&self, v: Valor) -> bool {
        matches!(self.clase(v), Some(c) if c != Clase::Entorno)
    }

    /// Mete valores en la pila de raíces; devuelve la marca para leerlos y soltarlos.
    fn proteger(&mut self, vals: &[Valor]) -> usize {
        let base = self.m.marca_raices();
        for v in vals {
            self.m.raiz(*v);
        }
        base
    }

    fn recuperar(&self, base: usize, i: usize) -> Valor {
        self.m.pila().get(base + i).copied().unwrap_or(Valor::INDEFINIDO)
    }

    // ---------------- creación ----------------

    /// `{}` con este prototipo (`null` para ninguno).
    pub fn crear_objeto(&mut self, proto: Valor) -> Valor {
        let forma = Valor::entero(FORMA_VACIA as i32);
        self.m.asignar(&[Valor::entero(Clase::Objeto as i32), forma, proto, Valor::NULO])
    }

    /// Un cierre de la función `indice`, con su entorno y su `this` léxico.
    pub fn crear_funcion(&mut self, indice: u32, entorno: Valor, este: Valor, proto: Valor) -> Valor {
        let tipo = Valor::entero(Clase::Funcion as i32);
        let forma = Valor::entero(FORMA_VACIA as i32);
        self.m.asignar(&[tipo, forma, proto, Valor::NULO, Valor::entero(indice as i32), entorno, este])
    }

    /// Una función nativa con este número.
    pub fn crear_nativa(&mut self, id: u32, proto: Valor) -> Valor {
        let tipo = Valor::entero(Clase::Nativa as i32);
        let forma = Valor::entero(FORMA_VACIA as i32);
        self.m.asignar(&[tipo, forma, proto, Valor::NULO, Valor::entero(id as i32)])
    }

    /// Una expresión regular: un objeto que guarda el número `indice` de su programa compilado.
    pub fn crear_regexp(&mut self, indice: u32, proto: Valor) -> Valor {
        let tipo = Valor::entero(Clase::RegExp as i32);
        let forma = Valor::entero(FORMA_VACIA as i32);
        self.m.asignar(&[tipo, forma, proto, Valor::NULO, Valor::entero(indice as i32)])
    }

    /// El número del programa de una expresión regular.
    pub fn regexp(&self, v: Valor) -> Option<u32> {
        if self.clase(v)? != Clase::RegExp {
            return None;
        }
        Some(entero(self.m.leer(v, C_EXTRA))? as u32)
    }

    /// Un arreglo con estos elementos.
    pub fn crear_arreglo(&mut self, elementos: &[Valor], proto: Valor) -> Valor {
        let base = self.proteger(&[proto]);
        let celda = self.m.asignar(elementos);
        let proto = self.recuperar(base, 0);
        self.m.soltar_raices(base);
        let tipo = Valor::entero(Clase::Arreglo as i32);
        let forma = Valor::entero(FORMA_VACIA as i32);
        let largo = Valor::entero(elementos.len() as i32);
        self.m.asignar(&[tipo, forma, proto, Valor::NULO, celda, largo])
    }

    /// Un entorno con `huecos` variables a `undefined`, hijo de `padre`.
    pub fn crear_entorno(&mut self, padre: Valor, huecos: u16) -> Valor {
        let n = E_HUECOS + huecos as usize;
        let tipo = Valor::entero(Clase::Entorno as i32);
        // Hasta 14 huecos, sin pedir memoria para preparar los campos.
        let mut fijo = [Valor::INDEFINIDO; 16];
        if n <= fijo.len() {
            fijo[C_TIPO] = tipo;
            fijo[E_PADRE] = padre;
            return self.m.asignar(&fijo[..n]);
        }
        let mut v = alloc::vec![Valor::INDEFINIDO; n];
        v[C_TIPO] = tipo;
        v[E_PADRE] = padre;
        self.m.asignar(&v)
    }

    // ---------------- entornos ----------------

    fn entorno_a(&self, mut env: Valor, saltos: u16) -> Option<Valor> {
        for _ in 0..saltos {
            env = self.m.leer(env, E_PADRE)?;
        }
        Some(env)
    }

    /// Lee el hueco `hueco` del entorno que está `saltos` niveles por encima de `env`.
    pub fn leer_entorno(&self, env: Valor, saltos: u16, hueco: u16) -> Option<Valor> {
        self.m.leer(self.entorno_a(env, saltos)?, E_HUECOS + hueco as usize)
    }

    /// Escribe el hueco `hueco` del entorno que está `saltos` niveles por encima de `env`.
    pub fn escribir_entorno(&mut self, env: Valor, saltos: u16, hueco: u16, v: Valor) -> bool {
        match self.entorno_a(env, saltos) {
            Some(e) => self.m.escribir(e, E_HUECOS + hueco as usize, v),
            None => false,
        }
    }

    // ---------------- funciones ----------------

    /// Índice de función, entorno y `this` léxico de un cierre.
    pub fn funcion(&self, f: Valor) -> Option<(u32, Valor, Valor)> {
        if self.clase(f)? != Clase::Funcion {
            return None;
        }
        let indice = entero(self.m.leer(f, C_EXTRA))? as u32;
        Some((indice, self.m.leer(f, C_EXTRA2)?, self.m.leer(f, C_ESTE)?))
    }

    /// Número de una función nativa.
    pub fn nativa(&self, f: Valor) -> Option<u32> {
        if self.clase(f)? != Clase::Nativa {
            return None;
        }
        Some(entero(self.m.leer(f, C_EXTRA))? as u32)
    }

    // ---------------- propiedades ----------------

    /// El prototipo de un objeto (`null` si no tiene).
    pub fn prototipo(&self, obj: Valor) -> Valor {
        self.m.leer(obj, C_PROTO).unwrap_or(Valor::NULO)
    }

    /// La forma de un objeto.
    pub fn forma(&self, obj: Valor) -> Option<IdForma> {
        Some(entero(self.m.leer(obj, C_FORMA))? as IdForma)
    }

    /// El valor del hueco de propiedad `hueco` (para las cachés en línea).
    pub fn leer_hueco(&self, obj: Valor, hueco: u32) -> Option<Valor> {
        self.m.leer(self.m.leer(obj, C_PROPS)?, hueco as usize)
    }

    /// Escribe el hueco de propiedad `hueco` (que ya existe).
    pub fn escribir_hueco(&mut self, obj: Valor, hueco: u32, v: Valor) -> bool {
        match self.m.leer(obj, C_PROPS) {
            Some(props) => self.m.escribir(props, hueco as usize, v),
            None => false,
        }
    }

    /// Una propiedad propia (sin mirar el prototipo).
    pub fn leer_propia(&self, obj: Valor, clave: u32) -> Option<Valor> {
        if clave == self.clave_length && self.clase(obj) == Some(Clase::Arreglo) {
            return self.longitud(obj).map(|n| Valor::numero(n as f64));
        }
        let hueco = self.formas.buscar(self.forma(obj)?, clave)?;
        self.leer_hueco(obj, hueco)
    }

    /// Una propiedad, buscando en la cadena de prototipos. `None` si no existe en ninguno.
    pub fn leer(&self, obj: Valor, clave: u32) -> Option<Valor> {
        let mut actual = obj;
        for _ in 0..10_000 {
            if let Some(v) = self.leer_propia(actual, clave) {
                return Some(v);
            }
            actual = self.prototipo(actual);
            if !self.es_objeto(actual) {
                return None;
            }
        }
        None
    }

    /// Crea o cambia una propiedad propia. Puede asignar memoria (y mover objetos).
    pub fn escribir(&mut self, obj: Valor, clave: u32, v: Valor) -> bool {
        if clave == self.clave_length && self.clase(obj) == Some(Clase::Arreglo) {
            let n = v.como_numero().filter(|n| *n >= 0.0 && *n <= u32::MAX as f64).map(|n| n as u32);
            return n.is_some_and(|n| self.cambiar_longitud(obj, n));
        }
        let Some(forma) = self.forma(obj) else { return false };
        if let Some(hueco) = self.formas.buscar(forma, clave) {
            return self.escribir_hueco(obj, hueco, v);
        }
        let nueva = self.formas.anadir(forma, clave);
        let hueco = self.formas.huecos(nueva) - 1;
        let base = self.proteger(&[obj, v]);
        let ok = self.asegurar_huecos(base, hueco + 1);
        let (obj, v) = (self.recuperar(base, 0), self.recuperar(base, 1));
        self.m.soltar_raices(base);
        ok && self.m.escribir(obj, C_FORMA, Valor::entero(nueva as i32)) && self.escribir_hueco(obj, hueco, v)
    }

    /// Que la celda de propiedades del objeto protegido en `base` tenga al menos `n` huecos.
    fn asegurar_huecos(&mut self, base: usize, n: u32) -> bool {
        let obj = self.recuperar(base, 0);
        let props = self.m.leer(obj, C_PROPS).unwrap_or(Valor::NULO);
        let capacidad = self.m.largo(props).unwrap_or(0) as u32;
        if n <= capacidad {
            return true;
        }
        let nueva_cap = (capacidad * 2).max(4).max(n) as usize;
        let mut campos = alloc::vec![Valor::INDEFINIDO; nueva_cap];
        for (i, c) in campos.iter_mut().enumerate().take(capacidad as usize) {
            *c = self.m.leer(props, i).unwrap_or(Valor::INDEFINIDO);
        }
        let nueva = self.m.asignar(&campos);
        let obj = self.recuperar(base, 0);
        self.m.escribir(obj, C_PROPS, nueva)
    }

    /// `delete obj[clave]` de una propiedad propia. La forma se rehace sin la clave y los
    /// huecos de detrás se corren uno a la izquierda, en su sitio: no asigna memoria.
    pub fn borrar(&mut self, obj: Valor, clave: u32) -> bool {
        let Some(forma) = self.forma(obj) else { return true };
        let Some(hueco) = self.formas.buscar(forma, clave) else { return true };
        let claves: alloc::vec::Vec<u32> = self.formas.claves(forma).to_vec();
        let mut nueva = FORMA_VACIA;
        for &k in claves.iter().filter(|&&k| k != clave) {
            nueva = self.formas.anadir(nueva, k);
        }
        let Some(props) = self.m.leer(obj, C_PROPS) else { return false };
        for i in hueco as usize..claves.len() - 1 {
            let siguiente = self.m.leer(props, i + 1).unwrap_or(Valor::INDEFINIDO);
            self.m.escribir(props, i, siguiente);
        }
        self.m.escribir(props, claves.len() - 1, Valor::INDEFINIDO);
        self.m.escribir(obj, C_FORMA, Valor::entero(nueva as i32))
    }

    /// Las claves propias en orden de creación.
    pub fn claves(&self, obj: Valor) -> &[u32] {
        self.forma(obj).map_or(&[], |f| self.formas.claves(f))
    }

    // ---------------- arreglos ----------------

    /// La longitud de un arreglo.
    pub fn longitud(&self, arr: Valor) -> Option<u32> {
        Some(entero(self.m.leer(arr, C_EXTRA2))? as u32)
    }

    /// El elemento `i` (`undefined` si está fuera o es un hueco).
    pub fn leer_elemento(&self, arr: Valor, i: u32) -> Option<Valor> {
        if i >= self.longitud(arr)? {
            return Some(Valor::INDEFINIDO);
        }
        self.m.leer(self.m.leer(arr, C_EXTRA)?, i as usize)
    }

    /// Escribe el elemento `i`, creciendo si hace falta. Puede asignar memoria (y mover objetos).
    pub fn escribir_elemento(&mut self, arr: Valor, i: u32, v: Valor) -> bool {
        let Some(largo) = self.longitud(arr) else { return false };
        if i > largo.saturating_add(MAX_SALTO_ARREGLO) {
            return false;
        }
        let base = self.proteger(&[arr, v]);
        let ok = self.asegurar_elementos(base, i + 1);
        let (arr, v) = (self.recuperar(base, 0), self.recuperar(base, 1));
        self.m.soltar_raices(base);
        let Some(celda) = self.m.leer(arr, C_EXTRA) else { return false };
        if !(ok && self.m.escribir(celda, i as usize, v)) {
            return false;
        }
        i < largo || self.m.escribir(arr, C_EXTRA2, Valor::entero(i as i32 + 1))
    }

    /// Añade al final (`push`) y devuelve la nueva longitud.
    pub fn empujar(&mut self, arr: Valor, v: Valor) -> Option<u32> {
        let n = self.longitud(arr)?;
        self.escribir_elemento(arr, n, v).then_some(n + 1)
    }

    /// Quita el último (`pop`).
    pub fn sacar(&mut self, arr: Valor) -> Option<Valor> {
        let n = self.longitud(arr)?;
        if n == 0 {
            return Some(Valor::INDEFINIDO);
        }
        let v = self.leer_elemento(arr, n - 1)?;
        self.cambiar_longitud(arr, n - 1).then_some(v)
    }

    fn cambiar_longitud(&mut self, arr: Valor, n: u32) -> bool {
        let Some(largo) = self.longitud(arr) else { return false };
        if n > largo {
            return self.escribir_elemento(arr, n - 1, Valor::INDEFINIDO);
        }
        if let Some(celda) = self.m.leer(arr, C_EXTRA) {
            for i in n..largo {
                self.m.escribir(celda, i as usize, Valor::INDEFINIDO);
            }
        }
        self.m.escribir(arr, C_EXTRA2, Valor::entero(n as i32))
    }

    /// Que la celda de elementos del arreglo protegido en `base` tenga al menos `n` huecos.
    fn asegurar_elementos(&mut self, base: usize, n: u32) -> bool {
        let arr = self.recuperar(base, 0);
        let celda = self.m.leer(arr, C_EXTRA).unwrap_or(Valor::NULO);
        let capacidad = self.m.largo(celda).unwrap_or(0) as u32;
        if n <= capacidad {
            return true;
        }
        let nueva_cap = (capacidad * 2).max(8).max(n) as usize;
        let mut campos = alloc::vec![Valor::INDEFINIDO; nueva_cap];
        for (i, c) in campos.iter_mut().enumerate().take(capacidad as usize) {
            *c = self.m.leer(celda, i).unwrap_or(Valor::INDEFINIDO);
        }
        let nueva = self.m.asignar(&campos);
        let arr = self.recuperar(base, 0);
        self.m.escribir(arr, C_EXTRA, nueva)
    }

    /// Una cadena como valor.
    pub fn cadena(&mut self, texto: &str) -> Valor {
        self.cadenas.valor(texto)
    }

    /// El texto de un valor de cadena.
    pub fn texto(&self, v: Valor) -> Option<&str> {
        match v.tipo() {
            Tipo::Cadena => self.cadenas.de_valor(v),
            _ => None,
        }
    }
}
