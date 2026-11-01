//! Perfil del intérprete (con la característica `perfil`): cuántas veces se ejecuta cada tipo de
//! instrucción y cuánto tiempo se lleva, con el cronómetro que da el anfitrión.
//!
//! Sirve para saber qué acelerar sin `perf`. Medir cada instrucción cuesta dos lecturas del
//! cronómetro, así que los tiempos absolutos salen inflados: lo que cuenta es la proporción.
//! A cada instrucción se le cuenta solo su tiempo propio: si ejecuta otras dentro (`apply`, una
//! conversión que llama a `valueOf`), el de esas se descuenta.

use crate::Motor;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::Discriminant;
use quimera_frontend::bytecode::Instr;

/// Lo anotado de un tipo de instrucción.
pub(crate) struct Entrada {
    tipo: Discriminant<Instr>,
    nombre: String,
    veces: u64,
    nanos: u64,
}

/// El perfil de un motor.
#[derive(Default)]
pub(crate) struct Perfil {
    pub cronometro: Option<fn() -> u64>,
    entradas: Vec<Entrada>,
    /// Tiempo propio acumulado de todas las instrucciones anotadas.
    acumulado: u64,
}

impl Perfil {
    /// La hora del cronómetro y el tiempo acumulado hasta ahora, si hay cronómetro.
    pub(crate) fn ahora(&self) -> Option<(u64, u64)> {
        self.cronometro.map(|f| (f(), self.acumulado))
    }

    /// Anota una instrucción que empezó en `inicio`.
    pub(crate) fn anotar(&mut self, ins: &Instr, inicio: Option<(u64, u64)>) {
        let (Some((inicio, antes)), Some(f)) = (inicio, self.cronometro) else { return };
        let dentro = self.acumulado - antes; // lo de las instrucciones que corrieron dentro
        let propio = f().saturating_sub(inicio).saturating_sub(dentro);
        self.acumulado += propio;
        let tipo = core::mem::discriminant(ins);
        let e = match self.entradas.iter().position(|e| e.tipo == tipo) {
            Some(i) => &mut self.entradas[i],
            None => {
                let texto = alloc::format!("{ins:?}");
                let nombre = String::from(texto.split([' ', '{', '(']).next().unwrap_or(""));
                self.entradas.push(Entrada { tipo, nombre, veces: 0, nanos: 0 });
                let ultimo = self.entradas.len() - 1;
                &mut self.entradas[ultimo]
            }
        };
        e.veces += 1;
        e.nanos += propio;
    }
}

impl Motor {
    /// Activa el perfil con un cronómetro en nanosegundos.
    pub fn perfilar(&mut self, cronometro: fn() -> u64) {
        self.perfil.cronometro = Some(cronometro);
    }

    /// (instrucción, veces, nanosegundos), de la que más tiempo se lleva a la que menos.
    pub fn informe_perfil(&self) -> Vec<(String, u64, u64)> {
        let mut v: Vec<(String, u64, u64)> =
            self.perfil.entradas.iter().map(|e| (e.nombre.clone(), e.veces, e.nanos)).collect();
        v.sort_by_key(|x| core::cmp::Reverse(x.2));
        v
    }
}
