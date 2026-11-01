//! El bytecode de QUIMERA: instrucciones de registros dentro de un *Stencil*.
//!
//! Cada función tiene sus registros (`r0`, `r1`...). Los parámetros llegan en los primeros.
//! Las variables que alguna función interna captura no viven en registros sino en un
//! **entorno** (como los *aliased bindings* de SpiderMonkey); el resto, en registros, que
//! es lo más rápido. Nada aquí apunta al GC: las cadenas son índices a la tabla de átomos y
//! las funciones internas, índices a la lista de funciones del Stencil.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Un registro de la función.
pub type Reg = u16;

/// Operaciones de dos operandos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpBin {
    /// `+`
    Suma,
    /// `-`
    Resta,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Resto,
    /// `**`
    Potencia,
    /// `==`
    Igual,
    /// `!=`
    Distinto,
    /// `===`
    Estricto,
    /// `!==`
    NoEstricto,
    /// `<`
    Menor,
    /// `<=`
    MenorIgual,
    /// `>`
    Mayor,
    /// `>=`
    MayorIgual,
    /// `&`
    YBits,
    /// `|`
    OBits,
    /// `^`
    XorBits,
    /// `<<`
    Izq,
    /// `>>`
    Der,
    /// `>>>`
    DerSinSigno,
    /// `in`
    En,
    /// `instanceof`
    Instancia,
}

impl OpBin {
    /// El operador de JavaScript que le corresponde.
    pub fn desde(op: &str) -> Option<OpBin> {
        use OpBin::*;
        Some(match op {
            "+" => Suma,
            "-" => Resta,
            "*" => Mul,
            "/" => Div,
            "%" => Resto,
            "**" => Potencia,
            "==" => Igual,
            "!=" => Distinto,
            "===" => Estricto,
            "!==" => NoEstricto,
            "<" => Menor,
            "<=" => MenorIgual,
            ">" => Mayor,
            ">=" => MayorIgual,
            "&" => YBits,
            "|" => OBits,
            "^" => XorBits,
            "<<" => Izq,
            ">>" => Der,
            ">>>" => DerSinSigno,
            "in" => En,
            "instanceof" => Instancia,
            _ => return None,
        })
    }
}

/// Operaciones de un operando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpUn {
    /// `-a`
    Negar,
    /// `+a` (convierte a número)
    ANumero,
    /// `!a`
    No,
    /// `~a`
    NoBits,
    /// `typeof a`
    Tipo,
    /// `void a`
    Vacio,
}

/// Un valor constante de la tabla de la función.
#[derive(Debug, Clone, PartialEq)]
pub enum Constante {
    /// Un número que no cabe en un entero de 32 bits (o es `-0`).
    Numero(f64),
    /// Una cadena (índice a la tabla de átomos del Stencil).
    Cadena(u32),
}

/// Una instrucción. `a`, `b`, `obj`... son registros; `nombre` es un átomo; `destino` es una posición del código.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)] // los campos (dst, a, b, obj...) se explican en el doc de cada instrucción
pub enum Instr {
    /// `dst = valor` (entero de 32 bits).
    Entero { dst: Reg, valor: i32 },
    /// `dst = constantes[k]`.
    Constante { dst: Reg, k: u32 },
    /// `dst = undefined`.
    Indefinido { dst: Reg },
    /// `dst = null`.
    Nulo { dst: Reg },
    /// `dst = true/false`.
    Booleano { dst: Reg, valor: bool },
    /// `dst = this`.
    Este { dst: Reg },
    /// `dst` = la función que se está ejecutando (para `function g() { g() }`).
    FuncionActual { dst: Reg },
    /// `dst = src`.
    Mover { dst: Reg, src: Reg },
    /// `dst = a op b`.
    Binaria { op: OpBin, dst: Reg, a: Reg, b: Reg },
    /// `dst = a op valor` con un entero dentro de la instrucción (`i + 1`, `i++`, `x -= 2`).
    BinariaInm { op: OpBin, dst: Reg, a: Reg, valor: i32 },
    /// `dst = op a`.
    Unaria { op: OpUn, dst: Reg, a: Reg },
    /// Salta a `destino`.
    Saltar { destino: u32 },
    /// Salta si `c` es verdadero.
    SaltarSi { c: Reg, destino: u32 },
    /// Salta si `c` es falso.
    SaltarSiNo { c: Reg, destino: u32 },
    /// Compara y salta en una sola instrucción: salta a `destino` si `a op b` es falso (`i < n`).
    SaltarSiNoCompara { op: OpBin, a: Reg, b: Reg, destino: u32 },
    /// Salta si `c` no es `null` ni `undefined` (para `??`).
    SaltarSiDefinido { c: Reg, destino: u32 },
    /// `dst = globalThis[nombre]`.
    LeerGlobal { dst: Reg, nombre: u32 },
    /// `globalThis[nombre] = src`.
    EscribirGlobal { nombre: u32, src: Reg },
    /// `dst = typeof nombre` para una global: si no existe da `"undefined"` en vez de un error.
    TipoGlobal { dst: Reg, nombre: u32 },
    /// Crea el entorno de la función con `huecos` variables capturadas.
    CrearEntorno { huecos: u16 },
    /// Lee el hueco `hueco` del entorno que está `saltos` niveles hacia fuera.
    LeerEntorno { dst: Reg, saltos: u16, hueco: u16 },
    /// Escribe el hueco `hueco` del entorno que está `saltos` niveles hacia fuera.
    EscribirEntorno { saltos: u16, hueco: u16, src: Reg },
    /// `dst = obj.nombre`.
    LeerProp { dst: Reg, obj: Reg, nombre: u32 },
    /// `obj.nombre = src`.
    EscribirProp { obj: Reg, nombre: u32, src: Reg },
    /// `dst = obj[indice]`.
    LeerIndice { dst: Reg, obj: Reg, indice: Reg },
    /// `obj[indice] = src`.
    EscribirIndice { obj: Reg, indice: Reg, src: Reg },
    /// `dst = delete obj.nombre`.
    BorrarProp { dst: Reg, obj: Reg, nombre: u32 },
    /// `dst = delete obj[indice]`.
    BorrarIndice { dst: Reg, obj: Reg, indice: Reg },
    /// `dst` = arreglo con las claves enumerables de `obj` (`for-in`).
    IterarClaves { dst: Reg, obj: Reg },
    /// `dst` = arreglo con los valores de `obj` (`for-of` sobre arreglos y cadenas).
    IterarValores { dst: Reg, obj: Reg },
    /// `dst = {}`.
    CrearObjeto { dst: Reg },
    /// Un arreglo con los `n` registros seguidos desde `desde` (los huecos llevan un valor especial).
    CrearArreglo { dst: Reg, desde: Reg, n: u16 },
    /// Un registro con el valor "hueco" de los arreglos (`[a, , b]`).
    Hueco { dst: Reg },
    /// Crea la variable global `nombre` a `undefined` si no existe (`var` del programa).
    DeclararGlobal { nombre: u32 },
    /// Crea un cierre de la función `funcion` del Stencil con el entorno actual.
    Cierre { dst: Reg, funcion: u32 },
    /// Llama a `f` con `este` como `this` y los `n` argumentos seguidos desde `args`.
    Llamar { dst: Reg, f: Reg, este: Reg, args: Reg, n: u16 },
    /// `dst = new f(args...)`.
    Construir { dst: Reg, f: Reg, args: Reg, n: u16 },
    /// `return src`.
    Retornar { src: Reg },
    /// `throw src`.
    Lanzar { src: Reg },
    /// Desde aquí, una excepción salta a `captura`.
    Intentar { captura: u32 },
    /// Fin del bloque protegido por el último `Intentar`.
    FinIntentar,
    /// Al llegar a `captura`: guarda la excepción.
    Excepcion { dst: Reg },
}

/// Una función compilada.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Funcion {
    /// Nombre (átomo), si lo tiene.
    pub nombre: Option<u32>,
    /// Cuántos parámetros (llegan en `r0`, `r1`...).
    pub params: u16,
    /// Cuántos registros usa en total.
    pub registros: u16,
    /// Huecos de su entorno (0: no crea entorno).
    pub entorno: u16,
    /// Es una flecha (`this` léxico).
    pub flecha: bool,
    /// Registro donde la máquina deja el arreglo `arguments` al llamarla (solo si lo usa).
    pub argumentos: Option<Reg>,
    /// Las instrucciones.
    pub codigo: Vec<Instr>,
    /// Sus constantes.
    pub constantes: Vec<Constante>,
}

/// El resultado de compilar un programa. La función 0 es el programa.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stencil {
    /// Todas las funciones, la 0 es el programa.
    pub funciones: Vec<Funcion>,
    /// Tabla de cadenas (nombres de variables y propiedades, cadenas literales).
    pub atomos: Vec<String>,
}

impl Stencil {
    /// Total de instrucciones de todas las funciones.
    pub fn instrucciones(&self) -> usize {
        self.funciones.iter().map(|f| f.codigo.len()).sum()
    }
}

impl fmt::Display for Stencil {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, fun) in self.funciones.iter().enumerate() {
            let nombre = fun.nombre.map_or("(anónima)", |n| self.atomos[n as usize].as_str());
            let nombre = if i == 0 { "(programa)" } else { nombre };
            writeln!(
                f,
                "función {i} {nombre}: {} parámetros, {} registros, entorno {}{}",
                fun.params,
                fun.registros,
                fun.entorno,
                if fun.flecha { ", flecha" } else { "" }
            )?;
            for (pc, ins) in fun.codigo.iter().enumerate() {
                writeln!(f, "  {pc:04}  {}", self.mostrar(fun, ins))?;
            }
        }
        Ok(())
    }
}

impl Stencil {
    fn atomo(&self, n: u32) -> &str {
        self.atomos.get(n as usize).map_or("?", String::as_str)
    }

    /// Una instrucción legible: `r2 = r0 + r1`, `r1 = global "Math"`...
    pub fn mostrar(&self, fun: &Funcion, ins: &Instr) -> String {
        use Instr::*;
        let k = |n: u32| match fun.constantes.get(n as usize) {
            Some(crate::bytecode::Constante::Numero(x)) => alloc::format!("{x}"),
            Some(crate::bytecode::Constante::Cadena(a)) => alloc::format!("{:?}", self.atomo(*a)),
            None => String::from("?"),
        };
        match ins {
            Entero { dst, valor } => alloc::format!("r{dst} = {valor}"),
            Constante { dst, k: n } => alloc::format!("r{dst} = {}", k(*n)),
            Indefinido { dst } => alloc::format!("r{dst} = undefined"),
            Nulo { dst } => alloc::format!("r{dst} = null"),
            Booleano { dst, valor } => alloc::format!("r{dst} = {valor}"),
            Este { dst } => alloc::format!("r{dst} = this"),
            FuncionActual { dst } => alloc::format!("r{dst} = función actual"),
            Mover { dst, src } => alloc::format!("r{dst} = r{src}"),
            Binaria { op, dst, a, b } => alloc::format!("r{dst} = r{a} {op:?} r{b}"),
            BinariaInm { op, dst, a, valor } => alloc::format!("r{dst} = r{a} {op:?} {valor}"),
            Unaria { op, dst, a } => alloc::format!("r{dst} = {op:?} r{a}"),
            Saltar { destino } => alloc::format!("saltar {destino:04}"),
            SaltarSi { c, destino } => alloc::format!("si r{c} saltar {destino:04}"),
            SaltarSiNo { c, destino } => alloc::format!("si no r{c} saltar {destino:04}"),
            SaltarSiNoCompara { op, a, b, destino } => alloc::format!("si no r{a} {op:?} r{b} saltar {destino:04}"),
            SaltarSiDefinido { c, destino } => alloc::format!("si r{c} no es null/undefined saltar {destino:04}"),
            LeerGlobal { dst, nombre } => alloc::format!("r{dst} = global {:?}", self.atomo(*nombre)),
            EscribirGlobal { nombre, src } => alloc::format!("global {:?} = r{src}", self.atomo(*nombre)),
            TipoGlobal { dst, nombre } => alloc::format!("r{dst} = typeof global {:?}", self.atomo(*nombre)),
            CrearEntorno { huecos } => alloc::format!("crear entorno de {huecos}"),
            LeerEntorno { dst, saltos, hueco } => alloc::format!("r{dst} = entorno[{saltos}][{hueco}]"),
            EscribirEntorno { saltos, hueco, src } => alloc::format!("entorno[{saltos}][{hueco}] = r{src}"),
            LeerProp { dst, obj, nombre } => alloc::format!("r{dst} = r{obj}.{}", self.atomo(*nombre)),
            EscribirProp { obj, nombre, src } => alloc::format!("r{obj}.{} = r{src}", self.atomo(*nombre)),
            LeerIndice { dst, obj, indice } => alloc::format!("r{dst} = r{obj}[r{indice}]"),
            EscribirIndice { obj, indice, src } => alloc::format!("r{obj}[r{indice}] = r{src}"),
            BorrarProp { dst, obj, nombre } => alloc::format!("r{dst} = delete r{obj}.{}", self.atomo(*nombre)),
            BorrarIndice { dst, obj, indice } => alloc::format!("r{dst} = delete r{obj}[r{indice}]"),
            IterarClaves { dst, obj } => alloc::format!("r{dst} = claves(r{obj})"),
            IterarValores { dst, obj } => alloc::format!("r{dst} = valores(r{obj})"),
            CrearObjeto { dst } => alloc::format!("r{dst} = {{}}"),
            CrearArreglo { dst, desde, n } => alloc::format!("r{dst} = [r{desde}..+{n}]"),
            Hueco { dst } => alloc::format!("r{dst} = hueco"),
            DeclararGlobal { nombre } => alloc::format!("var global {:?}", self.atomo(*nombre)),
            Cierre { dst, funcion } => alloc::format!("r{dst} = cierre función {funcion}"),
            Llamar { dst, f, este, args, n } => alloc::format!("r{dst} = r{f}.call(r{este}, r{args}..+{n})"),
            Construir { dst, f, args, n } => alloc::format!("r{dst} = new r{f}(r{args}..+{n})"),
            Retornar { src } => alloc::format!("return r{src}"),
            Lanzar { src } => alloc::format!("throw r{src}"),
            Intentar { captura } => alloc::format!("try (catch en {captura:04})"),
            FinIntentar => String::from("fin try"),
            Excepcion { dst } => alloc::format!("r{dst} = excepción"),
        }
    }
}
