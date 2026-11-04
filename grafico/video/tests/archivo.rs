//! Decodifica de verdad un MP4 con H.264 si `QUIMERA_VIDEO_PRUEBA` apunta a uno (no se guardan
//! vídeos en el repositorio). Con `QUIMERA_VIDEO_FOTOGRAMA=ruta` escribe el décimo fotograma en RGBA crudo.

#[cfg(windows)]
#[test]
fn decodifica_un_mp4_con_h264() {
    let Some(ruta) = std::env::var_os("QUIMERA_VIDEO_PRUEBA") else { return };
    let datos = std::fs::read(ruta).unwrap_or_default();
    let Ok((pistas, muestras)) = quimera_video::mp4::leer_archivo(&datos) else { panic!("MP4 no válido") };
    let Some(p) = pistas.iter().find(|p| matches!(p.codec, quimera_video::mp4::Codec::Avc { .. })) else {
        panic!("sin pista H.264: {pistas:?}")
    };
    let quimera_video::mp4::Codec::Avc { avcc, ancho, alto } = &p.codec else { return };
    let mut d = match quimera_video::DecodificadorH264::nuevo(avcc, p.escala) {
        Ok(d) => d,
        Err(e) => panic!("{e}"),
    };
    let inicio = std::time::Instant::now();
    let mut fotogramas = Vec::new();
    let suyas: Vec<_> = muestras.iter().filter(|m| m.pista == p.id).collect();
    for m in &suyas {
        match d.decodificar(m) {
            Ok(f) => fotogramas.extend(f),
            Err(e) => panic!("{e}"),
        }
    }
    fotogramas.extend(d.vaciar().unwrap_or_default());
    let ms = inicio.elapsed().as_secs_f64() * 1e3;
    println!("{ancho}x{alto}: {} muestras → {} fotogramas en {ms:.0} ms", suyas.len(), fotogramas.len());
    assert!(fotogramas.len() + 4 >= suyas.len(), "salen casi todos");
    let f = &fotogramas[fotogramas.len().min(10) - 1];
    println!("fotograma: {}x{} a {:.3} s", f.ancho, f.alto, f.tiempo);
    if let Some(salida) = std::env::var_os("QUIMERA_VIDEO_FOTOGRAMA") {
        let _ = std::fs::write(salida, &f.rgba);
    }
}
