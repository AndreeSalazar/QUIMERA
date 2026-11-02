//! El `this` del constructor de una clase derivada.
//!
//! En JavaScript, `super(...)` *sustituye* el `this` del constructor por lo que crea el padre:
//! un elemento del DOM (`class B extends HTMLElement`), un `Error`, un arreglo... Una función
//! normal no puede cambiar su `this`, así que dentro del constructor `this` pasa a ser una
//! variable que `super(...)` reasigna:
//!
//! ```text
//! constructor() { super(); this.x = 1; }
//! →  var este = this; este = __quimera_super(Padre, este, [], campos); este.x = 1; return este;
//! ```
//!
//! Se entra en las funciones flecha (comparten `this`), no en las normales ni en las clases de
//! dentro (tienen el suyo).

use crate::ast::{Clave, Expr, Sent};
use alloc::boxed::Box;
use alloc::string::String;

/// El nombre de la variable que hace de `this`.
pub(super) const ESTE: &str = "\u{1}este";

fn este() -> Expr {
    Expr::Ident(String::from(ESTE))
}

/// Cambia `this` por la variable en las sentencias de un constructor derivado.
pub(super) fn reemplazar(cuerpo: &mut [Sent]) {
    for s in cuerpo {
        sentencia(s);
    }
}

fn sentencia(s: &mut Sent) {
    match s {
        Sent::Declarar(_, lista) => lista.iter_mut().filter_map(|(_, e)| e.as_mut()).for_each(expr),
        Sent::Expr(e) | Sent::Lanzar(e) | Sent::Retornar(Some(e)) => expr(e),
        // `return;` en un constructor derivado devuelve el `this` (que ya no es el de la llamada).
        Sent::Retornar(None) => *s = Sent::Retornar(Some(este())),
        Sent::Si(c, a, b) => {
            expr(c);
            sentencia(a);
            if let Some(b) = b {
                sentencia(b);
            }
        }
        Sent::Mientras(c, cuerpo) | Sent::Hacer(cuerpo, c) => {
            expr(c);
            sentencia(cuerpo);
        }
        Sent::Para { inicio, cond, paso, cuerpo } => {
            if let Some(i) = inicio {
                sentencia(i);
            }
            cond.iter_mut().chain(paso.iter_mut()).for_each(expr);
            sentencia(cuerpo);
        }
        Sent::ParaEn { objeto, cuerpo, .. } => {
            expr(objeto);
            sentencia(cuerpo);
        }
        Sent::Intentar { cuerpo, captura, final_ } => {
            reemplazar(cuerpo);
            if let Some((_, b)) = captura {
                reemplazar(b);
            }
            if let Some(f) = final_ {
                reemplazar(f);
            }
        }
        Sent::Elegir(x, casos) => {
            expr(x);
            for (prueba, cuerpo) in casos {
                if let Some(p) = prueba {
                    expr(p);
                }
                reemplazar(cuerpo);
            }
        }
        Sent::Bloque(b) => reemplazar(b),
        Sent::Etiquetada(_, s) => sentencia(s),
        Sent::Funcion(_)
        | Sent::Romper
        | Sent::Continuar
        | Sent::RomperEtiqueta(_)
        | Sent::ContinuarEtiqueta(_)
        | Sent::Vacio => {}
    }
}

fn expr(e: &mut Expr) {
    match e {
        Expr::Este => *e = este(),
        Expr::Llamar(f, args) => {
            expr(f);
            args.iter_mut().for_each(expr);
            // `super(...)` (ya convertido): su resultado es el nuevo `this`.
            if matches!(&**f, Expr::Ident(n) if n == "__quimera_super") {
                let llamada = core::mem::replace(e, Expr::Nulo);
                *e = Expr::Asignar("=", Box::new(este()), Box::new(llamada));
            }
        }
        Expr::Funcion(f) if f.flecha => reemplazar(&mut f.cuerpo),
        Expr::Funcion(_) => {}
        Expr::Arreglo(v) => v.iter_mut().flatten().for_each(expr),
        Expr::Objeto(props) => {
            for (k, v) in props {
                if let Clave::Calculada(k) = k {
                    expr(k);
                }
                expr(v);
            }
        }
        Expr::Unaria(_, a) | Expr::Miembro(a, _) => expr(a),
        Expr::Actualizar { destino, .. } => expr(destino),
        Expr::Binaria(_, a, b) | Expr::Logica(_, a, b) | Expr::Asignar(_, a, b) | Expr::Indice(a, b) => {
            expr(a);
            expr(b);
        }
        Expr::Condicional(c, a, b) => {
            expr(c);
            expr(a);
            expr(b);
        }
        Expr::Nuevo(f, args) => {
            expr(f);
            args.iter_mut().for_each(expr);
        }
        Expr::Secuencia(v) => v.iter_mut().for_each(expr),
        Expr::Rendir { valor, .. } => {
            if let Some(v) = valor {
                expr(v);
            }
        }
        Expr::Ident(_) | Expr::Numero(_) | Expr::Cadena(_) | Expr::Regex(..) | Expr::Bool(_) | Expr::Nulo => {}
    }
}
