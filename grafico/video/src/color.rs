//! De NV12 (luminancia entera y croma a media resolución, U y V entrelazados) a RGBA, con los
//! coeficientes de BT.709 en rango limitado (16–235), los del vídeo de la web en HD.

use alloc::vec::Vec;

/// `datos`: el plano Y (`paso` × `alto_plano`) seguido del UV. `visible`: (x, y, ancho, alto).
pub fn nv12_a_rgba(datos: &[u8], paso: usize, alto_plano: usize, visible: (u32, u32, u32, u32)) -> Vec<u8> {
    let (x0, y0, ancho, alto) = (visible.0 as usize, visible.1 as usize, visible.2 as usize, visible.3 as usize);
    let mut rgba = alloc::vec![0u8; ancho * alto * 4];
    let uv = paso * alto_plano;
    for y in 0..alto {
        let fila_y = (y0 + y) * paso;
        let fila_uv = uv + ((y0 + y) / 2) * paso;
        for x in 0..ancho {
            let px = x0 + x;
            let luz = i32::from(*datos.get(fila_y + px).unwrap_or(&16)) - 16;
            let k = fila_uv + (px & !1);
            let u = i32::from(*datos.get(k).unwrap_or(&128)) - 128;
            let v = i32::from(*datos.get(k + 1).unwrap_or(&128)) - 128;
            // En coma fija (×256): R = 1,164·Y + 1,793·V; G = 1,164·Y − 0,213·U − 0,533·V; B = 1,164·Y + 2,112·U.
            let c = 298 * luz + 128;
            let o = (y * ancho + x) * 4;
            rgba[o] = ((c + 459 * v) >> 8).clamp(0, 255) as u8;
            rgba[o + 1] = ((c - 55 * u - 136 * v) >> 8).clamp(0, 255) as u8;
            rgba[o + 2] = ((c + 541 * u) >> 8).clamp(0, 255) as u8;
            rgba[o + 3] = 255;
        }
    }
    rgba
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn negro_blanco_y_rojo() {
        // 2 × 2 píxeles: Y = 16 (negro), 235 (blanco); un solo par UV.
        let gris = [16, 235, 16, 235, 128, 128];
        let r = nv12_a_rgba(&gris, 2, 2, (0, 0, 2, 2));
        assert_eq!(&r[0..4], &[0, 0, 0, 255]);
        assert_eq!(&r[4..8], &[255, 255, 255, 255]);
        // Rojo de BT.709: Y 63, U 102, V 240.
        let rojo = [63, 63, 63, 63, 102, 240];
        let r = nv12_a_rgba(&rojo, 2, 2, (0, 0, 1, 1));
        assert!(r[0] > 250 && r[1] < 5 && r[2] < 5, "{r:?}");
    }
}
