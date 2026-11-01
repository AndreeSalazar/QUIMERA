//! Hojas de estilo, recuperación de errores, selectores y valores.

use quimera_css::{Color, ElementoCss, Longitud, analizar, color, declaraciones, longitud, piezas};

/// (nombre, atributos, padre)
type NodoDePrueba = (&'static str, Vec<(&'static str, &'static str)>, Option<usize>);

/// Un árbol diminuto para probar los selectores sin depender de `quimera-html`.
struct Arbol {
    nodos: Vec<NodoDePrueba>,
}

#[derive(Clone, Copy)]
struct El<'a>(&'a Arbol, usize);

impl<'a> ElementoCss for El<'a> {
    fn nombre(&self) -> &str {
        self.0.nodos[self.1].0
    }
    fn atributo(&self, n: &str) -> Option<&str> {
        self.0.nodos[self.1].1.iter().find(|(k, _)| *k == n).map(|(_, v)| *v)
    }
    fn padre(&self) -> Option<Self> {
        self.0.nodos[self.1].2.map(|p| El(self.0, p))
    }
    fn anterior(&self) -> Option<Self> {
        let p = self.0.nodos[self.1].2;
        (0..self.1).rev().find(|&i| self.0.nodos[i].2 == p && p.is_some()).map(|i| El(self.0, i))
    }
    fn siguiente(&self) -> Option<Self> {
        let p = self.0.nodos[self.1].2;
        (self.1 + 1..self.0.nodos.len()).find(|&i| self.0.nodos[i].2 == p && p.is_some()).map(|i| El(self.0, i))
    }
    fn vacio(&self) -> bool {
        !self.0.nodos.iter().any(|n| n.2 == Some(self.1))
    }
}

/// html > body > (div#principal.caja.grande > (p.nota, p, a[href]), p)
fn arbol() -> Arbol {
    Arbol {
        nodos: vec![
            ("html", vec![], None),                                                // 0
            ("body", vec![], Some(0)),                                             // 1
            ("div", vec![("id", "principal"), ("class", "caja grande")], Some(1)), // 2
            ("p", vec![("class", "nota"), ("lang", "es-PE")], Some(2)),            // 3
            ("p", vec![], Some(2)),                                                // 4
            ("a", vec![("href", "https://quimera.dev/x.pdf")], Some(2)),           // 5
            ("p", vec![], Some(1)),                                                // 6
        ],
    }
}

fn casa(selector: &str, nodo: usize) -> bool {
    let a = arbol();
    let hoja = analizar(&format!("{selector} {{}}"));
    let r = hoja.reglas.first().unwrap_or_else(|| panic!("{selector} no se analizó"));
    r.selectores.iter().any(|s| s.casa(El(&a, nodo)))
}

#[test]
fn selectores_casan_como_en_los_navegadores() {
    assert!(casa("p", 3) && casa("*", 3) && casa(".nota", 3) && casa("p.nota", 3) && !casa("div.nota", 3));
    assert!(casa("#principal", 2) && casa("div#principal.caja.grande", 2) && !casa("#otro", 2));
    assert!(casa("body p", 3) && casa("div > p", 4) && !casa("body > p", 4) && casa("body > p", 6));
    assert!(casa("p + p", 4) && !casa("p + p", 3) && casa("p ~ a", 5) && casa("div ~ p", 6));
    assert!(casa("[href]", 5) && casa("a[href$='.pdf']", 5) && casa("[href^=https]", 5) && casa("[href*=quimera]", 5));
    assert!(casa("[class~=caja]", 2) && !casa("[class=caja]", 2) && casa("[lang|=es]", 3));
    assert!(casa("p:first-child", 3) && !casa("p:first-child", 4) && casa("a:last-child", 5) && casa(":root", 0));
    assert!(casa("a:link", 5) && casa("p:empty", 4) && !casa("p:hover", 3) && !casa("p::before", 3));
    assert!(casa("h1, p.nota", 3), "lista de selectores");
}

#[test]
fn especificidad() {
    let esp = |s: &str| analizar(&format!("{s} {{}}")).reglas[0].selectores[0].especificidad();
    assert_eq!(esp("p"), (0, 0, 1));
    assert_eq!(esp("div p.nota"), (0, 1, 2));
    assert_eq!(esp("#a .b:first-child [c]"), (1, 3, 0));
    assert_eq!(esp("a::before"), (0, 0, 2));
    assert_eq!(esp("*"), (0, 0, 0));
}

#[test]
fn reglas_medios_y_recuperacion_de_errores() {
    let hoja = analizar(
        "@charset 'utf-8'; @import url(x.css);
         body { margin: 0; color: red !important; }
         p { color: ; bad decl; font-size: 12px }     /* dos declaraciones malas se saltan */
         p:: { color: blue }                            /* selector malo: regla fuera */
         @font-face { font-family: X; src: url(x) }
         @media screen and (max-width: 600px) { .a { display: none } }
         h1 { font: bold 2em/1.2 \"Fira Sans\", sans-serif }",
    );
    let cuantas: Vec<usize> = hoja.reglas.iter().map(|r| r.declaraciones.len()).collect();
    assert_eq!(cuantas, [2, 1, 1], "body (2), p (1: las malas fuera), h1 (1); `p::` se descarta");
    let body = &hoja.reglas[0].declaraciones;
    assert_eq!((body[0].propiedad.as_str(), body[1].importante), ("margin", true));
    assert_eq!(color(&body[1].valor), Some(Color::rgb(255, 0, 0)));
    assert_eq!(hoja.reglas[1].declaraciones[0].propiedad, "font-size");
    assert_eq!(hoja.medios.len(), 1);
    assert_eq!(hoja.medios[0].condicion, "screen and (max-width: 600px)");
    assert_eq!(hoja.medios[0].reglas[0].declaraciones[0].propiedad, "display");
}

#[test]
fn atributo_style() {
    let d = declaraciones("color: #0af; BACKGROUND-color: rgba(0,0,0,.5) ;; --Mi-Var: 3");
    let props: Vec<&str> = d.iter().map(|x| x.propiedad.as_str()).collect();
    assert_eq!(props, ["color", "background-color", "--Mi-Var"]);
    assert_eq!(color(&d[0].valor), Some(Color::rgb(0, 170, 255)));
    assert_eq!(color(&d[1].valor), Some(Color { r: 0, g: 0, b: 0, a: 128 }));
}

#[test]
fn colores_y_longitudes() {
    let c = |s: &str| color(&declaraciones(&format!("x: {s}"))[0].valor);
    assert_eq!(c("RebeccaPurple"), Some(Color::rgb(102, 51, 153)));
    assert_eq!(c("#abcd"), Some(Color { r: 0xaa, g: 0xbb, b: 0xcc, a: 0xdd }));
    assert_eq!(c("rgb(100% 50% 0% / 50%)"), Some(Color { r: 255, g: 128, b: 0, a: 128 }));
    assert_eq!(c("hsl(120deg, 100%, 25%)"), Some(Color::rgb(0, 128, 0)));
    assert_eq!(c("transparent"), Some(Color::TRANSPARENTE));
    assert_eq!(c("notacolor"), None);
    assert_eq!(c("#12"), None);
    let l = |s: &str| longitud(&quimera_css::tokens(s)[0]);
    assert_eq!(l("12px"), Some(Longitud::Px(12.0)));
    assert_eq!(l("12pt"), Some(Longitud::Px(16.0)));
    assert_eq!(l("1in"), Some(Longitud::Px(96.0)));
    assert_eq!(l("1.5EM"), Some(Longitud::Em(1.5)));
    assert_eq!(l("50%"), Some(Longitud::Porcentaje(50.0)));
    assert_eq!(l("0"), Some(Longitud::Px(0.0)));
    assert_eq!(l("auto"), Some(Longitud::Auto));
    assert_eq!(l("12"), None, "un número sin unidad no es una longitud");
    let v = &declaraciones("border: 1px solid rgb(1, 2, 3)")[0].valor;
    assert_eq!(piezas(v).len(), 3);
}
