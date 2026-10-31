//! Recolector de basura generacional de QUIMERA.
//!
//! El diseño viene de SpiderMonkey (`gc/Nursery.cpp`, `gc/Tenuring.cpp`) y responde a lo que
//! midió el perfil de Boa: en Splay su GC *mark-sweep* se llevaba el 27 % del tiempo,
//! porque traza el montón entero aunque casi todo lo nuevo muere joven.
//!
//! - **Guardería** (*nursery*) como **arena**: un único bloque de valores. Crear un objeto es
//!   copiar sus campos al final del bloque: sin `malloc` y sin buscar hueco. Cuando se llena,
//!   una **recolección menor** copia a la zona principal solo lo que sigue vivo y vacía el
//!   bloque de golpe. Lo que murió no cuesta nada.
//! - **Objetos grandes** (más de [`Config::objeto_grande`] campos) van directos a la zona
//!   principal, como en SpiderMonkey: copiarlos en cada menor saldría caro.
//! - **Zona principal**: lo que sobrevive. Una **recolección mayor** la marca desde las
//!   raíces y libera lo que no se alcanzó (también los ciclos, que un contador no libera).
//! - **Barrera de escritura**: si un objeto de la zona principal pasa a apuntar a uno de la
//!   guardería, se anota, para que la recolección menor no tenga que recorrer la principal.
//! - **Raíces explícitas** (como `Rooted` en SpiderMonkey): el valor que deba sobrevivir a
//!   una asignación tiene que estar en la pila de raíces. Copiar un [`Valor`] no cuesta nada.
//!
//! Este prototipo no usa `unsafe`: las referencias son índices, y una referencia que ya no
//! apunta a nada devuelve `None`.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::mem;
use quimera_valor::{Ref, Valor};

/// Bit de la referencia que dice que el objeto está en la guardería.
const BIT_GUARDERIA: u64 = 1 << 46;
const SIN_DESTINO: u32 = u32::MAX;

/// Un objeto de la zona principal: por ahora, una lista de campos. Las *shapes* llegarán con `quimera-objetos`.
#[derive(Debug, Default)]
struct Celda {
    campos: Vec<Valor>,
    /// Ya está en la lista de recordados (apunta a la guardería).
    recordada: bool,
}

/// Ajustes del montón.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Valores (campos) que caben en la guardería antes de una recolección menor.
    pub guarderia: usize,
    /// Objetos con más campos que esto se crean directamente en la zona principal.
    pub objeto_grande: usize,
    /// Objetos vivos en la zona principal a partir de los cuales se hace la primera mayor.
    pub umbral_mayor_minimo: usize,
}

impl Default for Config {
    fn default() -> Self {
        // 64 Ki valores = 512 KiB de guardería, cerca de lo que cabe en la caché L2.
        Config { guarderia: 64 * 1024, objeto_grande: 256, umbral_mayor_minimo: 65_536 }
    }
}

/// Lo que ha hecho el recolector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Estadisticas {
    /// Objetos creados.
    pub asignados: u64,
    /// Objetos grandes creados directamente en la zona principal.
    pub grandes: u64,
    /// Recolecciones menores (guardería).
    pub menores: u64,
    /// Recolecciones mayores (todo el montón).
    pub mayores: u64,
    /// Objetos copiados de la guardería a la zona principal.
    pub promovidos: u64,
    /// Objetos de la zona principal liberados por una mayor.
    pub liberados: u64,
    /// Objetos vivos ahora en la zona principal.
    pub vivos_principal: usize,
    /// Objetos ahora en la guardería (vivos o no).
    pub en_guarderia: usize,
}

/// Posición en la pila de raíces. Se obtiene con [`Monton::raiz`].
#[derive(Debug, PartialEq, Eq)]
pub struct Raiz(usize);

/// El montón de QUIMERA.
pub struct Monton {
    config: Config,
    /// La arena: los campos de todos los objetos de la guardería, seguidos.
    arena: Vec<Valor>,
    /// Para cada objeto de la guardería: dónde empiezan sus campos y cuántos son.
    objetos: Vec<(u32, u32)>,
    principal: Vec<Option<Celda>>,
    libres: Vec<u32>,
    recordados: Vec<u32>,
    raices: Vec<Valor>,
    // Memoria de trabajo reutilizada entre recolecciones.
    reenvio: Vec<u32>,
    cola: Vec<u32>,
    marcas: Vec<bool>,
    umbral_mayor: usize,
    est: Estadisticas,
}

enum Sitio {
    Guarderia(usize),
    Principal(usize),
}

fn sitio(r: Ref) -> Sitio {
    let n = r.numero();
    if n & BIT_GUARDERIA != 0 { Sitio::Guarderia((n & !BIT_GUARDERIA) as usize) } else { Sitio::Principal(n as usize) }
}

fn ref_guarderia(i: usize) -> Ref {
    Ref::nueva(BIT_GUARDERIA | i as u64).expect("la guardería cabe en 46 bits")
}

fn ref_principal(i: u32) -> Ref {
    Ref::nueva(u64::from(i)).expect("un u32 cabe en 47 bits")
}

impl Default for Monton {
    fn default() -> Self {
        Self::nuevo(Config::default())
    }
}

impl Monton {
    /// Un montón vacío.
    pub fn nuevo(config: Config) -> Monton {
        let config = Config { guarderia: config.guarderia.max(1), ..config };
        Monton {
            arena: Vec::with_capacity(config.guarderia),
            objetos: Vec::new(),
            principal: Vec::new(),
            libres: Vec::new(),
            recordados: Vec::new(),
            raices: Vec::new(),
            reenvio: Vec::new(),
            cola: Vec::new(),
            marcas: Vec::new(),
            umbral_mayor: config.umbral_mayor_minimo,
            est: Estadisticas::default(),
            config,
        }
    }

    // ---------------- raíces ----------------

    /// Pone un valor en la pila de raíces: sobrevivirá a las recolecciones y se actualizará si se mueve.
    pub fn raiz(&mut self, v: Valor) -> Raiz {
        self.raices.push(v);
        Raiz(self.raices.len() - 1)
    }

    /// El valor actual de una raíz (puede haber cambiado si el objeto se movió).
    pub fn valor(&self, r: &Raiz) -> Valor {
        self.raices[r.0]
    }

    /// Cambia el valor de una raíz.
    pub fn fijar(&mut self, r: &Raiz, v: Valor) {
        self.raices[r.0] = v;
    }

    /// Cuántas raíces hay; con [`Monton::soltar_raices`] se vuelve a este punto (como al salir de un bloque).
    pub fn marca_raices(&self) -> usize {
        self.raices.len()
    }

    /// Suelta las raíces creadas después de `marca`.
    pub fn soltar_raices(&mut self, marca: usize) {
        self.raices.truncate(marca);
    }

    // ---------------- objetos ----------------

    /// Crea un objeto con una copia de estos campos y devuelve su referencia.
    ///
    /// Puede provocar una recolección. Los propios `campos` están protegidos; cualquier otro
    /// valor que se quiera conservar debe estar en la pila de raíces.
    pub fn asignar(&mut self, campos: &[Valor]) -> Valor {
        self.est.asignados += 1;
        if campos.len() > self.config.objeto_grande {
            self.est.grandes += 1;
            let destino = self.colocar(Celda { campos: campos.to_vec(), recordada: false });
            // Puede apuntar ya a la guardería: anotarlo como hace la barrera.
            if campos.iter().any(|v| matches!(v.ref_del_monton().map(sitio), Some(Sitio::Guarderia(_)))) {
                self.anotar(destino);
            }
            return Valor::referencia(ref_principal(destino));
        }
        if self.arena.len() + campos.len() > self.config.guarderia {
            let base = self.raices.len();
            self.raices.extend_from_slice(campos);
            self.menor();
            if self.est.vivos_principal > self.umbral_mayor {
                self.mayor();
            }
            let inicio = self.arena.len() as u32;
            self.arena.extend_from_slice(&self.raices[base..]);
            self.raices.truncate(base);
            self.objetos.push((inicio, campos.len() as u32));
        } else {
            let inicio = self.arena.len() as u32;
            self.arena.extend_from_slice(campos);
            self.objetos.push((inicio, campos.len() as u32));
        }
        Valor::referencia(ref_guarderia(self.objetos.len() - 1))
    }

    fn campos(&self, v: Valor) -> Option<&[Valor]> {
        match sitio(v.ref_del_monton()?) {
            Sitio::Guarderia(i) => {
                let &(inicio, largo) = self.objetos.get(i)?;
                self.arena.get(inicio as usize..(inicio + largo) as usize)
            }
            Sitio::Principal(i) => Some(&self.principal.get(i)?.as_ref()?.campos),
        }
    }

    /// Cuántos campos tiene un objeto.
    pub fn largo(&self, obj: Valor) -> Option<usize> {
        Some(self.campos(obj)?.len())
    }

    /// Lee un campo.
    pub fn leer(&self, obj: Valor, campo: usize) -> Option<Valor> {
        self.campos(obj)?.get(campo).copied()
    }

    /// Escribe un campo, con barrera de escritura. `false` si el objeto o el campo no existen.
    pub fn escribir(&mut self, obj: Valor, campo: usize, v: Valor) -> bool {
        let Some(r) = obj.ref_del_monton() else {
            return false;
        };
        match sitio(r) {
            Sitio::Guarderia(i) => {
                let Some(&(inicio, largo)) = self.objetos.get(i) else {
                    return false;
                };
                if campo >= largo as usize {
                    return false;
                }
                self.arena[inicio as usize + campo] = v;
                true
            }
            Sitio::Principal(i) => {
                let Some(Some(celda)) = self.principal.get_mut(i) else {
                    return false;
                };
                let Some(c) = celda.campos.get_mut(campo) else {
                    return false;
                };
                *c = v;
                // Barrera: lo viejo apunta a lo nuevo; anotarlo una sola vez.
                if matches!(v.ref_del_monton().map(sitio), Some(Sitio::Guarderia(_))) {
                    self.anotar(i as u32);
                }
                true
            }
        }
    }

    fn anotar(&mut self, i: u32) {
        if let Some(Some(celda)) = self.principal.get_mut(i as usize)
            && !celda.recordada
        {
            celda.recordada = true;
            self.recordados.push(i);
        }
    }

    /// Lo que ha hecho el recolector hasta ahora.
    pub fn estadisticas(&self) -> Estadisticas {
        Estadisticas { en_guarderia: self.objetos.len(), ..self.est }
    }

    /// Bytes aproximados que ocupan los objetos (arena reservada, campos y cabeceras). Recorre todo el montón.
    pub fn bytes_aprox(&self) -> usize {
        let celda = |c: &Celda| mem::size_of::<Celda>() + c.campos.capacity() * mem::size_of::<Valor>();
        self.arena.capacity() * mem::size_of::<Valor>()
            + self.objetos.capacity() * mem::size_of::<(u32, u32)>()
            + self.principal.iter().map(|c| c.as_ref().map_or(mem::size_of::<Option<Celda>>(), celda)).sum::<usize>()
    }

    // ---------------- recolección menor ----------------

    /// Copia a la zona principal lo que sigue vivo en la guardería y la vacía.
    pub fn menor(&mut self) {
        self.est.menores += 1;
        self.reenvio.clear();
        self.reenvio.resize(self.objetos.len(), SIN_DESTINO);
        self.cola.clear();

        let mut raices = mem::take(&mut self.raices);
        for v in &mut raices {
            *v = self.evacuar(*v);
        }
        self.raices = raices;

        let recordados = mem::take(&mut self.recordados);
        for &i in &recordados {
            self.recorrer_campos(i);
            if let Some(Some(c)) = self.principal.get_mut(i as usize) {
                c.recordada = false;
            }
        }
        self.recordados = recordados;
        self.recordados.clear();

        while let Some(i) = self.cola.pop() {
            self.recorrer_campos(i);
        }
        // Lo que no se copió estaba muerto: se libera todo junto, sin tocarlo.
        self.arena.clear();
        self.objetos.clear();
    }

    /// Actualiza los campos de un objeto de la zona principal, copiando lo que apunte a la guardería.
    fn recorrer_campos(&mut self, i: u32) {
        let Some(Some(celda)) = self.principal.get_mut(i as usize) else {
            return;
        };
        let mut campos = mem::take(&mut celda.campos);
        for v in &mut campos {
            *v = self.evacuar(*v);
        }
        if let Some(Some(celda)) = self.principal.get_mut(i as usize) {
            celda.campos = campos;
        }
    }

    /// Si `v` apunta a la guardería, copia el objeto (una sola vez) y devuelve la nueva referencia.
    fn evacuar(&mut self, v: Valor) -> Valor {
        let Some(r) = v.ref_del_monton() else {
            return v;
        };
        let Sitio::Guarderia(i) = sitio(r) else {
            return v;
        };
        let Some(&(inicio, largo)) = self.objetos.get(i) else {
            return v;
        };
        if self.reenvio[i] != SIN_DESTINO {
            return v.con_ref(ref_principal(self.reenvio[i]));
        }
        let campos = self.arena[inicio as usize..(inicio + largo) as usize].to_vec();
        let destino = self.colocar(Celda { campos, recordada: false });
        self.reenvio[i] = destino;
        self.cola.push(destino);
        self.est.promovidos += 1;
        v.con_ref(ref_principal(destino))
    }

    fn colocar(&mut self, celda: Celda) -> u32 {
        self.est.vivos_principal += 1;
        if let Some(i) = self.libres.pop() {
            self.principal[i as usize] = Some(celda);
            i
        } else {
            self.principal.push(Some(celda));
            u32::try_from(self.principal.len() - 1).expect("menos de 4.294 millones de objetos")
        }
    }

    // ---------------- recolección mayor ----------------

    /// Vacía la guardería, marca desde las raíces y libera lo que no se alcanzó.
    pub fn mayor(&mut self) {
        self.menor();
        self.est.mayores += 1;
        self.marcas.clear();
        self.marcas.resize(self.principal.len(), false);
        self.cola.clear();
        for v in &self.raices {
            if let Some(Sitio::Principal(i)) = v.ref_del_monton().map(sitio)
                && i < self.marcas.len()
                && !self.marcas[i]
            {
                self.marcas[i] = true;
                self.cola.push(i as u32);
            }
        }
        while let Some(i) = self.cola.pop() {
            let Some(Some(celda)) = self.principal.get(i as usize) else {
                continue;
            };
            for v in &celda.campos {
                if let Some(Sitio::Principal(j)) = v.ref_del_monton().map(sitio)
                    && j < self.marcas.len()
                    && !self.marcas[j]
                {
                    self.marcas[j] = true;
                    self.cola.push(j as u32);
                }
            }
        }
        for (i, hueco) in self.principal.iter_mut().enumerate() {
            if hueco.is_some() && !self.marcas[i] {
                *hueco = None;
                self.libres.push(i as u32);
                self.est.vivos_principal -= 1;
                self.est.liberados += 1;
            }
        }
        // La siguiente mayor, cuando lo vivo se duplique.
        self.umbral_mayor = (self.est.vivos_principal * 2).max(self.config.umbral_mayor_minimo);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pequeno() -> Monton {
        Monton::nuevo(Config { guarderia: 256, objeto_grande: 16, umbral_mayor_minimo: 256 })
    }

    #[test]
    fn una_lista_sobrevive_a_muchas_recolecciones() {
        let mut m = pequeno();
        let cabeza = m.raiz(Valor::NULO);
        for n in 0..1000 {
            let siguiente = m.valor(&cabeza);
            let nodo = m.asignar(&[Valor::entero(n), siguiente]);
            m.fijar(&cabeza, nodo);
            for _ in 0..20 {
                m.asignar(&[Valor::entero(-1); 4]); // basura
            }
        }
        let mut v = m.valor(&cabeza);
        for esperado in (0..1000).rev() {
            assert_eq!(m.leer(v, 0), Some(Valor::entero(esperado)));
            v = m.leer(v, 1).unwrap();
        }
        assert_eq!(v, Valor::NULO);
        let e = m.estadisticas();
        assert!(e.menores > 100, "{e:?}");
        assert!(e.mayores > 0, "{e:?}");
        assert!(e.vivos_principal <= 1000 + 64, "la basura se libera: {e:?}");
    }

    #[test]
    fn la_barrera_protege_lo_nuevo_apuntado_desde_lo_viejo() {
        let mut m = pequeno();
        let viejo = m.asignar(&[Valor::NULO]);
        let r = m.raiz(viejo);
        m.menor(); // ahora `viejo` está en la zona principal
        let viejo = m.valor(&r);
        let nuevo = m.asignar(&[Valor::entero(42)]);
        assert!(m.escribir(viejo, 0, nuevo));
        // `nuevo` no está en ninguna raíz: solo lo alcanza la barrera.
        m.menor();
        let hijo = m.leer(m.valor(&r), 0).unwrap();
        assert_eq!(m.leer(hijo, 0), Some(Valor::entero(42)));
    }

    #[test]
    fn los_campos_de_asignar_sobreviven_a_su_propia_recoleccion() {
        let mut m = Monton::nuevo(Config { guarderia: 2, objeto_grande: 16, umbral_mayor_minimo: 1000 });
        let a = m.asignar(&[Valor::entero(7)]);
        m.asignar(&[Valor::NULO]); // guardería llena
        let b = m.asignar(&[a]); // provoca una menor: `a` solo vive en los campos
        let r = m.raiz(b);
        m.menor();
        let a2 = m.leer(m.valor(&r), 0).unwrap();
        assert_eq!(m.leer(a2, 0), Some(Valor::entero(7)));
    }

    #[test]
    fn un_objeto_grande_va_directo_a_la_principal_y_guarda_a_sus_hijos() {
        let mut m = pequeno();
        let hijo = m.asignar(&[Valor::entero(5)]);
        let mut campos = [Valor::NULO; 20];
        campos[3] = hijo;
        let grande = m.asignar(&campos);
        assert_eq!(m.estadisticas().grandes, 1);
        let r = m.raiz(grande);
        m.menor(); // el hijo solo lo alcanza el objeto grande
        let hijo = m.leer(m.valor(&r), 3).unwrap();
        assert_eq!(m.leer(hijo, 0), Some(Valor::entero(5)));
    }

    #[test]
    fn los_ciclos_sin_raiz_se_liberan() {
        let mut m = pequeno();
        let a = m.asignar(&[Valor::NULO]);
        let b = m.asignar(&[a]);
        m.escribir(a, 0, b);
        let ra = m.raiz(a);
        m.mayor();
        assert_eq!(m.estadisticas().vivos_principal, 2);
        m.soltar_raices(ra.0);
        m.mayor();
        assert_eq!(m.estadisticas().vivos_principal, 0, "un contador de referencias no liberaría este ciclo");
    }

    #[test]
    fn las_raices_se_sueltan_como_un_bloque() {
        let mut m = pequeno();
        let marca = m.marca_raices();
        for i in 0..10 {
            let v = m.asignar(&[Valor::entero(i)]);
            m.raiz(v);
        }
        m.mayor();
        assert_eq!(m.estadisticas().vivos_principal, 10);
        m.soltar_raices(marca);
        m.mayor();
        assert_eq!(m.estadisticas().vivos_principal, 0);
    }

    #[test]
    fn una_referencia_invalida_no_rompe_nada() {
        let mut m = pequeno();
        let r = Valor::referencia(Ref::nueva(12345).unwrap());
        assert_eq!(m.leer(r, 0), None);
        assert!(!m.escribir(r, 0, Valor::NULO));
        assert_eq!(m.leer(Valor::entero(3), 0), None);
        let o = m.asignar(&[Valor::NULO]);
        assert_eq!(m.leer(o, 5), None);
        assert!(!m.escribir(o, 5, Valor::NULO));
    }
}
