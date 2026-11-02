//! Expresiones, de la de menor a la de mayor precedencia.

use super::azucar::{
    Firma, Piezas, definir_accesor, expr_a_patron, indefinido, lista_de, llamada_esparcida, llamar, miembro,
    objeto_esparcido, sin_esparcir,
};
use super::{Parser, describir};
use crate::ast::{Clave, Expr, Funcion, Param, Sent};
use crate::fuente::{Resultado, error};
use crate::lexico::Tipo;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const ASIGNACIONES: &[&str] =
    &["=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", ">>>=", "&=", "|=", "^=", "&&=", "||=", "??="];

/// Precedencia de un operador binario (mayor = se agrupa antes) y si asocia a la derecha.
fn precedencia(op: &str) -> Option<(u8, bool)> {
    Some(match op {
        "??" => (1, false),
        "||" => (2, false),
        "&&" => (3, false),
        "|" => (4, false),
        "^" => (5, false),
        "&" => (6, false),
        "==" | "!=" | "===" | "!==" => (7, false),
        "<" | ">" | "<=" | ">=" | "instanceof" | "in" => (8, false),
        "<<" | ">>" | ">>>" => (9, false),
        "+" | "-" => (10, false),
        "*" | "/" | "%" => (11, false),
        "**" => (12, true),
        _ => return None,
    })
}

fn es_destino(e: &Expr) -> bool {
    matches!(e, Expr::Ident(_) | Expr::Miembro(..) | Expr::Indice(..))
}

impl Parser {
    /// `a, b, c`.
    pub(super) fn expresion(&mut self) -> Resultado<Expr> {
        let primera = self.asignacion()?;
        if !self.es(",") {
            return Ok(primera);
        }
        let mut v = vec![primera];
        while self.tomar(",") {
            v.push(self.asignacion()?);
        }
        Ok(Expr::Secuencia(v))
    }

    /// `a = b`, `a += b`, una flecha o una condicional.
    pub(super) fn asignacion(&mut self) -> Resultado<Expr> {
        if self.es("yield") && self.contextos.last().is_some_and(|c| c.0) {
            return self.rendir();
        }
        if let Some(f) = self.flecha()? {
            return Ok(f);
        }
        let pos = self.pos();
        let izquierda = self.condicional()?;
        let op = match self.ver() {
            Tipo::Signo(s) if ASIGNACIONES.contains(s) => *s,
            _ => return Ok(izquierda),
        };
        if op == "="
            && matches!(izquierda, Expr::Arreglo(_) | Expr::Objeto(_) | Expr::Llamar(..))
            && !es_destino(&izquierda)
        {
            // `[a, b] = [b, a]`, `({a, b} = o)`.
            let Some(p) = expr_a_patron(izquierda) else { return error(pos, "no se puede asignar a esto") };
            self.avanzar();
            let derecha = self.asignacion()?;
            return Ok(self.asignar_patron(p, derecha));
        }
        if !es_destino(&izquierda) {
            return error(pos, "no se puede asignar a esto");
        }
        self.avanzar();
        let derecha = self.asignacion()?;
        Ok(Expr::Asignar(op, Box::new(izquierda), Box::new(derecha)))
    }

    /// `await e`: un `yield e` (en una `async`), o un `yield __quimera_espera(e)` (en un
    /// `async function*`, donde un `yield` normal entrega un valor).
    pub(super) fn esperar_expr(&mut self, e: Expr) -> Expr {
        let en_generador = self.contextos.last().is_some_and(|c| c.0 && c.1);
        let valor = if en_generador { llamar("__quimera_espera", vec![e]) } else { e };
        Expr::Rendir { valor: Some(Box::new(valor)), delegar: false }
    }

    /// `yield`, `yield v` o `yield* v` (ya se sabe que se está en un generador).
    fn rendir(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let delegar = self.tomar("*");
        let fin = self.token(0).salto_antes
            || matches!(self.ver(), Tipo::Signo(")" | "]" | "}" | "," | ";" | ":") | Tipo::Fin)
            || self.es("in") && self.sin_in;
        let valor = if fin && !delegar { None } else { Some(Box::new(self.asignacion()?)) };
        Ok(Expr::Rendir { valor, delegar })
    }

    /// Si empieza una flecha (`x =>`, `(a, b) =>`, también `async x =>`), la analiza.
    fn flecha(&mut self) -> Resultado<Option<Expr>> {
        let pos = self.pos();
        let asincrona = self.es_palabra("async")
            && !self.token(1).salto_antes
            && ((matches!(self.token(1).tipo, Tipo::Ident(_)) && self.es_en(2, "=>"))
                || (self.es_en(1, "(") && self.cierre_desde(1).is_some_and(|c| self.es_en(c + 1, "=>"))));
        if asincrona {
            self.avanzar();
        }
        let simple = matches!(self.ver(), Tipo::Ident(_)) && self.es_en(1, "=>");
        if !simple && !(self.es("(") && self.cierre_desde(0).is_some_and(|c| self.es_en(c + 1, "=>"))) {
            return Ok(None);
        }
        self.contextos.push((false, asincrona));
        self.abrir_temporales();
        let mut firma = if simple {
            Firma { params: vec![Param { nombre: self.ident()?, defecto: None }], prologo: Vec::new(), resto: None }
        } else {
            self.firma()?
        };
        if self.token(0).salto_antes {
            return error(self.pos(), "no puede haber un salto de línea antes de =>");
        }
        self.esperar("=>")?;
        let mut cuerpo = if self.es("{") {
            self.bloque()?
        } else {
            let sin_in = core::mem::replace(&mut self.sin_in, false);
            let e = self.asignacion();
            self.sin_in = sin_in;
            vec![Sent::Retornar(Some(e?))]
        };
        // Una flecha no tiene `arguments`: con `...rest` se hace función normal y `.bind(this)`.
        let con_resto = firma.resto.is_some();
        Self::prologo(&mut firma, &mut cuerpo);
        self.cerrar_temporales(&mut cuerpo);
        self.contextos.pop();
        if asincrona {
            let args = if con_resto { Expr::Ident(String::from("arguments")) } else { Expr::Arreglo(Vec::new()) };
            cuerpo = super::clases::envolver_async(cuerpo, args, pos);
        }
        let f = Expr::Funcion(Box::new(Funcion {
            nombre: None,
            params: firma.params,
            cuerpo,
            flecha: !con_resto,
            pos,
            generador: false,
        }));
        Ok(Some(if con_resto { Expr::Llamar(Box::new(miembro(f, "bind")), vec![Expr::Este]) } else { f }))
    }

    fn condicional(&mut self) -> Resultado<Expr> {
        let c = self.binaria(0)?;
        if !self.tomar("?") {
            return Ok(c);
        }
        let sin_in = core::mem::replace(&mut self.sin_in, false);
        let a = self.asignacion();
        self.sin_in = sin_in;
        let a = a?;
        self.esperar(":")?;
        let b = self.asignacion()?;
        Ok(Expr::Condicional(Box::new(c), Box::new(a), Box::new(b)))
    }

    /// Operadores binarios por precedencia (todos los de precedencia mayor que `minima`).
    fn binaria(&mut self, minima: u8) -> Resultado<Expr> {
        let mut izquierda = self.unaria()?;
        loop {
            let op = match self.ver() {
                Tipo::Signo(s) => *s,
                Tipo::Clave(k @ ("instanceof" | "in")) if !(self.sin_in && *k == "in") => *k,
                _ => return Ok(izquierda),
            };
            let Some((prec, derecha_asoc)) = precedencia(op) else { return Ok(izquierda) };
            if prec <= minima {
                return Ok(izquierda);
            }
            self.avanzar();
            let siguiente = if derecha_asoc { prec - 1 } else { prec };
            let derecha = self.binaria(siguiente)?;
            izquierda = match op {
                "&&" | "||" | "??" => Expr::Logica(op, Box::new(izquierda), Box::new(derecha)),
                _ => Expr::Binaria(op, Box::new(izquierda), Box::new(derecha)),
            };
        }
    }

    fn unaria(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        let op = match self.ver() {
            Tipo::Signo(s @ ("!" | "~" | "+" | "-" | "++" | "--")) => *s,
            Tipo::Clave(k @ ("typeof" | "void" | "delete")) => *k,
            Tipo::Ident(n) if n == "await" && self.contextos.last().is_some_and(|c| c.1) => "await",
            _ => return self.postfija(),
        };
        self.avanzar();
        let e = self.unaria()?;
        if op == "await" {
            return Ok(self.esperar_expr(e));
        }
        if op == "++" || op == "--" {
            if !es_destino(&e) {
                return error(pos, format!("{op} necesita una variable o una propiedad"));
            }
            return Ok(Expr::Actualizar { op, prefijo: true, destino: Box::new(e) });
        }
        if self.es("**") {
            return error(self.pos(), "una unaria delante de ** necesita paréntesis: (-a) ** b");
        }
        Ok(Expr::Unaria(op, Box::new(e)))
    }

    fn postfija(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        let e = self.llamada_o_miembro()?;
        if (self.es("++") || self.es("--")) && !self.token(0).salto_antes {
            if !es_destino(&e) {
                return error(pos, "++/-- necesita una variable o una propiedad");
            }
            let op = if self.es("++") { "++" } else { "--" };
            self.avanzar();
            return Ok(Expr::Actualizar { op, prefijo: false, destino: Box::new(e) });
        }
        Ok(e)
    }

    /// `new F(x)`, `a.b`, `a[b]`, `f(x)`, `a?.b`, encadenados.
    pub(super) fn llamada_o_miembro(&mut self) -> Resultado<Expr> {
        let mut e = if self.es("new") { self.nuevo()? } else { self.primaria()? };
        // Cada `?.`: (temporal, lo que se comprueba, si se comprueba). Al final, la cadena entera
        // queda dentro de `(t = base) == null ? undefined : ...`.
        let mut opcionales: Vec<(String, Expr, bool)> = Vec::new();
        loop {
            if self.tomar(".") {
                e = Expr::Miembro(Box::new(e), self.nombre_de_propiedad()?);
            } else if self.tomar("[") {
                let i = self.expresion()?;
                self.esperar("]")?;
                e = Expr::Indice(Box::new(e), Box::new(i));
            } else if self.es("(") {
                let piezas = self.piezas_entre("(", ")")?;
                e = match sin_esparcir(piezas) {
                    Ok(args) => Expr::Llamar(Box::new(e), args),
                    Err(p) => llamada_esparcida(e, p),
                };
            } else if self.tomar("?.") {
                // Lo raro, al final y aparte: este bucle corre en cada expresión.
                self.eslabon_opcional(&mut e, &mut opcionales)?;
            } else {
                break;
            }
        }
        for (t, base, comprobar) in opcionales.into_iter().rev() {
            let asignar = Expr::Asignar("=", Box::new(Expr::Ident(t)), Box::new(base));
            e = if comprobar {
                let nulo = Expr::Binaria("==", Box::new(asignar), Box::new(Expr::Nulo));
                Expr::Condicional(Box::new(nulo), Box::new(indefinido()), Box::new(e))
            } else {
                Expr::Secuencia(vec![asignar, e])
            };
        }
        Ok(e)
    }

    /// Un eslabón tras `?.` (ya tomado): `.b`, `[k]` o `(args)`, que queda dentro de la
    /// comprobación de `null`/`undefined` que se pone al final de la cadena.
    fn eslabon_opcional(&mut self, e: &mut Expr, opcionales: &mut Vec<(String, Expr, bool)>) -> Resultado<()> {
        let t = self.temporal_var();
        let base = core::mem::replace(e, Expr::Ident(t.clone()));
        if self.es("(") {
            let piezas = self.piezas_entre("(", ")")?;
            // `o.m?.()`: se comprueba la función y se llama con `this` = `o` (guardado
            // en otro temporal, sin comprobar).
            let objeto = match base {
                Expr::Miembro(o, n) => {
                    let t_o = self.temporal_var();
                    opcionales.push((t_o.clone(), *o, false));
                    opcionales.push((t.clone(), Expr::Miembro(Box::new(Expr::Ident(t_o.clone())), n), true));
                    Some(t_o)
                }
                Expr::Indice(o, k) => {
                    let t_o = self.temporal_var();
                    opcionales.push((t_o.clone(), *o, false));
                    opcionales.push((t.clone(), Expr::Indice(Box::new(Expr::Ident(t_o.clone())), k), true));
                    Some(t_o)
                }
                b => {
                    opcionales.push((t.clone(), b, true));
                    None
                }
            };
            let f = Expr::Ident(t);
            *e = match (objeto, sin_esparcir(piezas)) {
                (None, Ok(args)) => Expr::Llamar(Box::new(f), args),
                (None, Err(p)) => llamada_esparcida(f, p),
                (Some(o), Ok(mut args)) => {
                    args.insert(0, Expr::Ident(o));
                    Expr::Llamar(Box::new(miembro(f, "call")), args)
                }
                (Some(o), Err(p)) => Expr::Llamar(Box::new(miembro(f, "apply")), vec![Expr::Ident(o), lista_de(p)]),
            };
            return Ok(());
        }
        opcionales.push((t.clone(), base, true));
        let objeto = core::mem::replace(e, Expr::Nulo);
        *e = if self.tomar("[") {
            let i = self.expresion()?;
            self.esperar("]")?;
            Expr::Indice(Box::new(objeto), Box::new(i))
        } else {
            Expr::Miembro(Box::new(objeto), self.nombre_de_propiedad()?)
        };
        Ok(())
    }

    /// `new F(args)` o `new F` (sin paréntesis no hay argumentos).
    fn nuevo(&mut self) -> Resultado<Expr> {
        self.avanzar();
        if self.es(".") {
            return error(self.pos(), "new.target todavía no está en QUIMERA");
        }
        let mut f = if self.es("new") { self.nuevo()? } else { self.primaria()? };
        loop {
            if self.tomar(".") {
                f = Expr::Miembro(Box::new(f), self.nombre_de_propiedad()?);
            } else if self.tomar("[") {
                let i = self.expresion()?;
                self.esperar("]")?;
                f = Expr::Indice(Box::new(f), Box::new(i));
            } else {
                break;
            }
        }
        let piezas = if self.es("(") {
            self.piezas_entre("(", ")")?
        } else {
            Piezas { exprs: Vec::new(), esparcidas: Vec::new() }
        };
        Ok(match sin_esparcir(piezas) {
            Ok(args) => Expr::Nuevo(Box::new(f), args),
            Err(p) => llamar("__quimera_nuevo", vec![f, lista_de(p)]),
        })
    }

    /// Tras `.`: cualquier nombre, también palabras reservadas (`a.default`, `a.new`).
    pub(super) fn nombre_de_propiedad(&mut self) -> Resultado<String> {
        match self.avanzar().tipo {
            Tipo::Ident(n) => Ok(n),
            Tipo::Clave(k) => Ok(String::from(k)),
            Tipo::Signo("#") => Ok(format!("#{}", self.ident()?)),
            otro => error(self.pos(), format!("se esperaba un nombre de propiedad y hay {}", describir(&otro))),
        }
    }

    fn primaria(&mut self) -> Resultado<Expr> {
        let pos = self.pos();
        let e = match self.ver().clone() {
            Tipo::Numero(n) => Expr::Numero(n),
            Tipo::Cadena(s) => Expr::Cadena(s),
            Tipo::Ident(n) if n == "async" && self.es_en(1, "function") && !self.token(1).salto_antes => {
                self.avanzar();
                return Ok(Expr::Funcion(Box::new(self.funcion_asincrona_o_no(false, true)?)));
            }
            Tipo::Ident(n) => Expr::Ident(n),
            Tipo::Clave("true") => Expr::Bool(true),
            Tipo::Clave("false") => Expr::Bool(false),
            Tipo::Clave("null") => Expr::Nulo,
            Tipo::Clave("this") => Expr::Este,
            Tipo::Clave("function") => return Ok(Expr::Funcion(Box::new(self.funcion(false)?))),
            Tipo::Signo("(") => return self.parentesis(),
            Tipo::Signo("[") => return self.arreglo(),
            Tipo::Signo("{") => return self.objeto(),
            Tipo::Regex(patron, banderas) => {
                // Error temprano, como en los navegadores: un patrón mal escrito no llega a ejecutarse.
                if let Err(e) = quimera_regex::Regex::nueva(&patron, &banderas) {
                    return error(pos, format!("expresión regular no válida /{patron}/{banderas}: {}", e.mensaje));
                }
                Expr::Regex(patron, banderas)
            }
            Tipo::Clave("class") => return Ok(self.clase()?.1),
            Tipo::Clave("super") => return self.super_(),
            Tipo::Clave("import") if self.es_en(1, "(") => {
                // `import(x)`: el módulo, como promesa (relativo al módulo actual, o a la página).
                self.avanzar();
                let mut piezas = self.piezas_entre("(", ")")?;
                let especificador =
                    if piezas.exprs.is_empty() { Expr::Cadena(String::new()) } else { piezas.exprs.remove(0) };
                let f = if self.modulo { "__importar" } else { "__quimera_importar_dinamico" };
                return Ok(llamar(f, vec![especificador]));
            }
            Tipo::Clave("import") if self.modulo && self.es_en(1, ".") => {
                self.avanzar();
                self.avanzar();
                if !self.es_palabra("meta") {
                    return error(pos, "tras `import.` solo puede ir `meta`");
                }
                self.avanzar();
                return Ok(Expr::Ident(String::from("__meta")));
            }
            Tipo::Clave(k @ ("import" | "yield")) => {
                return error(pos, format!("`{k}` aquí necesita un módulo o un generador"));
            }
            otro => return error(pos, format!("se esperaba una expresión y hay {}", describir(&otro))),
        };
        self.avanzar();
        Ok(e)
    }

    fn parentesis(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let sin_in = core::mem::replace(&mut self.sin_in, false);
        let e = self.expresion();
        self.sin_in = sin_in;
        let e = e?;
        self.esperar(")")?;
        Ok(e)
    }

    fn arreglo(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let mut v: Vec<Option<Expr>> = Vec::new();
        let mut esparcidas = Vec::new();
        while !self.tomar("]") {
            if self.tomar(",") {
                v.push(None);
                continue;
            }
            if self.tomar("...") {
                esparcidas.push(v.len());
            }
            v.push(Some(self.asignacion()?));
            if !self.es("]") {
                self.esperar(",")?;
            }
        }
        if esparcidas.is_empty() {
            return Ok(Expr::Arreglo(v));
        }
        let exprs = v.into_iter().map(|e| e.unwrap_or_else(indefinido)).collect();
        Ok(lista_de(Piezas { exprs, esparcidas }))
    }

    fn objeto(&mut self) -> Resultado<Expr> {
        self.avanzar();
        let mut props = Vec::new();
        // Solo si aparece `...`: los trozos de propiedades y lo esparcido, en orden.
        let mut segmentos: Vec<Result<Vec<(Clave, Expr)>, Expr>> = Vec::new();
        // `get x() {}` / `set x(v) {}`: se definen sobre el objeto ya creado.
        let mut accesores: Vec<(Clave, bool, Funcion)> = Vec::new();
        let mut proto: Option<Expr> = None;
        while !self.tomar("}") {
            let es_accesor = (self.es_palabra("get") || self.es_palabra("set"))
                && !(self.es_en(1, ":") || self.es_en(1, "(") || self.es_en(1, ",") || self.es_en(1, "}"));
            if es_accesor {
                let pos = self.pos();
                let es_get = self.es_palabra("get");
                self.avanzar();
                let clave = self.clave_de_propiedad()?;
                let f = self.funcion_de_metodo(None, pos)?;
                // Un hueco en su sitio (si el nombre se conoce): el accesor lo ocupará y las
                // claves quedan en el orden del código.
                if let Clave::Nombre(n) = &clave
                    && !props.iter().any(|(k, _)| matches!(k, Clave::Nombre(x) if x == n))
                {
                    props.push((clave.clone(), indefinido()));
                }
                accesores.push((clave, es_get, f));
            } else if self.tomar("...") {
                if !props.is_empty() {
                    segmentos.push(Ok(core::mem::take(&mut props)));
                }
                segmentos.push(Err(self.asignacion()?));
            } else {
                let es_proto = self.es_palabra("__proto__") && self.es_en(1, ":")
                    || matches!(self.ver(), Tipo::Cadena(c) if c == "__proto__") && self.es_en(1, ":");
                let p = self.propiedad()?;
                if es_proto {
                    // `{__proto__: P}` no crea una propiedad: fija el prototipo.
                    proto = Some(p.1);
                } else {
                    props.push(p);
                }
            }
            if !self.es("}") {
                self.esperar(",")?;
            }
        }
        let mut objeto = if segmentos.is_empty() {
            Expr::Objeto(props)
        } else {
            if !props.is_empty() {
                segmentos.push(Ok(props));
            }
            objeto_esparcido(segmentos)
        };
        // `Object.defineProperty` devuelve el objeto: se encadenan.
        for (clave, es_get, f) in accesores {
            objeto = definir_accesor(objeto, clave, es_get, f, true);
        }
        if let Some(p) = proto {
            let poner = miembro(Expr::Ident(String::from("Object")), "setPrototypeOf");
            objeto = Expr::Llamar(Box::new(poner), vec![objeto, p]);
        }
        Ok(objeto)
    }

    /// `a: 1`, `a` (abreviada), `[k]: 1`, `"a": 1`, `1: x` o un método `f() {}`.
    fn propiedad(&mut self) -> Resultado<(Clave, Expr)> {
        let pos = self.pos();
        let asincrona = self.es_palabra("async")
            && !(self.es_en(1, ":") || self.es_en(1, "(") || self.es_en(1, ",") || self.es_en(1, "}"))
            && !self.token(1).salto_antes;
        if asincrona {
            self.avanzar();
        }
        let generador = self.tomar("*");
        if asincrona || generador {
            let clave = self.clave_de_propiedad()?;
            let nombre = if let Clave::Nombre(n) = &clave { Some(n.clone()) } else { None };
            let f = self.funcion_con(nombre, pos, generador, asincrona)?;
            return Ok((clave, Expr::Funcion(Box::new(f))));
        }
        let abreviable = matches!(self.ver(), Tipo::Ident(_));
        let clave = self.clave_de_propiedad()?;
        if self.es("(") {
            let nombre = if let Clave::Nombre(n) = &clave { Some(n.clone()) } else { None };
            let f = self.funcion_de_metodo(nombre, pos)?;
            return Ok((clave, Expr::Funcion(Box::new(f))));
        }
        if self.tomar(":") {
            return Ok((clave, self.asignacion()?));
        }
        match (&clave, abreviable) {
            (Clave::Nombre(n), true) => {
                let valor = Expr::Ident(n.clone());
                Ok((clave, valor))
            }
            _ => error(self.pos(), "se esperaba `:` tras la clave"),
        }
    }
}
