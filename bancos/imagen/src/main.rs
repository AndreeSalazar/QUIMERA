//! quimera-imagen contra los decodificadores de referencia en Rust: `png` 0.17, `jpeg-decoder`
//! 0.3 y `gif` 0.13. Mismos píxeles (PNG y GIF exactos; JPEG con la diferencia máxima, porque
//! cada decodificador redondea el color a su manera) y velocidad.
//!
//! Imágenes: las reales del sistema (fondos de Windows y temas del teclado, JPEG secuenciales y
//! progresivos; las PNG de la documentación de Rust) y otras generadas aquí con codificadores de
//! referencia para cubrir lo que no aparece (PNG de 1 a 16 bits y paleta, JPEG con cada
//! submuestreo, progresivo y reinicios, GIF con transparencia).
//!
//! Uso: `cargo run --release -p quimera-banco-imagen`

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// RGBA de una imagen con el decodificador de referencia que toca.
fn referencia(datos: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    match quimera_imagen::formato(datos) {
        Some(quimera_imagen::Formato::Png) => ref_png(datos),
        Some(quimera_imagen::Formato::Jpeg) => ref_jpeg(datos),
        Some(quimera_imagen::Formato::Gif) => ref_gif(datos),
        otro => Err(format!("sin referencia para {otro:?}")),
    }
}

fn ref_png(d: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(d);
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut lector = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; lector.output_buffer_size()];
    let info = lector.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let rgba = match info.color_type {
        png::ColorType::Grayscale => buf.iter().take((w * h) as usize).flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            buf.chunks(2).take((w * h) as usize).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
        }
        png::ColorType::Rgb => buf.chunks(3).take((w * h) as usize).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => buf[..(w * h * 4) as usize].to_vec(),
    };
    Ok((w, h, rgba))
}

fn ref_jpeg(d: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = jpeg_decoder::Decoder::new(d);
    let pix = dec.decode().map_err(|e| e.to_string())?;
    let info = dec.info().ok_or("sin info")?;
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => pix.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        jpeg_decoder::PixelFormat::RGB24 => pix.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        jpeg_decoder::PixelFormat::CMYK32 => pix
            .chunks(4)
            .flat_map(|p| {
                let k = u16::from(p[3]);
                [
                    (u16::from(p[0]) * k / 255) as u8,
                    (u16::from(p[1]) * k / 255) as u8,
                    (u16::from(p[2]) * k / 255) as u8,
                    255,
                ]
            })
            .collect(),
        _ => return Err(String::from("formato de píxel sin comparar")),
    };
    Ok((u32::from(info.width), u32::from(info.height), rgba))
}

fn ref_gif(d: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut opciones = gif::DecodeOptions::new();
    opciones.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opciones.read_info(d).map_err(|e| e.to_string())?;
    let (w, h) = (u32::from(dec.width()), u32::from(dec.height()));
    let marco = dec.read_next_frame().map_err(|e| e.to_string())?.ok_or("sin fotogramas")?;
    let mut lienzo = vec![0u8; (w * h * 4) as usize];
    for y in 0..u32::from(marco.height) {
        for x in 0..u32::from(marco.width) {
            let (px, py) = (u32::from(marco.left) + x, u32::from(marco.top) + y);
            let o = ((y * u32::from(marco.width) + x) * 4) as usize;
            if px < w && py < h && marco.buffer[o + 3] != 0 {
                let i = ((py * w + px) * 4) as usize;
                lienzo[i..i + 4].copy_from_slice(&marco.buffer[o..o + 4]);
            }
        }
    }
    Ok((w, h, lienzo))
}

/// Un caso: su nombre y sus bytes.
type Caso = (String, Vec<u8>);

fn reales() -> Vec<Caso> {
    let mut rutas = Vec::new();
    let casa = std::env::var("USERPROFILE").unwrap_or_default();
    let docs = Path::new(&casa).join(".rustup/toolchains/stable-x86_64-pc-windows-msvc/share/doc/rust/html");
    for raiz in [PathBuf::from("C:/Windows/Web"), docs] {
        recorrer(&raiz, &mut rutas);
    }
    rutas.sort();
    rutas.into_iter().filter_map(|p| std::fs::read(&p).ok().map(|d| (p.display().to_string(), d))).collect()
}

fn recorrer(dir: &Path, v: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(dir) else { return };
    for e in entradas.flatten() {
        let p = e.path();
        let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if p.is_dir() {
            recorrer(&p, v);
        } else if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif") {
            v.push(p);
        }
    }
}

/// Un dibujo de prueba: degradados, bordes nítidos y ruido (lo difícil para los compresores).
fn dibujo(w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    let mut semilla = 12345u32;
    for y in 0..h {
        for x in 0..w {
            semilla = semilla.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let ruido = (semilla >> 24) as u8 / 8;
            let borde = if (x / 16 + y / 16) % 2 == 0 { 40 } else { 0 };
            v.extend_from_slice(&[
                (x * 255 / w) as u8 ^ ruido,
                ((y * 255 / h) as u8).wrapping_add(borde),
                ((x + y) % 256) as u8,
                (255 - x % 200) as u8,
            ]);
        }
    }
    v
}

fn png_generado(w: u32, h: u32, color: png::ColorType, bits: png::BitDepth) -> Vec<u8> {
    let rgba = dibujo(w, h);
    let profundidad = bits as u8;
    let mut datos = Vec::new();
    let maximo = (1u32 << profundidad) - 1;
    let mut paleta = Vec::new();
    for p in rgba.chunks(4) {
        let escalar = |v: u8| (u32::from(v) * maximo / 255) as u16;
        let muestras: Vec<u16> = match color {
            png::ColorType::Grayscale => vec![escalar(p[1])],
            png::ColorType::GrayscaleAlpha => vec![escalar(p[1]), escalar(p[3])],
            png::ColorType::Rgb => vec![escalar(p[0]), escalar(p[1]), escalar(p[2])],
            png::ColorType::Rgba => vec![escalar(p[0]), escalar(p[1]), escalar(p[2]), escalar(p[3])],
            png::ColorType::Indexed => vec![u16::from(p[0]) % (maximo as u16 + 1)],
        };
        datos.extend(muestras);
    }
    if color == png::ColorType::Indexed {
        for i in 0..=maximo {
            paleta.extend_from_slice(&[(i * 37) as u8, (i * 91) as u8, (i * 13) as u8]);
        }
    }
    let mut bytes = Vec::new();
    let por_linea = (w as usize * datos.len() / (w * h) as usize * profundidad as usize).div_ceil(8);
    let canales = datos.len() / (w * h) as usize;
    for y in 0..h as usize {
        let mut linea = vec![0u8; por_linea];
        for k in 0..w as usize * canales {
            let v = datos[y * w as usize * canales + k];
            match profundidad {
                16 => linea[k * 2..k * 2 + 2].copy_from_slice(&v.to_be_bytes()),
                8 => linea[k] = v as u8,
                b => linea[k * b as usize / 8] |= (v as u8) << (8 - b as usize - k * b as usize % 8),
            }
        }
        bytes.extend(linea);
    }
    let mut salida = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut salida, w, h);
        enc.set_color(color);
        enc.set_depth(bits);
        if color == png::ColorType::Indexed {
            enc.set_palette(paleta);
            enc.set_trns(vec![0u8, 128, 255]);
        }
        let Ok(mut escritor) = enc.write_header() else { return Vec::new() };
        if escritor.write_image_data(&bytes).is_err() {
            return Vec::new();
        }
    }
    salida
}

fn generados() -> Vec<Caso> {
    let mut v = Vec::new();
    use png::{BitDepth as B, ColorType as C};
    for (color, bits) in [
        (C::Grayscale, B::One),
        (C::Grayscale, B::Two),
        (C::Grayscale, B::Four),
        (C::Grayscale, B::Eight),
        (C::Grayscale, B::Sixteen),
        (C::GrayscaleAlpha, B::Eight),
        (C::GrayscaleAlpha, B::Sixteen),
        (C::Rgb, B::Eight),
        (C::Rgb, B::Sixteen),
        (C::Rgba, B::Eight),
        (C::Rgba, B::Sixteen),
        (C::Indexed, B::One),
        (C::Indexed, B::Two),
        (C::Indexed, B::Four),
        (C::Indexed, B::Eight),
    ] {
        v.push((format!("generada: PNG {color:?} {} bits", bits as u8), png_generado(173, 61, color, bits)));
    }
    v.extend(jpeg_generados());
    v.push((String::from("generada: GIF con transparencia"), gif_generado(97, 43)));
    v
}

fn jpeg_generados() -> Vec<Caso> {
    use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
    let (w, h) = (203u16, 77u16);
    let rgba = dibujo(u32::from(w), u32::from(h));
    let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let gris: Vec<u8> = rgba.chunks(4).map(|p| p[1]).collect();
    let mut v = Vec::new();
    for (nombre, muestreo, progresivo, reinicio) in [
        ("4:4:4", SamplingFactor::R_4_4_4, false, 0),
        ("4:2:2", SamplingFactor::R_4_2_2, false, 0),
        ("4:2:0", SamplingFactor::R_4_2_0, false, 0),
        ("4:2:0 con reinicios", SamplingFactor::R_4_2_0, false, 3),
        ("progresivo 4:4:4", SamplingFactor::R_4_4_4, true, 0),
        ("progresivo 4:2:0", SamplingFactor::R_4_2_0, true, 0),
        ("progresivo 4:2:0 con reinicios", SamplingFactor::R_4_2_0, true, 5),
    ] {
        let mut salida = Vec::new();
        let mut enc = Encoder::new(&mut salida, 85);
        enc.set_sampling_factor(muestreo);
        enc.set_progressive(progresivo);
        if reinicio > 0 {
            enc.set_restart_interval(reinicio);
        }
        if enc.encode(&rgb, w, h, ColorType::Rgb).is_ok() {
            v.push((format!("generada: JPEG {nombre}"), salida));
        }
    }
    let mut salida = Vec::new();
    if Encoder::new(&mut salida, 90).encode(&gris, w, h, ColorType::Luma).is_ok() {
        v.push((String::from("generada: JPEG gris"), salida));
    }
    v
}

fn gif_generado(w: u16, h: u16) -> Vec<u8> {
    let paleta: Vec<u8> = (0..=255u32).flat_map(|i| [(i * 7) as u8, (i * 3) as u8, (255 - i) as u8]).collect();
    let mut salida = Vec::new();
    {
        let Ok(mut enc) = gif::Encoder::new(&mut salida, w, h, &paleta) else { return Vec::new() };
        let indices: Vec<u8> =
            (0..u32::from(w) * u32::from(h)).map(|i| ((i * 31) % 256) as u8 ^ ((i / 97) as u8)).collect();
        let mut marco = gif::Frame {
            width: w,
            height: h,
            buffer: std::borrow::Cow::Owned(indices),
            transparent: Some(5),
            ..Default::default()
        };
        marco.left = 0;
        if enc.write_frame(&marco).is_err() {
            return Vec::new();
        }
    }
    salida
}

fn main() {
    let mut casos = reales();
    let n_reales = casos.len();
    casos.extend(generados());
    println!("{} imágenes ({} reales del sistema, {} generadas)\n", casos.len(), n_reales, casos.len() - n_reales);
    let (mut t_q, mut t_r) = (Duration::ZERO, Duration::ZERO);
    let mut por_formato: std::collections::BTreeMap<String, (Duration, Duration)> = Default::default();
    let (mut iguales, mut cerca, mut mal) = (0, 0, Vec::new());
    for (nombre, datos) in &casos {
        let t = Instant::now();
        let nuestra = quimera_imagen::decodificar(datos);
        t_q += t.elapsed();
        let tq = t.elapsed();
        let t = Instant::now();
        let otra = referencia(datos);
        t_r += t.elapsed();
        let f = por_formato.entry(format!("{:?}", quimera_imagen::formato(datos))).or_default();
        f.0 += tq;
        f.1 += t.elapsed();
        match (nuestra, otra) {
            (Ok(a), Ok((w, h, rgba))) if (a.ancho, a.alto) == (w, h) && a.rgba.len() == rgba.len() => {
                let max = a.rgba.iter().zip(&rgba).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
                let distintos = a.rgba.iter().zip(&rgba).filter(|(x, y)| x.abs_diff(**y) > 1).count();
                match max {
                    0 => iguales += 1,
                    m if m <= 2 => cerca += 1,
                    m => mal.push(format!("{nombre}: diferencia máxima {m}, {distintos} valores con más de 1")),
                }
            }
            (Ok(a), Ok((w, h, _))) => {
                mal.push(format!("{nombre}: tamaño {}×{} y la referencia {w}×{h}", a.ancho, a.alto))
            }
            (Err(e), Ok(_)) => mal.push(format!("{nombre}: QUIMERA falla: {e}")),
            (Ok(_), Err(e)) => println!("(la referencia falla en {nombre}: {e})"),
            (Err(_), Err(_)) => println!("(los dos fallan en {nombre})"),
        }
    }
    println!(
        "Píxeles idénticos: {iguales} · diferencia de 1-2 (redondeo del color): {cerca} · distintas: {}",
        mal.len()
    );
    for m in mal.iter().take(12) {
        println!("  {m}");
    }
    println!(
        "\nVelocidad: QUIMERA {:.0} ms · referencias {:.0} ms · QUIMERA/referencias {:.2}x",
        t_q.as_secs_f64() * 1e3,
        t_r.as_secs_f64() * 1e3,
        t_r.as_secs_f64() / t_q.as_secs_f64()
    );
    for (f, (q, r)) in &por_formato {
        println!(
            "  {f:<12} QUIMERA {:>6.0} ms · referencia {:>6.0} ms · {:.2}x",
            q.as_secs_f64() * 1e3,
            r.as_secs_f64() * 1e3,
            r.as_secs_f64() / q.as_secs_f64()
        );
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Las generadas (pequeñas, rápidas): los mismos píxeles que las referencias.
    #[test]
    fn generadas_iguales_que_las_referencias() {
        let casos = generados();
        assert_eq!(casos.len(), 15 + 8 + 1);
        for (nombre, datos) in casos {
            let q = quimera_imagen::decodificar(&datos).map_err(|e| e.to_string());
            let r = referencia(&datos);
            match (q, r) {
                (Ok(q), Ok((w, h, rgba))) => assert!((q.ancho, q.alto) == (w, h) && q.rgba == rgba, "{nombre}"),
                (q, r) => panic!("{nombre}: QUIMERA {:?} · referencia {:?}", q.err(), r.err()),
            }
        }
    }
}
