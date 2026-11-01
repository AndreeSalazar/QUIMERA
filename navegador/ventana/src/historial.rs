//! El historial de una pestaña: atrás y adelante.

use quimera_red::Url;

/// Las páginas visitadas y dónde se está.
#[derive(Debug, Default)]
pub struct Historial {
    entradas: Vec<Url>,
    actual: usize,
}

impl Historial {
    /// Ir a una página nueva: lo que había «adelante» se olvida, como en los navegadores.
    pub fn ir(&mut self, url: Url) {
        if !self.entradas.is_empty() {
            self.entradas.truncate(self.actual + 1);
        }
        self.entradas.push(url);
        self.actual = self.entradas.len() - 1;
    }

    /// Cambia la entrada actual (por ejemplo, por la URL final tras una redirección).
    pub fn sustituir(&mut self, url: Url) {
        match self.entradas.get_mut(self.actual) {
            Some(e) => *e = url,
            None => self.ir(url),
        }
    }

    /// Un paso atrás, si se puede.
    pub fn atras(&mut self) -> Option<Url> {
        (self.actual > 0).then(|| {
            self.actual -= 1;
            self.entradas[self.actual].clone()
        })
    }

    /// Un paso adelante, si se puede.
    pub fn adelante(&mut self) -> Option<Url> {
        (self.actual + 1 < self.entradas.len()).then(|| {
            self.actual += 1;
            self.entradas[self.actual].clone()
        })
    }

    /// Si hay a dónde volver.
    pub fn puede_atras(&self) -> bool {
        self.actual > 0
    }

    /// Si hay a dónde avanzar.
    pub fn puede_adelante(&self) -> bool {
        self.actual + 1 < self.entradas.len()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn u(s: &str) -> Url {
        Url::analizar(s).unwrap()
    }

    #[test]
    fn atras_adelante_y_olvidar_lo_de_adelante() {
        let mut h = Historial::default();
        assert!(!h.puede_atras() && h.atras().is_none());
        h.ir(u("https://a.com/"));
        h.ir(u("https://b.com/"));
        h.ir(u("https://c.com/"));
        assert_eq!(h.atras(), Some(u("https://b.com/")));
        assert_eq!(h.atras(), Some(u("https://a.com/")));
        assert!(h.atras().is_none());
        assert_eq!(h.adelante(), Some(u("https://b.com/")));
        h.ir(u("https://d.com/"));
        assert!(!h.puede_adelante(), "ir a otra página borra c.com de adelante");
        h.sustituir(u("https://d.com/final"));
        assert_eq!(h.atras(), Some(u("https://b.com/")));
        assert_eq!(h.adelante(), Some(u("https://d.com/final")));
    }
}
