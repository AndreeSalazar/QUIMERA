//! Maquetación incremental: lo que se mide en seco y los tamaños intrínsecos pasan de una
//! maquetación a la siguiente, y solo se tira lo que depende de algo que cambió.
//!
//! Medir en seco (maquetar algo solo para saber su alto: los elementos flexibles, flotantes,
//! posicionados y las celdas) es casi todo el trabajo: en YouTube, 46 000 de los 49 000 bloques de
//! cada pasada. Cada medida depende solo de lo que hay dentro del elemento y de lo que lleva su
//! clave (ancho, alto base, alto y bordes impuestos). Así que, si nada de dentro cambió, la medida
//! de la pasada anterior vale.
//!
//! Quien maqueta dice qué nodos están **sucios**: los de estilo cambiado, los que tocó JavaScript,
//! los `<img>` cuya imagen acaba de llegar... Se tiran las medidas de esos nodos y de todos sus
//! antepasados (los que los llevan dentro). Con `None` se tira todo (fuentes nuevas, otra ventana).
//! Lo de los pseudoelementos (números nuevos en cada pasada) no se guarda.

use crate::bloque::Maquetador;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use quimera_html::{Documento, NodoId};

/// (nodo, ancho, alto base, alto impuesto, bordes impuestos) → alto medido en seco.
pub(crate) type Altos = BTreeMap<(u32, u32, u32, u32, u32), f32>;

/// Lo que una maquetación deja para la siguiente.
#[derive(Debug, Default)]
pub struct MemoriaMaquetacion {
    altos: Altos,
    intrinsecos: Vec<Option<(f32, f32)>>,
    /// De la última: (medidas en seco que venían de la anterior, medidas que quedaron guardadas).
    pub ultimo: (usize, usize),
}

impl MemoriaMaquetacion {
    /// Tira lo que depende de los `sucios` y le pasa el resto al maquetador.
    pub(crate) fn prestar(&mut self, doc: &Documento, sucios: Option<&[NodoId]>, m: &mut Maquetador<'_>) {
        match sucios {
            None => {
                self.altos.clear();
                self.intrinsecos.clear();
            }
            Some(s) => {
                let tocados = con_antepasados(doc, s);
                self.altos.retain(|k, _| !tocados.contains(&k.0));
                for &i in &tocados {
                    if let Some(x) = self.intrinsecos.get_mut(i as usize) {
                        *x = None;
                    }
                }
            }
        }
        let reutilizadas = self.altos.len();
        m.altos_en_seco = core::mem::take(&mut self.altos);
        for (i, v) in self.intrinsecos.iter().enumerate() {
            if let Some(x) = m.intrinsecos.get_mut(i) {
                *x = *v;
            }
        }
        self.ultimo = (reutilizadas, 0);
    }

    /// Recoge lo medido para la siguiente (sin lo de los pseudoelementos).
    pub(crate) fn devolver(&mut self, m: &mut Maquetador<'_>, originales: usize) {
        self.altos = core::mem::take(&mut m.altos_en_seco);
        self.altos.retain(|k, _| (k.0 as usize) < originales);
        self.intrinsecos = core::mem::take(&mut m.intrinsecos);
        self.intrinsecos.truncate(originales);
        self.ultimo.1 = self.altos.len();
    }
}

/// Los nodos y todos sus antepasados.
fn con_antepasados(doc: &Documento, sucios: &[NodoId]) -> BTreeSet<u32> {
    let mut c = BTreeSet::new();
    for &d in sucios {
        let mut actual = Some(d).filter(|x| doc.existe(*x));
        while let Some(x) = actual {
            if !c.insert(x.0) {
                break; // de aquí hacia arriba ya está
            }
            actual = doc.nodo(x).padre;
        }
    }
    c
}

#[cfg(test)]
mod pruebas {
    use crate::{Aproximado, MemoriaMaquetacion, maquetar_incremental, maquetar_pagina};
    use quimera_html::{Documento, NodoId};

    const PAGINA: &str = "<!doctype html><style>
        .fila { display: flex } .fila > div { flex: 1 } .f { float: left; width: 50px }
        td { padding: 2px }
    </style><body><div class=fila><div id=a>uno dos tres</div><div id=b><p>cuatro</p></div></div>
    <div class=f>flota</div><p>al lado</p><table><tr><td id=c>celda</td><td>otra</td></tr></table>";

    fn sin_formulario() -> &'static dyn crate::control::Formulario {
        &crate::control::SinFormulario
    }

    fn una(doc: &Documento, m: &mut MemoriaMaquetacion, sucios: Option<&[NodoId]>) -> crate::Maquetacion {
        let estilos = quimera_estilo::calcular(doc, &quimera_estilo::Opciones { ancho: 800.0, alto: 600.0 });
        maquetar_incremental(
            doc,
            &estilos,
            &Aproximado,
            (800.0, 600.0),
            (&|_| None, sin_formulario()),
            m,
            (doc.total(), sucios),
        )
    }

    fn entera(doc: &Documento) -> crate::Maquetacion {
        let estilos = quimera_estilo::calcular(doc, &quimera_estilo::Opciones { ancho: 800.0, alto: 600.0 });
        maquetar_pagina(doc, &estilos, &Aproximado, (800.0, 600.0), &|_| None, sin_formulario())
    }

    #[test]
    fn el_ancla_de_dentro_de_un_inline_block_va_a_su_altura() {
        let doc = quimera_html::analizar(
            "<body style='margin:0'><div style='height:100px'></div><p style='margin:0'>a <span style='display:inline-block' id=x>b</span></p>",
        );
        let m = entera(&doc);
        let x = doc
            .descendientes(Documento::RAIZ)
            .find(|&n| doc.elemento(n).is_some_and(|e| e.atributo("id") == Some("x")))
            .unwrap();
        let caja = m.cajas.iter().find(|(n, _)| *n == x).map(|(_, c)| c[1]).unwrap();
        assert_eq!(m.anclas.iter().filter(|(i, _)| i == "x").count(), 1, "una sola ancla");
        assert_eq!(m.ancla("x"), Some(caja), "a la altura de su caja, no en (0, 0)");
        assert!(caja >= 100.0);
    }

    #[test]
    fn reutiliza_lo_que_no_cambio_y_da_lo_mismo_que_la_entera() {
        let mut doc = quimera_html::analizar(PAGINA);
        let mut m = MemoriaMaquetacion::default();
        assert_eq!(una(&doc, &mut m, None), entera(&doc));
        let guardadas = m.ultimo.1;
        assert!(guardadas > 0, "hay medidas en seco que guardar");
        // Sin cambios: todo viene de la anterior.
        assert_eq!(una(&doc, &mut m, Some(&[])), entera(&doc));
        assert_eq!(m.ultimo.0, guardadas);
        // Más texto en #a: su medida y las de sus antepasados se tiran; la de #b sigue.
        let a = doc
            .descendientes(Documento::RAIZ)
            .find(|&n| doc.elemento(n).is_some_and(|e| e.atributo("id") == Some("a")))
            .unwrap();
        let texto = doc.nodo(a).hijos[0];
        doc.poner_texto(texto, "uno dos tres cuatro cinco seis siete ocho nueve diez once doce trece catorce quince dieciséis diecisiete dieciocho diecinueve veinte veintiuno");
        assert_eq!(una(&doc, &mut m, Some(&[a])), entera(&doc), "da lo mismo que maquetar entera");
        assert!(m.ultimo.0 > 0 && m.ultimo.0 < guardadas + 10, "reutiliza parte: {:?}", m.ultimo);
    }
}
