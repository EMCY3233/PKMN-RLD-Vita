//! Salida de audio de PS Vita (sceAudioOut): un puerto BGM a 48 kHz en estéreo alimentado desde el
//! hilo que llama a `play` (el mezclador del juego). sceAudioOutOutput bloquea hasta que el bloque
//! anterior se ha reproducido, así que el hilo va al ritmo del audio. Se fija al núcleo 1 para no
//! competir con la lógica del juego (núcleo 0).

use crate::{error::Error, session, source::{self, ChannelCount, SampleRate, Source}};
use vitasdk_sys::*;

/// Muestras por canal y bloque (≈10,7 ms a 48 kHz): poca latencia entre la acción y su sonido
const GRAIN: usize = 512;
/// Prioridad más alta de usuario (64); el hilo pasa casi todo el tiempo bloqueado en sceAudioOutOutput
const PRIORITY: i32 = 64;

pub struct Device;
pub struct OutputStream;
pub struct Session;

impl Device {
    pub fn channel_count(&self) -> ChannelCount {
        source::consts::CH_STEREO
    }

    pub fn sample_rate(&self) -> SampleRate {
        source::consts::SR_48000
    }
}

impl Session {
    pub fn new() -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn default_output_device(&self) -> Result<session::Device, Error> {
        session_wrap!(Ok(Device), Device(DeviceImpl), Vita)
    }

    pub fn open_output_stream(&self, _device: session::Device) -> Result<session::OutputStream, Error> {
        session_wrap!(Ok(OutputStream), OutputStream(OutputStreamImpl), Vita)
    }
}

impl OutputStream {
    pub fn play(&self, mut source: impl Source + Send + 'static) -> Result<(), Error> {
        unsafe {
            sceKernelChangeThreadCpuAffinityMask(sceKernelGetThreadId(), SCE_KERNEL_CPU_MASK_USER_1 as _);
            sceKernelChangeThreadPriority(sceKernelGetThreadId(), PRIORITY);
        }
        let port = unsafe {
            sceAudioOutOpenPort(SCE_AUDIO_OUT_PORT_TYPE_BGM, GRAIN as _, 48000, SCE_AUDIO_OUT_MODE_STEREO)
        };
        if port < 0 {
            return Err(Error::ApiNotAvailable)
        }
        let mut f = vec![0.0f32; GRAIN * 2];
        let mut pcm = vec![0i16; GRAIN * 2];
        loop {
            let n = source.write_samples(&mut f).min(f.len());
            for s in &mut f[n..] {
                *s = 0.0;
            }
            for (o, s) in pcm.iter_mut().zip(f.iter()) {
                *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            }
            unsafe {
                sceAudioOutOutput(port, pcm.as_ptr().cast());
            }
        }
    }
}
