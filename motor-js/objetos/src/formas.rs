//! Formas (*shapes*, *hidden classes*): qué propiedades tiene un objeto y en qué hueco está cada una.
//!
//! Dos objetos creados igual (`{x: 1, y: 2}`) comparten forma, así que leer `o.x` se reduce a
//! "¿es la forma de siempre? entonces el hueco 0". Es la base de las cachés en línea de
//! SpiderMonkey y V8. Las formas viven fuera del GC y no se liberan (como en un motor real,
//! son pocas comparadas con los objetos).
//!
//! **Modo diccionario** (como en V8): cada forma nueva copia las claves de la anterior, así que
//! un objeto de N claves dejaría N formas con N²/2 claves en total. Los diccionarios de
//! traducciones de Steam (miles de claves) se comían 1,7 GB y segundos de CPU. Pasadas
//! [`LIMITE_DICCIONARIO`] claves, el objeto recibe una forma propia que crece en su sitio, sin
//! copias ni transiciones. Las cachés en línea no guardan nada de esas formas (pueden ganar o
//! perder claves sin cambiar de número), así que acceder a esos objetos va por el camino lento.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Identificador de una forma.
pub type IdForma = u32;

/// La forma vacía, con la que nace todo objeto.
pub const FORMA_VACIA: IdForma = 0;

/// Claves a partir de las que un objeto pasa a modo diccionario. Alto para que los prototipos
/// normales (`jQuery.fn` tiene unas 200) sigan compartiendo formas y cachés.
pub const LIMITE_DICCIONARIO: usize = 1024;

/// Una lista de claves en orden de creación, con su índice clave → hueco. La comparten todas las
/// formas de una cadena `{a} → {a, b} → {a, b, c}`: cada una usa sus primeras `n` claves.
#[derive(Debug, Clone, Default)]
struct Tabla {
    /// Claves en orden de creación (el orden de `for-in` y de `Object.keys`).
    orden: Vec<u32>,
    /// Clave (id de cadena) → hueco (su posición en `orden`).
    huecos: BTreeMap<u32, u32>,
}

#[derive(Debug, Clone, Default)]
struct Forma {
    /// Su tabla de claves (compartida) y cuántas de sus claves son suyas.
    tabla: u32,
    n: u32,
    /// Clave añadida → forma siguiente.
    transiciones: BTreeMap<u32, IdForma>,
    /// Forma propia de un objeto en modo diccionario: crece y encoge en su sitio (con su tabla
    /// propia).
    diccionario: bool,
}

/// La tabla de formas.
///
/// **Tablas compartidas** (como las *descriptor arrays* de V8): antes cada forma nueva copiaba
/// todas las claves de la anterior, así que una cadena de N claves guardaba N²/2. En Steam eran
/// 33.000 formas con 8,9 millones de claves (unos 330 MB). Ahora una forma que crece desde la
/// última de su tabla añade la clave a esa misma tabla; solo se copia al ramificarse (añadir otra
/// clave a una forma que ya no es la última de su tabla), y solo sus primeras claves.
#[derive(Debug, Clone)]
pub struct Formas {
    formas: Vec<Forma>,
    tablas: Vec<Tabla>,
}

impl Default for Formas {
    fn default() -> Self {
        Formas { formas: alloc::vec![Forma::default()], tablas: alloc::vec![Tabla::default()] }
    }
}

impl Formas {
    /// El hueco de `clave` en `forma`, si la tiene.
    pub fn buscar(&self, forma: IdForma, clave: u32) -> Option<u32> {
        let f = self.formas.get(forma as usize)?;
        self.tablas.get(f.tabla as usize)?.huecos.get(&clave).copied().filter(|h| *h < f.n)
    }

    /// Cuántos huecos usa la forma.
    pub fn huecos(&self, forma: IdForma) -> u32 {
        self.formas.get(forma as usize).map_or(0, |f| f.n)
    }

    /// Las claves en orden de creación.
    pub fn claves(&self, forma: IdForma) -> &[u32] {
        let Some(f) = self.formas.get(forma as usize) else { return &[] };
        self.tablas.get(f.tabla as usize).map_or(&[], |t| &t.orden[..f.n as usize])
    }

    /// Una tabla nueva con las primeras `n` claves de `tabla` y `clave` detrás. Devuelve su número.
    fn copiar_tabla(&mut self, tabla: u32, n: u32, clave: u32) -> u32 {
        let mut orden: Vec<u32> = self.tablas.get(tabla as usize).map_or(&[][..], |t| &t.orden[..n as usize]).to_vec();
        orden.push(clave);
        let huecos = orden.iter().enumerate().map(|(i, k)| (*k, i as u32)).collect();
        self.tablas.push(Tabla { orden, huecos });
        (self.tablas.len() - 1) as u32
    }

    /// La forma que resulta de añadir `clave`. La misma secuencia de claves da siempre la misma forma.
    pub fn anadir(&mut self, forma: IdForma, clave: u32) -> IdForma {
        let Some(f) = self.formas.get(forma as usize) else { return forma };
        let (tabla, n, diccionario) = (f.tabla, f.n, f.diccionario);
        if diccionario {
            // Su tabla es solo suya: crece en su sitio.
            if self.buscar(forma, clave).is_none()
                && let Some(t) = self.tablas.get_mut(tabla as usize)
            {
                t.huecos.insert(clave, n);
                t.orden.push(clave);
                if let Some(f) = self.formas.get_mut(forma as usize) {
                    f.n += 1;
                }
            }
            return forma;
        }
        if let Some(&siguiente) = f.transiciones.get(&clave) {
            return siguiente;
        }
        if n as usize >= LIMITE_DICCIONARIO {
            // La forma propia de este objeto: tabla propia y sin transición desde la anterior.
            let t = self.copiar_tabla(tabla, n, clave);
            self.formas.push(Forma { tabla: t, n: n + 1, transiciones: BTreeMap::new(), diccionario: true });
            return (self.formas.len() - 1) as IdForma;
        }
        // Si es la última forma de su tabla, la nueva comparte la tabla; si no, se ramifica.
        let es_la_ultima = self.tablas.get(tabla as usize).is_some_and(|t| t.orden.len() == n as usize);
        let tabla = if es_la_ultima {
            if let Some(t) = self.tablas.get_mut(tabla as usize) {
                t.huecos.insert(clave, n);
                t.orden.push(clave);
            }
            tabla
        } else {
            self.copiar_tabla(tabla, n, clave)
        };
        let id = self.formas.len() as IdForma;
        self.formas.push(Forma { tabla, n: n + 1, transiciones: BTreeMap::new(), diccionario: false });
        if let Some(f) = self.formas.get_mut(forma as usize) {
            f.transiciones.insert(clave, id);
        }
        id
    }

    /// Si `forma` es la forma propia de un objeto en modo diccionario.
    #[inline]
    pub fn es_diccionario(&self, forma: IdForma) -> bool {
        self.formas.get(forma as usize).is_some_and(|f| f.diccionario)
    }

    /// Quita `clave` de una forma diccionario, en su sitio (los huecos de detrás se corren uno).
    pub fn quitar(&mut self, forma: IdForma, clave: u32) {
        let Some(f) = self.formas.get_mut(forma as usize).filter(|f| f.diccionario) else { return };
        let Some(t) = self.tablas.get_mut(f.tabla as usize) else { return };
        let Some(hueco) = t.huecos.remove(&clave) else { return };
        t.orden.remove(hueco as usize);
        for (i, k) in t.orden.iter().enumerate().skip(hueco as usize) {
            t.huecos.insert(*k, i as u32);
        }
        f.n -= 1;
    }

    /// Cambia `clave` por `nueva` en una forma diccionario, en el mismo hueco.
    pub fn renombrar(&mut self, forma: IdForma, clave: u32, nueva: u32) {
        let Some(f) = self.formas.get(forma as usize).filter(|f| f.diccionario) else { return };
        let Some(t) = self.tablas.get_mut(f.tabla as usize) else { return };
        let Some(hueco) = t.huecos.remove(&clave) else { return };
        t.huecos.insert(nueva, hueco);
        t.orden[hueco as usize] = nueva;
    }

    /// Cuántas formas existen.
    pub fn total(&self) -> usize {
        self.formas.len()
    }

    /// Cuántas claves guardan todas las tablas juntas (para medir la memoria de las formas).
    pub fn claves_guardadas(&self) -> usize {
        self.tablas.iter().map(|t| t.orden.len()).sum()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_mismo_orden_da_la_misma_forma() {
        let mut f = Formas::default();
        let a = f.anadir(FORMA_VACIA, 10);
        let ab = f.anadir(a, 20);
        let otra_a = f.anadir(FORMA_VACIA, 10);
        assert_eq!(f.anadir(otra_a, 20), ab, "transición reutilizada");
        let b = f.anadir(FORMA_VACIA, 20);
        let ba = f.anadir(b, 10);
        assert_ne!(ab, ba, "otro orden, otra forma");
        assert_eq!((f.buscar(ab, 10), f.buscar(ab, 20), f.buscar(ab, 30)), (Some(0), Some(1), None));
        assert_eq!(f.claves(ab), [10, 20]);
        assert_eq!(f.total(), 5);
    }

    #[test]
    fn las_cadenas_comparten_tabla_y_las_ramas_copian() {
        let mut f = Formas::default();
        let mut forma = FORMA_VACIA;
        for k in 0..500 {
            forma = f.anadir(forma, k);
        }
        assert_eq!(f.claves_guardadas(), 500, "una sola tabla para la cadena, no 500²/2");
        let uno = f.anadir(FORMA_VACIA, 0);
        let dos = f.anadir(uno, 1);
        let tres = f.anadir(dos, 2);
        assert_eq!((f.huecos(tres), f.buscar(tres, 2), f.buscar(tres, 3)), (3, Some(2), None), "la 3 no ve la clave 3");
        // Ramificar: otra clave tras {0, 1, 2} copia solo esas tres.
        let rama = f.anadir(tres, 9_999);
        assert_eq!(f.claves(rama), [0, 1, 2, 9_999]);
        assert_eq!((f.buscar(rama, 9_999), f.buscar(rama, 3), f.buscar(forma, 9_999)), (Some(3), None, None));
        assert_eq!(f.claves_guardadas(), 504);
        assert_eq!(f.anadir(tres, 9_999), rama, "la transición se reutiliza");
    }

    #[test]
    fn modo_diccionario_sin_copias() {
        let mut f = Formas::default();
        let mut forma = FORMA_VACIA;
        for k in 0..LIMITE_DICCIONARIO as u32 + 5000 {
            forma = f.anadir(forma, k);
        }
        // Una forma por clave hasta el límite, y una sola (propia) para todo lo demás.
        assert_eq!(f.total(), LIMITE_DICCIONARIO + 2);
        assert!(f.es_diccionario(forma) && !f.es_diccionario(FORMA_VACIA));
        assert_eq!((f.buscar(forma, 7), f.buscar(forma, 3000)), (Some(7), Some(3000)));
        f.quitar(forma, 7);
        assert_eq!((f.buscar(forma, 7), f.buscar(forma, 8), f.claves(forma)[7]), (None, Some(7), 8));
        f.renombrar(forma, 8, 99_999);
        assert_eq!((f.buscar(forma, 8), f.buscar(forma, 99_999)), (None, Some(7)));
    }
}
