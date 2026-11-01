//! Guardar una imagen RGBA como PNG, sin dependencias: sin comprimir (zlib en bloques
//! «almacenados»), que todos los visores leen. Para capturas de prueba basta.

fn crc32(datos: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in datos {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(datos: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in datos {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn trozo(salida: &mut Vec<u8>, tipo: &[u8; 4], datos: &[u8]) {
    salida.extend_from_slice(&(datos.len() as u32).to_be_bytes());
    let inicio = salida.len();
    salida.extend_from_slice(tipo);
    salida.extend_from_slice(datos);
    let crc = crc32(&salida[inicio..]);
    salida.extend_from_slice(&crc.to_be_bytes());
}

/// Un PNG con `rgba` (`ancho × alto × 4` bytes, fila a fila).
pub fn codificar(ancho: u32, alto: u32, rgba: &[u8]) -> Vec<u8> {
    let mut filas = Vec::with_capacity(rgba.len() + alto as usize);
    for fila in rgba.chunks(ancho as usize * 4) {
        filas.push(0); // sin filtro
        filas.extend_from_slice(fila);
    }
    let mut zlib = vec![0x78, 0x01];
    let bloques: Vec<&[u8]> = filas.chunks(65_535).collect();
    for (i, bloque) in bloques.iter().enumerate() {
        zlib.push(u8::from(i + 1 == bloques.len()));
        let largo = bloque.len() as u16;
        zlib.extend_from_slice(&largo.to_le_bytes());
        zlib.extend_from_slice(&(!largo).to_le_bytes());
        zlib.extend_from_slice(bloque);
    }
    zlib.extend_from_slice(&adler32(&filas).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut cabecera = Vec::new();
    cabecera.extend_from_slice(&ancho.to_be_bytes());
    cabecera.extend_from_slice(&alto.to_be_bytes());
    cabecera.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA
    trozo(&mut png, b"IHDR", &cabecera);
    trozo(&mut png, b"IDAT", &zlib);
    trozo(&mut png, b"IEND", &[]);
    png
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn crc_y_adler_conocidos() {
        assert_eq!(super::crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(super::adler32(b"Wikipedia"), 0x11E6_0398);
        let png = super::codificar(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(&png[1..4], b"PNG");
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
    }
}
