//! Pruebas de `MediaSource` y `<video>` con un MP4 fragmentado hecho a mano.

use crate::pruebas::{correr, dom};
use alloc::vec::Vec;

/// Un MP4 fragmentado de prueba: el segmento de inicio (una pista H.264 de 640 × 360 a 90 000
/// por segundo) y un fragmento con dos muestras de medio segundo desde `tiempo`.
fn mp4_de_prueba(tiempo: u32) -> (Vec<u8>, Vec<u8>) {
    fn caja(tipo: &[u8; 4], cuerpo: &[u8]) -> Vec<u8> {
        let mut v = ((cuerpo.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(tipo);
        v.extend_from_slice(cuerpo);
        v
    }
    let mut visual = alloc::vec![0u8; 78];
    visual[24..28].copy_from_slice(&[2, 128, 1, 104]);
    visual.extend(caja(b"avcC", &[1, 0x64, 0, 0x1f, 0xff, 0xe0, 0]));
    let stsd = [alloc::vec![0, 0, 0, 0, 0, 0, 0, 1], caja(b"avc1", &visual)].concat();
    let mut mdhd = alloc::vec![0u8; 20];
    mdhd[12..16].copy_from_slice(&90_000u32.to_be_bytes());
    let minf = caja(b"minf", &caja(b"stbl", &caja(b"stsd", &stsd)));
    let mut tkhd = alloc::vec![0u8; 20];
    tkhd[15] = 1;
    let trak = caja(b"trak", &[caja(b"tkhd", &tkhd), caja(b"mdia", &[caja(b"mdhd", &mdhd), minf].concat())].concat());
    // trex: duración por defecto 45 000 (medio segundo).
    let trex = caja(b"trex", &[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0xaf, 0xc8, 0, 0, 0, 0, 0, 0, 0, 0]);
    let inicio = [caja(b"ftyp", b"iso5"), caja(b"moov", &[trak, caja(b"mvex", &trex)].concat())].concat();
    let tfhd = caja(b"tfhd", &[0, 0x02, 0, 0, 0, 0, 0, 1]);
    let tfdt = caja(b"tfdt", &[[0u8, 0, 0, 0].as_slice(), &tiempo.to_be_bytes()].concat());
    let mut trun = alloc::vec![0, 0, 0x02, 0x01, 0, 0, 0, 2, 0, 0, 0, 0];
    trun.extend_from_slice(&3u32.to_be_bytes());
    trun.extend_from_slice(&2u32.to_be_bytes());
    let traf = caja(b"traf", &[tfhd, tfdt, caja(b"trun", &trun)].concat());
    let mut moof = caja(b"moof", &[caja(b"mfhd", &[0; 8]), traf].concat());
    let desfase = (moof.len() + 8) as u32;
    let i = moof.windows(4).position(|w| w == b"trun").unwrap_or(0) + 12;
    moof[i..i + 4].copy_from_slice(&desfase.to_be_bytes());
    (inicio, [moof, caja(b"mdat", &[1, 2, 3, 4, 5])].concat())
}

#[test]
fn mediasource_y_reloj_del_video() {
    let mut d = dom("<video id=v></video>");
    let (inicio, fragmento) = mp4_de_prueba(0);
    let g = d.motor.global();
    let b = d.motor.crear_array_buffer(&inicio);
    d.motor.definir(g, "inicio", b);
    let b = d.motor.crear_array_buffer(&fragmento);
    d.motor.definir(g, "fragmento", b);
    let r = correr(
        &mut d,
        "var v = document.getElementById('v'), ms = new MediaSource(), ev = [];
         console.log(MediaSource.isTypeSupported('video/mp4; codecs=\"avc1.64001f\"'), MediaSource.isTypeSupported('video/mp4'),
           MediaSource.isTypeSupported('video/webm; codecs=\"vp9\"'), v.canPlayType('video/mp4; codecs=\"avc1.42E01E, mp4a.40.2\"'));
         ['loadedmetadata', 'loadeddata', 'canplay', 'play', 'playing', 'waiting', 'pause'].forEach(function (t) {
           v.addEventListener(t, function () { ev.push(t); });
         });
         ms.addEventListener('sourceopen', function () {
           var sb = ms.addSourceBuffer('video/mp4; codecs=\"avc1.64001f\"');
           sb.addEventListener('updateend', function fin() {
             sb.removeEventListener('updateend', fin);
             sb.addEventListener('updateend', function () {
               console.log('datos', v.buffered.length, v.buffered.start(0), v.buffered.end(0), v.readyState, v.videoWidth, v.videoHeight);
               v.play();
             });
             sb.appendBuffer(fragmento);
           });
           sb.appendBuffer(inicio);
         });
         v.src = URL.createObjectURL(ms);",
    );
    assert_eq!(r, ["true false false probably"]);
    for t in [1.0, 2.0, 3.0, 4.0, 300.0, 600.0, 1600.0] {
        d.correr_temporizadores(t);
    }
    assert_eq!(d.consola(), ["datos 1 0 1 4 640 360"]);
    let videos = d.tomar_videos();
    assert_eq!((videos.len(), videos[0].muestras.len(), videos[0].config.as_ref().map(|c| c.1)), (1, 2, Some(90_000)));
    assert!((videos[0].tiempo - 1.0).abs() < 1e-9, "el reloj se para al final de los datos: {}", videos[0].tiempo);
    assert!(d.tomar_videos()[0].muestras.is_empty(), "las muestras se toman una vez");
    let r = correr(&mut d, "v.pause(); console.log(ev.join(' '), v.currentTime, v.paused);");
    assert_eq!(r, ["loadedmetadata loadeddata canplay play playing waiting pause 1 true"]);
}
