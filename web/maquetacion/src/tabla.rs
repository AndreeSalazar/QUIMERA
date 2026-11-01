//! Tablas (CSS 2.1 §17) con el algoritmo automático, como lo hacen los navegadores:
//!
//! 1. **Rejilla:** las filas de `<thead>`, luego las de `<tbody>` (y las sueltas) y al final las de
//!    `<tfoot>`; cada celda ocupa `colspan × rowspan` casillas libres.
//! 2. **Columnas:** cada una mide como mínimo la celda más estrecha posible y como máximo la
//!    más ancha sin partir líneas (`width` en una celda la fija; un `%`, la reparte). Las celdas
//!    de varias columnas reparten lo que les falte entre ellas.
//! 3. **Ancho de la tabla:** el pedido (`width`, que en una tabla incluye borde y relleno), o el
//!    del contenedor sin pasar del máximo de las columnas; nunca menos que su mínimo. Lo que hay
//!    entre el mínimo y el máximo se reparte en proporción.
//! 4. **Filas:** cada una tan alta como su celda más alta; una celda de varias filas alarga la
//!    última. Cada celda se maqueta como un bloque del ancho de sus columnas y del alto de sus
//!    filas, con el contenido arriba, en medio o abajo (`vertical-align`).
//!
//! `border-spacing` separa las celdas. Con `border-collapse: collapse` no hay espacio: cada borde
//! entre dos celdas es el más ancho de los dos (y en los de fuera, también el de la tabla), cada
//! celda ocupa la mitad de sus bordes y la tabla la mitad de los de fuera, y la tabla los dibuja
//! centrados en las líneas. Es como lo hace Chromium.

use crate::bloque::{Hecho, Maquetador};
use crate::pintura::Pieza;
use alloc::vec::Vec;
use quimera_estilo::{AlinearVertical, Color, Display, Estilo, Medida};
use quimera_html::{Dato, NodoId};

/// Una fila: su `<tr>` (`None` si es anónima), su grupo y sus celdas.
struct Fila {
    tr: Option<NodoId>,
    grupo: Option<NodoId>,
    celdas: Vec<NodoId>,
}

/// Una celda en la rejilla.
struct Celda {
    id: NodoId,
    fila: usize,
    col: usize,
    filas: usize,
    cols: usize,
}

/// La tabla ordenada en casillas.
struct Rejilla {
    titulos: Vec<NodoId>,
    filas: Vec<Fila>,
    celdas: Vec<Celda>,
    columnas: usize,
}

/// Lo que mide cada columna.
struct Columnas {
    min: Vec<f32>,
    max: Vec<f32>,
    fijo: Vec<bool>,
    pct: Vec<Option<f32>>,
}

/// Lo que hay entre las columnas (o filas): `n + 1` huecos, del borde izquierdo al derecho.
type Huecos = Vec<f32>;

/// Un borde resuelto: ancho y color.
type Lado = (f32, Color);

/// Con bordes que colapsan, los cuatro bordes de cada celda (arriba, derecha, abajo, izquierda).
type Colapsados = Vec<[Lado; 4]>;

fn lado(e: &Estilo, i: usize) -> Lado {
    (e.borde[i].ocupa(), e.color_borde(i))
}

/// Gana el más ancho; si empatan, el que ya estaba (el de la propia celda).
fn mayor(a: Lado, b: Lado) -> Lado {
    if b.0 > a.0 { b } else { a }
}

/// La mitad de cada borde: lo que ocupa una celda con bordes que colapsan.
fn mitades(b: &[Lado; 4]) -> [f32; 4] {
    [b[0].0 / 2.0, b[1].0 / 2.0, b[2].0 / 2.0, b[3].0 / 2.0]
}

/// Lo que ocupan en total varias columnas (o filas) seguidas, con los huecos entre ellas.
fn tramo(tam: &[f32], huecos: &[f32], desde: usize, cuantas: usize) -> f32 {
    let hasta = (desde + cuantas).min(tam.len());
    tam[desde..hasta].iter().sum::<f32>() + huecos[desde + 1..hasta].iter().sum::<f32>()
}

/// Dónde empieza cada columna (o fila) a partir de `origen`.
fn posiciones(origen: f32, tam: &[f32], huecos: &[f32]) -> Vec<f32> {
    let mut v = Vec::with_capacity(tam.len());
    let mut p = origen;
    for (i, t) in tam.iter().enumerate() {
        p += huecos[i];
        v.push(p);
        p += t;
    }
    v
}

/// Reparte `exceso` entre `v` en proporción a `peso` (a partes iguales si no pesa nada).
fn repartir(v: &mut [f32], peso: &[f32], exceso: f32) {
    let total: f32 = peso.iter().sum();
    let n = v.len().max(1) as f32;
    for (x, p) in v.iter_mut().zip(peso) {
        *x += if total > 0.0 { exceso * p / total } else { exceso / n };
    }
}

/// El ancho de cada columna para que entre todas sumen `objetivo`.
fn anchos(c: &Columnas, objetivo: f32) -> Vec<f32> {
    let n = c.min.len();
    let suma_min: f32 = c.min.iter().sum();
    if objetivo <= suma_min || n == 0 {
        return c.min.clone();
    }
    // Primero los porcentajes, del ancho de la tabla.
    let mut w: Vec<f32> = (0..n).map(|i| c.pct[i].map_or(0.0, |p| (objetivo * p / 100.0).max(c.min[i]))).collect();
    let usado: f32 = w.iter().sum();
    let resto: Vec<usize> = (0..n).filter(|&i| c.pct[i].is_none()).collect();
    let disponible = (objetivo - usado).max(resto.iter().map(|&i| c.min[i]).sum());
    let (min, max): (Vec<f32>, Vec<f32>) = resto.iter().map(|&i| (c.min[i], c.max[i])).unzip();
    let (suma_min, suma_max): (f32, f32) = (min.iter().sum(), max.iter().sum());
    let mut r: Vec<f32> = if disponible >= suma_max {
        let mut r = max.clone();
        // Lo que sobra va a las columnas automáticas; si no hay, a las fijas.
        let auto: Vec<bool> = resto.iter().map(|&i| !c.fijo[i]).collect();
        let hay_auto = auto.iter().any(|a| *a);
        let peso: Vec<f32> =
            resto.iter().zip(&auto).map(|(&i, &a)| if a == hay_auto { c.max[i].max(0.0) } else { 0.0 }).collect();
        let peso = if peso.iter().all(|p| *p == 0.0) {
            auto.iter().map(|&a| if a == hay_auto { 1.0 } else { 0.0 }).collect()
        } else {
            peso
        };
        repartir(&mut r, &peso, disponible - suma_max);
        r
    } else {
        let k = if suma_max > suma_min { (disponible - suma_min) / (suma_max - suma_min) } else { 0.0 };
        min.iter().zip(&max).map(|(a, b)| a + (b - a) * k).collect()
    };
    if resto.is_empty() && objetivo > usado {
        // Todo son porcentajes que no llegan: lo que falta, en proporción.
        let peso = w.clone();
        repartir(&mut w, &peso, objetivo - usado);
    }
    for (k, &i) in resto.iter().enumerate() {
        w[i] = r[k];
    }
    r.clear();
    w
}

fn numero_atributo(m: &Maquetador<'_>, id: NodoId, nombre: &str) -> Option<usize> {
    m.doc.elemento(id).and_then(|e| e.atributo(nombre)).and_then(|v| v.trim().parse::<usize>().ok())
}

impl<'a> Maquetador<'a> {
    fn display_de(&self, id: NodoId) -> Option<Display> {
        match self.doc.nodo(id).dato {
            Dato::Elemento(_) => self.estilos.de(id).map(|e| e.display),
            _ => None,
        }
    }

    /// Las celdas de una fila (todo hijo que se dibuja cuenta como celda).
    fn celdas_de(&self, tr: NodoId) -> Vec<NodoId> {
        self.doc
            .nodo(tr)
            .hijos
            .iter()
            .copied()
            .filter(|&h| !matches!(self.display_de(h), None | Some(Display::Ninguno | Display::Columna)))
            .collect()
    }

    /// Las filas de un grupo (o de la tabla); las celdas sueltas forman filas anónimas.
    fn filas_de(&self, padre: NodoId, grupo: Option<NodoId>, filas: &mut Vec<Fila>) {
        let mut anonima: Option<Fila> = None;
        for &h in &self.doc.nodo(padre).hijos {
            match self.display_de(h) {
                None | Some(Display::Ninguno | Display::Columna | Display::Titulo | Display::GrupoFilas) => {}
                Some(Display::Fila) => {
                    filas.extend(anonima.take());
                    filas.push(Fila { tr: Some(h), grupo, celdas: self.celdas_de(h) });
                }
                Some(_) => anonima.get_or_insert_with(|| Fila { tr: None, grupo, celdas: Vec::new() }).celdas.push(h),
            }
        }
        filas.extend(anonima);
    }

    /// La rejilla de la tabla: filas en su orden y cada celda en sus casillas.
    fn rejilla(&self, tabla: NodoId) -> Rejilla {
        let (mut cabeza, mut cuerpo, mut pie, mut titulos) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        // Las filas sueltas de la tabla van con los cuerpos, en su orden.
        for &h in &self.doc.nodo(tabla).hijos {
            match self.display_de(h) {
                Some(Display::Titulo) => titulos.push(h),
                Some(Display::GrupoFilas) => match self.doc.elemento(h).map(|e| e.nombre.as_str()) {
                    Some("thead") => self.filas_de(h, Some(h), &mut cabeza),
                    Some("tfoot") => self.filas_de(h, Some(h), &mut pie),
                    _ => self.filas_de(h, Some(h), &mut cuerpo),
                },
                _ => {}
            }
        }
        let mut sueltas = Vec::new();
        self.filas_de(tabla, None, &mut sueltas);
        cuerpo.extend(sueltas);
        let filas: Vec<Fila> = cabeza.into_iter().chain(cuerpo).chain(pie).collect();
        let (celdas, columnas) = self.colocar_celdas(&filas);
        Rejilla { titulos, filas, celdas, columnas }
    }

    /// Cada celda en la primera casilla libre de su fila, con su `colspan` y `rowspan`.
    fn colocar_celdas(&self, filas: &[Fila]) -> (Vec<Celda>, usize) {
        let mut ocupado: Vec<Vec<bool>> = (0..filas.len()).map(|_| Vec::new()).collect();
        let mut celdas = Vec::new();
        let mut columnas = 0;
        for (r, f) in filas.iter().enumerate() {
            let mut c = 0;
            for &id in &f.celdas {
                while ocupado[r].get(c).copied().unwrap_or(false) {
                    c += 1;
                }
                let cols = numero_atributo(self, id, "colspan").filter(|n| *n > 0).unwrap_or(1).min(1000);
                let quedan = filas.len() - r;
                let n = match numero_atributo(self, id, "rowspan") {
                    Some(0) => quedan,
                    Some(n) => n.min(quedan),
                    None => 1,
                };
                for fila in ocupado.iter_mut().skip(r).take(n) {
                    if fila.len() < c + cols {
                        fila.resize(c + cols, false);
                    }
                    fila[c..c + cols].fill(true);
                }
                celdas.push(Celda { id, fila: r, col: c, filas: n, cols });
                columnas = columnas.max(c + cols);
                c += cols;
            }
        }
        (celdas, columnas)
    }

    /// Los huecos entre columnas (`columnas`) o filas: el espaciado, o nada si los bordes colapsan.
    fn huecos(rj: &Rejilla, e: &Estilo, columnas: bool) -> Huecos {
        let n = if columnas { rj.columnas } else { rj.filas.len() };
        let espacio = match (e.tabla.colapsar, columnas) {
            (true, _) => 0.0,
            (false, true) => e.tabla.espaciado.0,
            (false, false) => e.tabla.espaciado.1,
        };
        alloc::vec![espacio; n + 1]
    }

    /// Con `border-collapse`, cada borde de cada celda: el más ancho entre el suyo, el de las
    /// celdas vecinas y, en los de fuera, el de la tabla.
    fn bordes_colapsados(&self, rj: &Rejilla, e: &Estilo) -> Colapsados {
        let (nf, nc) = (rj.filas.len(), rj.columnas);
        let mut dueno = alloc::vec![usize::MAX; nf * nc];
        for (k, c) in rj.celdas.iter().enumerate() {
            for f in c.fila..c.fila + c.filas {
                dueno[f * nc + c.col..f * nc + c.col + c.cols].fill(k);
            }
        }
        let estilo = |k: usize| self.estilos.de(rj.celdas[k].id);
        let vecino = |f: usize, col: usize, k: usize| {
            dueno.get(f * nc + col).copied().filter(|&v| v != usize::MAX && v != k).and_then(estilo)
        };
        let mut todos = Vec::with_capacity(rj.celdas.len());
        for (k, c) in rj.celdas.iter().enumerate() {
            let Some(ec) = estilo(k) else {
                todos.push([(0.0, Color::NEGRO); 4]);
                continue;
            };
            let mut b = [lado(ec, 0), lado(ec, 1), lado(ec, 2), lado(ec, 3)];
            let (fin_f, fin_c) = (c.fila + c.filas, c.col + c.cols);
            let cols = c.col..fin_c;
            let filas = c.fila..fin_f;
            b[0] = if c.fila == 0 {
                mayor(b[0], lado(e, 0))
            } else {
                cols.clone().filter_map(|col| vecino(c.fila - 1, col, k)).fold(b[0], |a, v| mayor(a, lado(v, 2)))
            };
            b[2] = if fin_f >= nf {
                mayor(b[2], lado(e, 2))
            } else {
                cols.filter_map(|col| vecino(fin_f, col, k)).fold(b[2], |a, v| mayor(a, lado(v, 0)))
            };
            b[3] = if c.col == 0 {
                mayor(b[3], lado(e, 3))
            } else {
                filas.clone().filter_map(|f| vecino(f, c.col - 1, k)).fold(b[3], |a, v| mayor(a, lado(v, 1)))
            };
            b[1] = if fin_c >= nc {
                mayor(b[1], lado(e, 1))
            } else {
                filas.filter_map(|f| vecino(f, fin_c, k)).fold(b[1], |a, v| mayor(a, lado(v, 3)))
            };
            todos.push(b);
        }
        todos
    }

    /// (mínimo, máximo) de una celda, y su ancho si lo tiene fijado.
    fn anchos_celda(&mut self, id: NodoId) -> (f32, f32, Option<f32>) {
        let (a, b) = self.intrinseco(id);
        match self.estilos.de(id).map(|e| e.ancho) {
            Some(Medida::Px(_)) => {
                let w = self.minimo_contenido(id).max(b);
                (w, w, Some(w))
            }
            _ => (a, b, None),
        }
    }

    /// Lo que mide cada columna, según sus celdas (primero las de una sola columna).
    fn columnas(&mut self, rj: &Rejilla, huecos: &[f32], colapsados: Option<&Colapsados>) -> Columnas {
        let n = rj.columnas;
        let mut c = Columnas {
            min: alloc::vec![0.0; n],
            max: alloc::vec![0.0; n],
            fijo: alloc::vec![false; n],
            pct: alloc::vec![None; n],
        };
        let mut orden: Vec<usize> = (0..rj.celdas.len()).collect();
        orden.sort_by_key(|&k| rj.celdas[k].cols);
        for k in orden {
            let x = &rj.celdas[k];
            let (mut min, mut max, fijo) = self.anchos_celda(x.id);
            if let (Some(cb), Some(ec)) = (colapsados, self.estilos.de(x.id)) {
                let ajuste = (cb[k][1].0 + cb[k][3].0) / 2.0 - ec.borde[1].ocupa() - ec.borde[3].ocupa();
                (min, max) = ((min + ajuste).max(0.0), (max + ajuste).max(0.0));
            }
            let pct = match self.estilos.de(x.id).map(|e| e.ancho) {
                Some(Medida::Porcentaje(p)) => Some(p),
                _ => None,
            };
            if x.cols == 1 {
                let i = x.col;
                c.min[i] = c.min[i].max(min);
                c.max[i] = c.max[i].max(max).max(c.min[i]);
                c.fijo[i] |= fijo.is_some();
                if let Some(p) = pct {
                    c.pct[i] = Some(c.pct[i].map_or(p, |q: f32| q.max(p)));
                }
                continue;
            }
            let r = x.col..x.col + x.cols;
            let peso: Vec<f32> = c.max[r.clone()].to_vec();
            let falta_min = min - tramo(&c.min, huecos, x.col, x.cols);
            if falta_min > 0.0 {
                repartir(&mut c.min[r.clone()], &peso, falta_min);
            }
            let falta_max = max - tramo(&c.max, huecos, x.col, x.cols);
            if falta_max > 0.0 {
                repartir(&mut c.max[r.clone()], &peso, falta_max);
            }
            for i in r {
                c.max[i] = c.max[i].max(c.min[i]);
            }
        }
        c
    }

    /// Relleno y borde de la tabla (izquierda, derecha, arriba, abajo). Con bordes que colapsan
    /// no hay relleno, y la tabla ocupa la mitad del borde más ancho de cada lado de fuera.
    fn extras_tabla(rj: &Rejilla, e: &Estilo, contenedor: f32, colapsados: Option<&Colapsados>) -> [f32; 4] {
        let Some(cb) = colapsados else {
            let px = |m: Medida| m.px(contenedor, 0.0);
            return [
                px(e.relleno[3]) + e.borde[3].ocupa(),
                px(e.relleno[1]) + e.borde[1].ocupa(),
                px(e.relleno[0]) + e.borde[0].ocupa(),
                px(e.relleno[2]) + e.borde[2].ocupa(),
            ];
        };
        let (nf, nc) = (rj.filas.len(), rj.columnas);
        let mut m = [e.borde[3].ocupa(), e.borde[1].ocupa(), e.borde[0].ocupa(), e.borde[2].ocupa()];
        if !rj.celdas.is_empty() {
            m = [0.0; 4];
        }
        for (c, b) in rj.celdas.iter().zip(cb) {
            let de_fuera = [c.col == 0, c.col + c.cols >= nc, c.fila == 0, c.fila + c.filas >= nf];
            for (i, lado) in [3, 1, 0, 2].into_iter().enumerate() {
                if de_fuera[i] {
                    m[i] = m[i].max(b[lado].0);
                }
            }
        }
        m.map(|x| x / 2.0)
    }

    /// (mínimo, máximo) de una tabla: sus columnas, huecos, relleno y borde.
    pub(crate) fn intrinseco_tabla(&mut self, id: NodoId, e: &Estilo) -> (f32, f32) {
        let rj = self.rejilla(id);
        let huecos = Self::huecos(&rj, e, true);
        let colapsados = e.tabla.colapsar.then(|| self.bordes_colapsados(&rj, e));
        let c = self.columnas(&rj, &huecos, colapsados.as_ref());
        let [l, r, _, _] = Self::extras_tabla(&rj, e, 0.0, colapsados.as_ref());
        let fuera = huecos.iter().sum::<f32>() + l + r;
        let (min, max) = (c.min.iter().sum::<f32>() + fuera, c.max.iter().sum::<f32>() + fuera);
        match e.ancho {
            Medida::Px(w) => (w.max(min), w.max(min)),
            _ => (min, max.max(min)),
        }
    }

    /// Maqueta la tabla `id` con su caja empezando en (x, y) dentro de `contenedor` píxeles.
    pub(crate) fn tabla(&mut self, id: NodoId, e: &'a Estilo, (x, y): (f32, f32), contenedor: f32) -> Hecho {
        let rj = self.rejilla(id);
        let (hc, hf) = (Self::huecos(&rj, e, true), Self::huecos(&rj, e, false));
        let colapsados = e.tabla.colapsar.then(|| self.bordes_colapsados(&rj, e));
        let cb = colapsados.as_ref();
        let cols = self.columnas(&rj, &hc, cb);
        let [l, r, t, b] = Self::extras_tabla(&rj, e, contenedor, cb);
        let px = |m: Medida| m.px(contenedor, 0.0);
        let (ml, mr) = (px(e.margen[3]), px(e.margen[1]));
        let espacio: f32 = hc.iter().sum();
        let (suma_min, suma_max) = (cols.min.iter().sum::<f32>() + espacio, cols.max.iter().sum::<f32>() + espacio);
        let pedido = match e.ancho {
            _ if self.ancho_libre == Some(id) => Some(contenedor - ml - mr),
            Medida::Px(w) => Some(w),
            Medida::Porcentaje(p) => Some(contenedor * p / 100.0),
            Medida::Auto => None,
        };
        let ancho_rejilla = match pedido {
            Some(w) => (w - l - r).max(suma_min),
            None => (contenedor - ml - mr - l - r).min(suma_max).max(suma_min),
        };
        let w = anchos(&cols, ancho_rejilla - espacio);
        let ancho_borde = ancho_rejilla + l + r;
        let izquierda = match (e.margen[3] == Medida::Auto, e.margen[1] == Medida::Auto) {
            (true, true) => ((contenedor - ancho_borde) / 2.0).max(0.0),
            (true, false) => (contenedor - ancho_borde - mr).max(0.0),
            _ if self.padre_centra(id) && self.ancho_libre != Some(id) => {
                ml + ((contenedor - ml - mr - ancho_borde) / 2.0).max(0.0)
            }
            _ => ml,
        };
        let tx = x + izquierda;
        if let Some(i) = self.doc.elemento(id).and_then(|el| el.atributo("id")) {
            self.anclas.push((alloc::string::String::from(i), y));
        }
        let mut cy = y;
        for &titulo in &rj.titulos {
            cy += self.bloque(titulo, tx, cy, ancho_borde).alto;
        }
        let inicio = self.piezas.len();
        let (alturas, medidas) = self.alturas(&rj, (&w, &hc), &hf, e, cb);
        let alto_rejilla = alturas.iter().sum::<f32>() + hf.iter().sum::<f32>();
        self.poner_celdas(&rj, (&w, &hc), (&alturas, &hf), (tx + l, cy + t), (&medidas, cb));
        let alto_caja = alto_rejilla + t + b;
        if cb.is_some() {
            // Los bordes los dibujan las celdas, centrados en las líneas: aquí solo el fondo.
            if e.visible && e.fondo.a > 0 {
                let fondo = Pieza::Rect { x: tx, y: cy, ancho: ancho_borde, alto: alto_caja, color: e.fondo };
                self.piezas.insert(inicio, fondo);
            }
        } else {
            self.fondo_y_bordes(inicio, e, tx, cy, ancho_borde, alto_caja);
        }
        Hecho {
            alto: cy - y + alto_caja,
            margen_inferior: px(e.margen[2]),
            primera_base: None,
            caja: (tx, ancho_borde),
        }
    }

    /// El alto de cada fila y el que necesita cada celda (medida sin dibujar).
    fn alturas(
        &mut self,
        rj: &Rejilla,
        (w, hc): (&[f32], &[f32]),
        hf: &[f32],
        e: &Estilo,
        cb: Option<&Colapsados>,
    ) -> (Vec<f32>, Vec<f32>) {
        let medidas: Vec<f32> = rj
            .celdas
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let ancho = tramo(w, hc, c.col, c.cols);
                let previo = core::mem::replace(&mut self.bordes_celda, cb.map(|cb| (c.id, mitades(&cb[k]))));
                let alto = self.en_seco(|m| m.bloque_libre(c.id, 0.0, 0.0, ancho).alto);
                self.bordes_celda = previo;
                alto
            })
            .collect();
        let mut alto: Vec<f32> = rj
            .filas
            .iter()
            .map(|f| match f.tr.and_then(|tr| self.estilos.de(tr)).map(|e| e.alto) {
                Some(Medida::Px(h)) => h,
                _ => 0.0,
            })
            .collect();
        for (c, h) in rj.celdas.iter().zip(&medidas).filter(|(c, _)| c.filas == 1) {
            alto[c.fila] = alto[c.fila].max(*h);
        }
        for (c, h) in rj.celdas.iter().zip(&medidas).filter(|(c, _)| c.filas > 1) {
            let falta = h - tramo(&alto, hf, c.fila, c.filas);
            if falta > 0.0 {
                alto[c.fila + c.filas - 1] += falta;
            }
        }
        // `height` en la tabla: lo que sobre se reparte entre las filas.
        if let Medida::Px(pedido) = e.alto {
            let [_, _, t, b] = Self::extras_tabla(rj, e, 0.0, cb);
            let sobra = pedido - t - b - alto.iter().sum::<f32>() - hf.iter().sum::<f32>();
            if sobra > 0.0 && !alto.is_empty() {
                let n = alto.len() as f32;
                alto.iter_mut().for_each(|a| *a += sobra / n);
            }
        }
        (alto, medidas)
    }

    /// Dibuja cada celda en su sitio: debajo, el fondo de su grupo y de su fila.
    fn poner_celdas(
        &mut self,
        rj: &Rejilla,
        (w, hc): (&[f32], &[f32]),
        (alto, hf): (&[f32], &[f32]),
        (gx, gy): (f32, f32),
        (medidas, cb): (&[f32], Option<&Colapsados>),
    ) {
        let (xs, ys) = (posiciones(gx, w, hc), posiciones(gy, alto, hf));
        let mut lineas = Vec::new();
        for (k, (c, medida)) in rj.celdas.iter().zip(medidas).enumerate() {
            let (cx, cy) = (xs[c.col], ys[c.fila]);
            let (ancho, alto_celda) = (tramo(w, hc, c.col, c.cols), tramo(alto, hf, c.fila, c.filas));
            let fila = &rj.filas[c.fila];
            for dueno in [fila.grupo, fila.tr].into_iter().flatten() {
                if let Some(ef) = self.estilos.de(dueno).filter(|ef| ef.fondo.a > 0 && ef.visible) {
                    self.piezas.push(Pieza::Rect { x: cx, y: cy, ancho, alto: alto_celda, color: ef.fondo });
                }
            }
            let (p0, e0) = (self.piezas.len(), self.enlaces.len());
            let previo = self.alto_forzado.replace((c.id, alto_celda));
            let previos = core::mem::replace(&mut self.bordes_celda, cb.map(|cb| (c.id, mitades(&cb[k]))));
            self.bloque_libre(c.id, cx, cy, ancho);
            (self.alto_forzado, self.bordes_celda) = (previo, previos);
            if let Some(cb) = cb {
                lineas.extend(lineas_de(&cb[k], (cx, cy, ancho, alto_celda)));
            }
            let Some(ec) = self.estilos.de(c.id) else { continue };
            let sobra = (alto_celda - medida).max(0.0);
            let bajar = match ec.tabla.vertical {
                AlinearVertical::Medio => sobra / 2.0,
                AlinearVertical::Abajo => sobra,
                _ => 0.0,
            };
            if bajar > 0.0 {
                let fondo = if cb.is_some() {
                    usize::from(ec.visible && ec.fondo.a > 0)
                } else {
                    piezas_de_fondo(ec, ancho, alto_celda)
                };
                for p in self.piezas.iter_mut().skip(p0 + fondo) {
                    p.bajar(bajar);
                }
                for en in self.enlaces.iter_mut().skip(e0) {
                    en.1 += bajar;
                }
            }
        }
        // Los bordes que colapsan, encima de todas las celdas (no los tapa el fondo de la vecina).
        self.piezas.extend(lineas);
    }
}

/// Los cuatro bordes de una celda con bordes que colapsan, centrados en el borde de su caja.
fn lineas_de(b: &[Lado; 4], (x, y, ancho, alto): (f32, f32, f32, f32)) -> Vec<Pieza> {
    let [(t, ct), (r, cr), (bo, cbo), (l, cl)] = *b;
    let (x0, y0, x1, y1) = (x - l / 2.0, y - t / 2.0, x + ancho + r / 2.0, y + alto + bo / 2.0);
    [
        (x0, y0, x1 - x0, t, ct),
        (x + ancho - r / 2.0, y0, r, y1 - y0, cr),
        (x0, y + alto - bo / 2.0, x1 - x0, bo, cbo),
        (x0, y0, l, y1 - y0, cl),
    ]
    .into_iter()
    .filter(|&(_, _, w, h, _)| w > 0.0 && h > 0.0)
    .map(|(x, y, ancho, alto, color)| Pieza::Rect { x, y, ancho, alto, color })
    .collect()
}

/// Cuántas piezas de fondo y borde pone `fondo_y_bordes` para una caja (van antes del contenido).
fn piezas_de_fondo(e: &Estilo, ancho: f32, alto: f32) -> usize {
    if !e.visible {
        return 0;
    }
    let lados = (0..4).filter(|&i| e.borde[i].ocupa() > 0.0 && if i % 2 == 0 { ancho > 0.0 } else { alto > 0.0 });
    usize::from(e.fondo.a > 0) + lados.count()
}
