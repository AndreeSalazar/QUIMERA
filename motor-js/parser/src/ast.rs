//! Árbol de sintaxis de JavaScript (la parte que QUIMERA entiende por ahora).

use crate::fuente::Pos;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// `var`, `let` o `const`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decl {
    /// `var`: de función.
    Var,
    /// `let`: de bloque.
    Let,
    /// `const`: de bloque, sin reasignar.
    Const,
}

/// Un parámetro, con su valor por defecto.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Nombre.
    pub nombre: String,
    /// `= valor` si lo tiene.
    pub defecto: Option<Expr>,
}

/// Una función (declaración, expresión, método o flecha).
#[derive(Debug, Clone, PartialEq)]
pub struct Funcion {
    /// Nombre, si lo tiene.
    pub nombre: Option<String>,
    /// Parámetros.
    pub params: Vec<Param>,
    /// Cuerpo. Una flecha con expresión (`x => x + 1`) se guarda como `return x + 1`.
    pub cuerpo: Vec<Sent>,
    /// Es una flecha (`this` léxico, sin `arguments`).
    pub flecha: bool,
    /// Es un generador (`function*`). Las `async` llegan ya convertidas en generadores.
    pub generador: bool,
    /// Dónde empieza.
    pub pos: Pos,
}

/// La clave de una propiedad en un objeto literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Clave {
    /// `a`, `"a"` o `1` (los números se guardan como texto, como hace JavaScript).
    Nombre(String),
    /// `[expresión]`.
    Calculada(Expr),
}

/// Una expresión.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// `1.5`.
    Numero(f64),
    /// `"texto"`.
    Cadena(String),
    /// `/patrón/banderas`.
    Regex(String, String),
    /// `true` / `false`.
    Bool(bool),
    /// `null`.
    Nulo,
    /// Un nombre de variable.
    Ident(String),
    /// `this`.
    Este,
    /// `[a, , b]` (un hueco es `None`).
    Arreglo(Vec<Option<Expr>>),
    /// `{a: 1, b}`.
    Objeto(Vec<(Clave, Expr)>),
    /// `function () {}` o `() => {}`.
    Funcion(Box<Funcion>),
    /// `!a`, `-a`, `typeof a`, `void a`, `delete a.b`...
    Unaria(&'static str, Box<Expr>),
    /// `++a`, `a--`...
    Actualizar {
        /// `++` o `--`.
        op: &'static str,
        /// Va delante (`++a`).
        prefijo: bool,
        /// Lo que cambia.
        destino: Box<Expr>,
    },
    /// `a + b`, `a === b`, `a instanceof b`...
    Binaria(&'static str, Box<Expr>, Box<Expr>),
    /// `a && b`, `a || b`, `a ?? b` (evalúan `b` solo si hace falta).
    Logica(&'static str, Box<Expr>, Box<Expr>),
    /// `a = b`, `a += b`...
    Asignar(&'static str, Box<Expr>, Box<Expr>),
    /// `c ? a : b`.
    Condicional(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `f(a, b)`.
    Llamar(Box<Expr>, Vec<Expr>),
    /// `new F(a)`.
    Nuevo(Box<Expr>, Vec<Expr>),
    /// `a.b`.
    Miembro(Box<Expr>, String),
    /// `a[b]`.
    Indice(Box<Expr>, Box<Expr>),
    /// `a, b`.
    Secuencia(Vec<Expr>),
    /// `yield v`, `yield* v` (dentro de un generador) o `await v` (dentro de una `async`, que
    /// se convierte en generador).
    Rendir {
        /// Lo que se entrega (`None`: `undefined`).
        valor: Option<Box<Expr>>,
        /// `yield*`: entrega uno a uno los valores de otro iterable.
        delegar: bool,
    },
}

/// Un caso de `switch` (`None` es `default`).
pub type Caso = (Option<Expr>, Vec<Sent>);

/// Una sentencia.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    /// `let a = 1, b;`
    Declarar(Decl, Vec<(String, Option<Expr>)>),
    /// `function f() {}`
    Funcion(Funcion),
    /// Una expresión sola.
    Expr(Expr),
    /// `if (c) a else b`.
    Si(Expr, Box<Sent>, Option<Box<Sent>>),
    /// `while (c) a`.
    Mientras(Expr, Box<Sent>),
    /// `do a while (c)`.
    Hacer(Box<Sent>, Expr),
    /// `for (inicio; cond; paso) cuerpo`.
    Para {
        /// Declaración o expresión inicial.
        inicio: Option<Box<Sent>>,
        /// Condición.
        cond: Option<Expr>,
        /// Paso.
        paso: Option<Expr>,
        /// Cuerpo.
        cuerpo: Box<Sent>,
    },
    /// `for (let x in obj)` o `for (const x of lista)`.
    ParaEn {
        /// La declaración, si la hay.
        decl: Option<Decl>,
        /// La variable.
        nombre: String,
        /// `of` (si no, `in`).
        de: bool,
        /// Lo que se recorre.
        objeto: Expr,
        /// Cuerpo.
        cuerpo: Box<Sent>,
    },
    /// `return x;`
    Retornar(Option<Expr>),
    /// `break;`
    Romper,
    /// `continue;`
    Continuar,
    /// `throw x;`
    Lanzar(Expr),
    /// `try {} catch (e) {} finally {}`.
    Intentar {
        /// El bloque `try`.
        cuerpo: Vec<Sent>,
        /// `catch`: nombre de la variable (si la tiene) y bloque.
        captura: Option<(Option<String>, Vec<Sent>)>,
        /// `finally`.
        final_: Option<Vec<Sent>>,
    },
    /// `switch (x) { case a: ... default: ... }`.
    Elegir(Expr, Vec<Caso>),
    /// `{ ... }`.
    Bloque(Vec<Sent>),
    /// `etiqueta: sentencia`.
    Etiquetada(String, Box<Sent>),
    /// `break etiqueta;`
    RomperEtiqueta(String),
    /// `continue etiqueta;`
    ContinuarEtiqueta(String),
    /// `;`
    Vacio,
}
