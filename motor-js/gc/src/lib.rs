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
//! - **Zona principal**: lo que sobrevive. Sus campos también van **seguidos en un solo bloque**
//!   (`datos`): cada objeto es (inicio, largo) en él, sin un `malloc` ni una cabecera por objeto,
//!   y leer un campo es un salto, no dos. Una **recolección mayor** la marca desde las raíces y
//!   libera lo que no se alcanzó (también los ciclos, que un contador no libera); el hueco de lo
//!   liberado vuelve a una lista por tamaño para el siguiente objeto de ese tamaño.
//! - **Barrera de escritura**: si un objeto de la zona principal pasa a apuntar a uno de la
//!   guardería, se anota, para que la recolección menor no tenga que recorrer la principal.
//! - **Raíces explícitas** (como `Rooted` en SpiderMonkey): el valor que deba sobrevivir a
//!   una asignación tiene que estar en la pila de raíces. Copiar un [`Valor`] no cuesta nada.
//!
//! Este prototipo no usa `unsafe`: las referencias son índices, y una referencia que ya no
//! apunta a nada devuelve `None`. Solo sigue referencias a **objetos**: las cadenas y los
//! símbolos todavía viven en su propia tabla (`quimera-objetos`), no en este montón.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::mem;
use quimera_valor::{Ref, Tipo, Valor};

/// Bytes de fuera del montón (cadenas) que provocan como mínimo una recolección mayor.
const UMBRAL_EXTERNO_MINIMO: usize = 8 << 20;

/// Bit de la referencia que dice que el objeto está en la guardería.
const BIT_GUARDERIA: u64 = 1 << 46;
const SIN_DESTINO: u32 = u32::MAX;

/// Un objeto de la zona principal: dónde están sus campos en `datos` y cuántos son.
#[derive(Debug, Clone, Copy, Default)]
struct Entrada {
    inicio: u32,
    largo: u32,
    /// Hay un objeto (si no, el número está libre para el siguiente).
    viva: bool,
    /// Ya está en la lista de recordados (apunta a la guardería).
    recordada: bool,
}

/// Tamaños con su propia lista de huecos libres (los más grandes van a un árbol).
const CLASES: usize = 64;

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
    /// Los objetos de la zona principal, por su número.
    principal: Vec<Entrada>,
    /// Los campos de todos los objetos de la zona principal, seguidos.
    datos: Vec<Valor>,
    /// Números de objeto libres.
    libres: Vec<u32>,
    /// Huecos libres en `datos`, por tamaño: dónde empieza cada uno.
    huecos: Vec<Vec<u32>>,
    huecos_grandes: BTreeMap<u32, Vec<u32>>,
    recordados: Vec<u32>,
    raices: Vec<Valor>,
    // Memoria de trabajo reutilizada entre recolecciones.
    reenvio: Vec<u32>,
    cola: Vec<u32>,
    marcas: Vec<bool>,
    /// Las cadenas que vio la última recolección mayor (un bit por número de cadena).
    cadenas_vistas: Vec<u64>,
    /// Bytes pedidos fuera del montón (cadenas) desde la última mayor, y cuántos la provocan.
    externo: usize,
    umbral_externo: usize,
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
    Ref::desde_u32(i)
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
            datos: Vec::new(),
            libres: Vec::new(),
            huecos: alloc::vec![Vec::new(); CLASES + 1],
            huecos_grandes: BTreeMap::new(),
            recordados: Vec::new(),
            raices: Vec::new(),
            reenvio: Vec::new(),
            cola: Vec::new(),
            marcas: Vec::new(),
            cadenas_vistas: Vec::new(),
            externo: 0,
            umbral_externo: UMBRAL_EXTERNO_MINIMO,
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

    /// La pila de raíces entera. La máquina virtual guarda aquí sus registros: así, si el GC
    /// mueve un objeto, los registros que lo apuntan se actualizan solos.
    pub fn pila(&self) -> &[Valor] {
        &self.raices
    }

    /// La pila de raíces, para leer y escribir registros sin pasar por [`Raiz`].
    pub fn pila_mut(&mut self) -> &mut [Valor] {
        &mut self.raices
    }

    /// Añade `n` raíces con `valor` (abre el marco de una llamada) y devuelve dónde empiezan.
    pub fn abrir_raices(&mut self, n: usize, valor: Valor) -> usize {
        let base = self.raices.len();
        self.raices.resize(base + n, valor);
        base
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
            let destino = self.colocar(campos);
            // Puede apuntar ya a la guardería: anotarlo como hace la barrera.
            if campos.iter().any(|v| matches!(v.como_referencia().map(sitio), Some(Sitio::Guarderia(_)))) {
                self.anotar(destino);
            }
            return Valor::referencia(ref_principal(destino));
        }
        if self.arena.len() + campos.len() > self.config.guarderia || self.quiere_mayor() {
            let base = self.raices.len();
            self.raices.extend_from_slice(campos);
            self.menor();
            if self.est.vivos_principal > self.umbral_mayor || self.quiere_mayor() {
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

    /// Todos los campos de un objeto de una vez: para leer varios sin buscar el objeto cada vez.
    #[inline]
    pub fn campos(&self, v: Valor) -> Option<&[Valor]> {
        match sitio(v.como_referencia()?) {
            Sitio::Guarderia(i) => {
                let &(inicio, largo) = self.objetos.get(i)?;
                self.arena.get(inicio as usize..(inicio + largo) as usize)
            }
            Sitio::Principal(i) => {
                let e = self.principal.get(i).filter(|e| e.viva)?;
                self.datos.get(e.inicio as usize..(e.inicio + e.largo) as usize)
            }
        }
    }

    /// Cuántos campos tiene un objeto.
    pub fn largo(&self, obj: Valor) -> Option<usize> {
        Some(self.campos(obj)?.len())
    }

    /// Lee un campo.
    #[inline]
    pub fn leer(&self, obj: Valor, campo: usize) -> Option<Valor> {
        self.campos(obj)?.get(campo).copied()
    }

    /// Escribe un campo, con barrera de escritura. `false` si el objeto o el campo no existen.
    pub fn escribir(&mut self, obj: Valor, campo: usize, v: Valor) -> bool {
        let Some(r) = obj.como_referencia() else {
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
                let Some(&e) = self.principal.get(i).filter(|e| e.viva) else {
                    return false;
                };
                if campo >= e.largo as usize {
                    return false;
                }
                self.datos[e.inicio as usize + campo] = v;
                // Barrera: lo viejo apunta a lo nuevo; anotarlo una sola vez.
                if matches!(v.como_referencia().map(sitio), Some(Sitio::Guarderia(_))) {
                    self.anotar(i as u32);
                }
                true
            }
        }
    }

    fn anotar(&mut self, i: u32) {
        if let Some(e) = self.principal.get_mut(i as usize)
            && e.viva
            && !e.recordada
        {
            e.recordada = true;
            self.recordados.push(i);
        }
    }

    /// Lo que ha hecho el recolector hasta ahora.
    pub fn estadisticas(&self) -> Estadisticas {
        Estadisticas { en_guarderia: self.objetos.len(), ..self.est }
    }

    /// Bytes aproximados que ocupan los objetos (arena reservada, campos y cabeceras). Recorre todo el montón.
    pub fn bytes_aprox(&self) -> usize {
        (self.arena.capacity() + self.datos.capacity()) * mem::size_of::<Valor>()
            + self.objetos.capacity() * mem::size_of::<(u32, u32)>()
            + self.principal.capacity() * mem::size_of::<Entrada>()
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
            if let Some(e) = self.principal.get_mut(i as usize) {
                e.recordada = false;
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
        let Some(&e) = self.principal.get(i as usize).filter(|e| e.viva) else {
            return;
        };
        // Por índice: evacuar puede hacer crecer `datos` (y moverlo en memoria).
        for k in e.inicio as usize..(e.inicio + e.largo) as usize {
            let v = self.evacuar(self.datos[k]);
            self.datos[k] = v;
        }
    }

    /// Si `v` apunta a la guardería, copia el objeto (una sola vez) y devuelve la nueva referencia.
    fn evacuar(&mut self, v: Valor) -> Valor {
        let Some(r) = v.como_referencia() else {
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
        let destino = self.colocar_desde_arena(inicio, largo);
        self.reenvio[i] = destino;
        self.cola.push(destino);
        self.est.promovidos += 1;
        v.con_ref(ref_principal(destino))
    }

    /// Un hueco de `largo` campos en `datos`: uno liberado de ese tamaño o uno nuevo al final.
    fn reservar(&mut self, largo: u32) -> u32 {
        let libre = if (largo as usize) <= CLASES {
            self.huecos[largo as usize].pop()
        } else {
            self.huecos_grandes.get_mut(&largo).and_then(Vec::pop)
        };
        libre.unwrap_or_else(|| {
            let inicio = self.datos.len();
            self.datos.resize(inicio + largo as usize, Valor::INDEFINIDO);
            u32::try_from(inicio).expect("la zona principal cabe en 4.294 millones de campos")
        })
    }

    /// Un número de objeto para el hueco `inicio` de `largo` campos.
    fn entrada(&mut self, inicio: u32, largo: u32) -> u32 {
        self.est.vivos_principal += 1;
        let e = Entrada { inicio, largo, viva: true, recordada: false };
        if let Some(i) = self.libres.pop() {
            self.principal[i as usize] = e;
            i
        } else {
            self.principal.push(e);
            u32::try_from(self.principal.len() - 1).expect("menos de 4.294 millones de objetos")
        }
    }

    /// Un objeto nuevo en la zona principal con una copia de estos campos.
    fn colocar(&mut self, campos: &[Valor]) -> u32 {
        let largo = campos.len() as u32;
        let inicio = self.reservar(largo);
        self.datos[inicio as usize..(inicio + largo) as usize].copy_from_slice(campos);
        self.entrada(inicio, largo)
    }

    /// Copia a la zona principal el objeto de la guardería que está en `desde` (sin copias intermedias).
    fn colocar_desde_arena(&mut self, desde: u32, largo: u32) -> u32 {
        let inicio = self.reservar(largo);
        let (d, a) = (inicio as usize, desde as usize);
        self.datos[d..d + largo as usize].copy_from_slice(&self.arena[a..a + largo as usize]);
        self.entrada(inicio, largo)
    }

    // ---------------- recolección mayor ----------------

    /// Anota `bytes` pedidos fuera del montón (las cadenas). Cuando pasan del umbral, la
    /// siguiente asignación hace una mayor, para que quien guarda esa memoria pueda liberarla.
    pub fn anotar_externo(&mut self, bytes: usize) {
        self.externo += bytes;
    }

    /// Lo de fuera que sigue vivo tras liberar: la siguiente mayor llega cuando se pida otro tanto.
    pub fn externo_vivo(&mut self, bytes: usize) {
        self.umbral_externo = bytes.max(UMBRAL_EXTERNO_MINIMO);
    }

    /// Si la memoria de fuera pide ya una recolección mayor. Quien ejecuta código puede hacerla
    /// en un punto seguro (con todo lo vivo en las raíces) sin esperar a la siguiente asignación.
    #[inline]
    pub fn quiere_mayor(&self) -> bool {
        self.externo > self.umbral_externo
    }

    /// Las cadenas que alcanzó la última recolección mayor: un bit por número de cadena.
    /// Quien guarda las cadenas libera las demás (las cadenas viven fuera del montón).
    pub fn cadenas_vistas(&self) -> &[u64] {
        &self.cadenas_vistas
    }

    /// Vacía la guardería, marca desde las raíces y libera lo que no se alcanzó.
    pub fn mayor(&mut self) {
        self.menor();
        self.est.mayores += 1;
        self.externo = 0;
        self.marcas.clear();
        self.marcas.resize(self.principal.len(), false);
        self.cola.clear();
        self.cadenas_vistas.clear();
        for v in &self.raices {
            anotar_cadena(&mut self.cadenas_vistas, *v);
            if let Some(Sitio::Principal(i)) = v.como_referencia().map(sitio)
                && i < self.marcas.len()
                && !self.marcas[i]
            {
                self.marcas[i] = true;
                self.cola.push(i as u32);
            }
        }
        while let Some(i) = self.cola.pop() {
            let Some(&e) = self.principal.get(i as usize).filter(|e| e.viva) else {
                continue;
            };
            for v in &self.datos[e.inicio as usize..(e.inicio + e.largo) as usize] {
                anotar_cadena(&mut self.cadenas_vistas, *v);
                if let Some(Sitio::Principal(j)) = v.como_referencia().map(sitio)
                    && j < self.marcas.len()
                    && !self.marcas[j]
                {
                    self.marcas[j] = true;
                    self.cola.push(j as u32);
                }
            }
        }
        for (i, e) in self.principal.iter_mut().enumerate() {
            if e.viva && !self.marcas[i] {
                e.viva = false;
                self.libres.push(i as u32);
                // Su hueco en `datos`, para el siguiente objeto de su tamaño.
                if (e.largo as usize) <= CLASES {
                    self.huecos[e.largo as usize].push(e.inicio);
                } else {
                    self.huecos_grandes.entry(e.largo).or_default().push(e.inicio);
                }
                self.est.vivos_principal -= 1;
                self.est.liberados += 1;
            }
        }
        // La siguiente mayor, cuando lo vivo se duplique.
        self.umbral_mayor = (self.est.vivos_principal * 2).max(self.config.umbral_mayor_minimo);
    }
}

/// Si `v` es una cadena, pone su bit.
#[inline]
fn anotar_cadena(bits: &mut Vec<u64>, v: Valor) {
    if v.tipo() != Tipo::Cadena {
        return;
    }
    let Some(r) = v.ref_del_monton() else { return };
    let n = r.numero() as usize;
    if bits.len() <= n / 64 {
        bits.resize(n / 64 + 1, 0);
    }
    bits[n / 64] |= 1 << (n % 64);
}

#[cfg(test)]
mod pruebas;
