//! H.264 con el decodificador de Windows (Media Foundation, `CLSID_MSH264DecoderMFT`), que usa
//! la GPU cuando puede. Es la única parte de QUIMERA que delega en el sistema: el MP4, el paso a
//! Annex B y lo demás son propios.
//!
//! El decodificador es un objeto COM: todo lo que se le pide pasa por `unsafe`. Cada bloque dice
//! por qué es correcto.

use crate::h264::{self, Config};
use crate::mp4::Muestra;
use crate::{Error, Fotograma};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::ManuallyDrop;
use windows::Win32::Media::MediaFoundation::{
    CLSID_MSH264DecoderMFT, IMFMediaType, IMFSample, IMFTransform, MF_E_TRANSFORM_NEED_MORE_INPUT,
    MF_E_TRANSFORM_STREAM_CHANGE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_SUBTYPE, MF_VERSION, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
    MFMediaType_Video, MFSTARTUP_FULL, MFStartup, MFT_MESSAGE_COMMAND_DRAIN, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFVideoArea, MFVideoFormat_H264, MFVideoFormat_NV12,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};

fn fallo(que: &str, e: windows::core::Error) -> Error {
    format!("Media Foundation ({que}): {e}")
}

/// Un decodificador de H.264 para una pista.
pub struct DecodificadorH264 {
    mft: IMFTransform,
    config: Config,
    /// Unidades de tiempo por segundo de la pista.
    escala: u32,
    /// Ancho, alto, paso de línea y recorte visible (x, y, ancho, alto) de lo que sale.
    salida: Option<Formato>,
}

#[derive(Clone, Copy)]
struct Formato {
    ancho: u32,
    alto: u32,
    paso: u32,
    visible: (u32, u32, u32, u32),
}

impl DecodificadorH264 {
    /// Uno para la configuración `avcC` de una pista de `escala` unidades por segundo.
    pub fn nuevo(avcc: &[u8], escala: u32) -> Result<DecodificadorH264, Error> {
        let config = h264::config(avcc).ok_or_else(|| String::from("avcC no válido"))?;
        // SAFETY: inicializar COM y Media Foundation en este hilo es seguro las veces que haga
        // falta (las repetidas solo suman una referencia); el resultado de COM se ignora porque
        // «ya iniciado» también vale.
        let mft: IMFTransform = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| fallo("MFStartup", e))?;
            CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| fallo("crear el decodificador", e))?
        };
        // SAFETY: `mft` es un IMFTransform válido y los tipos se crean y rellenan antes de dárselos.
        unsafe {
            let entrada = MFCreateMediaType().map_err(|e| fallo("tipo", e))?;
            entrada.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| fallo("tipo", e))?;
            entrada.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(|e| fallo("tipo", e))?;
            mft.SetInputType(0, &entrada, 0).map_err(|e| fallo("tipo de entrada", e))?;
        }
        let mut d = DecodificadorH264 { mft, config, escala: escala.max(1), salida: None };
        d.elegir_salida()?;
        // SAFETY: mensajes sin datos a un IMFTransform válido.
        unsafe {
            d.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).map_err(|e| fallo("empezar", e))?;
            d.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0).map_err(|e| fallo("empezar", e))?;
        }
        Ok(d)
    }

    /// Pide la salida en NV12 y apunta su tamaño (se repite cuando el vídeo cambia de formato).
    fn elegir_salida(&mut self) -> Result<(), Error> {
        for i in 0.. {
            // SAFETY: `GetOutputAvailableType` devuelve un tipo nuevo o un error al acabarse la lista.
            let tipo = match unsafe { self.mft.GetOutputAvailableType(0, i) } {
                Ok(t) => t,
                Err(_) => break,
            };
            // SAFETY: `tipo` es un IMFMediaType válido que nos acaba de dar el decodificador.
            if unsafe { tipo.GetGUID(&MF_MT_SUBTYPE) }.is_ok_and(|g| g == MFVideoFormat_NV12) {
                // SAFETY: se le devuelve uno de los tipos que él mismo ofrece.
                unsafe { self.mft.SetOutputType(0, &tipo, 0) }.map_err(|e| fallo("tipo de salida", e))?;
                self.salida = Some(formato(&tipo));
                return Ok(());
            }
        }
        // Antes del primer SPS puede no ofrecer nada: se vuelve a intentar al cambiar el formato.
        Ok(())
    }

    /// Mete una muestra y devuelve los fotogramas que ya salen (el decodificador puede quedarse
    /// unos cuantos dentro para ordenarlos).
    pub fn decodificar(&mut self, m: &Muestra) -> Result<Vec<Fotograma>, Error> {
        let datos = h264::annex_b(&self.config, &m.datos, m.clave);
        let a_100ns = |t: i64| t * 10_000_000 / i64::from(self.escala);
        // SAFETY: el búfer se crea del tamaño justo, se bloquea, se copia dentro de su capacidad y
        // se desbloquea antes de dárselo a la muestra.
        let muestra = unsafe {
            let buf = MFCreateMemoryBuffer(datos.len() as u32).map_err(|e| fallo("búfer", e))?;
            let mut p = core::ptr::null_mut();
            buf.Lock(&mut p, None, None).map_err(|e| fallo("búfer", e))?;
            core::ptr::copy_nonoverlapping(datos.as_ptr(), p, datos.len());
            buf.Unlock().map_err(|e| fallo("búfer", e))?;
            buf.SetCurrentLength(datos.len() as u32).map_err(|e| fallo("búfer", e))?;
            let s = MFCreateSample().map_err(|e| fallo("muestra", e))?;
            s.AddBuffer(&buf).map_err(|e| fallo("muestra", e))?;
            s.SetSampleTime(a_100ns(m.pts)).map_err(|e| fallo("muestra", e))?;
            s.SetSampleDuration(a_100ns(i64::from(m.duracion))).map_err(|e| fallo("muestra", e))?;
            s
        };
        let mut v = self.sacar()?;
        // SAFETY: `muestra` es válida; si el decodificador está lleno se vacía y se reintenta.
        if unsafe { self.mft.ProcessInput(0, &muestra, 0) }.is_err() {
            v.extend(self.sacar()?);
            // SAFETY: igual que arriba.
            unsafe { self.mft.ProcessInput(0, &muestra, 0) }.map_err(|e| fallo("entrada", e))?;
        }
        v.extend(self.sacar()?);
        Ok(v)
    }

    /// Lo que queda dentro, al acabar el vídeo.
    pub fn vaciar(&mut self) -> Result<Vec<Fotograma>, Error> {
        // SAFETY: mensajes sin datos a un IMFTransform válido.
        unsafe {
            self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0).map_err(|e| fallo("acabar", e))?;
            self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0).map_err(|e| fallo("acabar", e))?;
        }
        self.sacar()
    }

    /// Todos los fotogramas que el decodificador tiene listos.
    fn sacar(&mut self) -> Result<Vec<Fotograma>, Error> {
        let mut v = Vec::new();
        loop {
            // SAFETY: consulta sin efectos a un IMFTransform válido.
            let info = unsafe { self.mft.GetOutputStreamInfo(0) }.map_err(|e| fallo("salida", e))?;
            let propia = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
            let nuestra: Option<IMFSample> = if propia {
                None
            } else {
                // SAFETY: muestra y búfer nuevos del tamaño que pide el decodificador.
                unsafe {
                    let s = MFCreateSample().map_err(|e| fallo("muestra", e))?;
                    let b = MFCreateMemoryBuffer(info.cbSize.max(1)).map_err(|e| fallo("búfer", e))?;
                    s.AddBuffer(&b).map_err(|e| fallo("muestra", e))?;
                    Some(s)
                }
            };
            let mut salida = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(nuestra),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut estado = 0u32;
            // SAFETY: `salida` vive durante la llamada; después se recuperan (y sueltan) la muestra y
            // los eventos que haya dejado.
            let r = unsafe { self.mft.ProcessOutput(0, &mut salida, &mut estado) };
            // SAFETY: cada `ManuallyDrop` se toma una sola vez.
            let (muestra, _eventos) =
                unsafe { (ManuallyDrop::take(&mut salida[0].pSample), ManuallyDrop::take(&mut salida[0].pEvents)) };
            match r {
                Ok(()) => {
                    if let (Some(s), Some(f)) = (muestra, self.salida) {
                        v.push(copiar(&s, f)?);
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(v),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.elegir_salida()?,
                Err(e) => return Err(fallo("salida", e)),
            }
        }
    }
}

/// Ancho, alto, paso de línea y zona visible de un tipo NV12.
fn formato(t: &IMFMediaType) -> Formato {
    // SAFETY: lecturas de atributos de un IMFMediaType válido; si faltan se usan valores neutros.
    unsafe {
        let tam = t.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
        let (ancho, alto) = ((tam >> 32) as u32, tam as u32);
        let paso = t.GetUINT32(&MF_MT_DEFAULT_STRIDE).map_or(ancho, |p| (p as i32).unsigned_abs()).max(ancho);
        let mut area = MFVideoArea::default();
        let mut largo = 0u32;
        let bytes = core::slice::from_raw_parts_mut(
            (&mut area as *mut MFVideoArea).cast::<u8>(),
            core::mem::size_of::<MFVideoArea>(),
        );
        let visible = match t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, bytes, Some(&mut largo)) {
            Ok(()) if area.Area.cx > 0 && area.Area.cy > 0 => (
                area.OffsetX.value.max(0) as u32,
                area.OffsetY.value.max(0) as u32,
                area.Area.cx as u32,
                area.Area.cy as u32,
            ),
            _ => (0, 0, ancho, alto),
        };
        Formato { ancho, alto, paso, visible }
    }
}

/// Copia un fotograma NV12 de una muestra de salida y lo pasa a RGBA (recortado a lo visible).
fn copiar(s: &IMFSample, f: Formato) -> Result<Fotograma, Error> {
    // SAFETY: la muestra es la que acaba de devolver el decodificador; el búfer se lee solo
    // dentro de la longitud que dice y se desbloquea antes de salir.
    unsafe {
        let tiempo = s.GetSampleTime().unwrap_or(0);
        let buf = s.ConvertToContiguousBuffer().map_err(|e| fallo("fotograma", e))?;
        let mut p = core::ptr::null_mut();
        let mut largo = 0u32;
        buf.Lock(&mut p, None, Some(&mut largo)).map_err(|e| fallo("fotograma", e))?;
        let nv12 = core::slice::from_raw_parts(p, largo as usize);
        let rgba = crate::color::nv12_a_rgba(nv12, f.paso as usize, f.alto as usize, f.visible);
        buf.Unlock().map_err(|e| fallo("fotograma", e))?;
        let (_, _, ancho, alto) = f.visible;
        let _ = f.ancho;
        Ok(Fotograma { ancho, alto, rgba, tiempo: tiempo as f64 / 10_000_000.0 })
    }
}
