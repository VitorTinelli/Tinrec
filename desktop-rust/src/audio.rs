use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use pinray::{AudioData, AudioFrame, SampleFormat};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

pub struct MicPacket {
    pub at: Duration,
    pub data: Vec<u8>,
}
pub struct Microphone {
    _stream: cpal::Stream,
    pub receiver: Receiver<MicPacket>,
    pub failed: Arc<AtomicBool>,
    pub rate: u32,
    pub channels: u16,
}
pub fn list() -> Result<Vec<String>> {
    cpal::default_host()
        .input_devices()?
        .map(|d| Ok(d.name()?))
        .collect()
}
impl Microphone {
    pub fn start(name: Option<&str>, epoch: Instant) -> Result<Self> {
        let host = cpal::default_host();
        let device = match name {
            Some(name) => host
                .input_devices()?
                .find(|d| d.name().is_ok_and(|n| n == name))
                .context("Microfone não encontrado; consulte sources")?,
            None => host
                .default_input_device()
                .context("Nenhum microfone padrão disponível")?,
        };
        let config = device.default_input_config()?;
        let rate = config.sample_rate().0;
        // WASAPI may deliver buffered callbacks in a burst after a scheduler
        // stall. Keep a bounded Windows buffer comparable to the encoder queue;
        // sustained overload still fails instead of silently dropping audio.
        let capacity = if cfg!(target_os = "windows") { 256 } else { 32 };
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let failed = Arc::new(AtomicBool::new(false));
        let error_flag = failed.clone();
        let on_error = move |err| {
            eprintln!("Microfone: {err}");
            error_flag.store(true, Ordering::Relaxed);
        };
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(
                &device,
                &config.into(),
                sender,
                failed.clone(),
                epoch,
                on_error,
            )?,
            cpal::SampleFormat::I16 => build::<i16>(
                &device,
                &config.into(),
                sender,
                failed.clone(),
                epoch,
                on_error,
            )?,
            cpal::SampleFormat::U16 => build::<u16>(
                &device,
                &config.into(),
                sender,
                failed.clone(),
                epoch,
                on_error,
            )?,
            format => bail!("Formato de microfone ainda não suportado: {format:?}"),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            receiver,
            failed,
            rate,
            channels: 1,
        })
    }
}
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sender: SyncSender<MicPacket>,
    failed: Arc<AtomicBool>,
    epoch: Instant,
    on_error: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::Sample,
    f32: cpal::FromSample<T>,
{
    let channels = usize::from(config.channels);
    let rate = config.sample_rate.0;
    let mut anchor: Option<(cpal::StreamInstant, Duration)> = None;
    let mut clock = PacketClock::default();
    Ok(device.build_input_stream(
        config,
        move |samples: &[T], info: &cpal::InputCallbackInfo| {
            let ts = info.timestamp();
            let delay = ts.callback.duration_since(&ts.capture).unwrap_or_default();
            // Map the device clock once; mapping callback arrival on every packet
            // mistakes scheduler jitter and buffered bursts for capture-time jumps.
            let (native, wall) =
                *anchor.get_or_insert((ts.capture, epoch.elapsed().saturating_sub(delay)));
            let candidate = wall + ts.capture.duration_since(&native).unwrap_or_default();
            let at = clock.timestamp(candidate, samples.len() / channels, rate);
            let data = microphone_mono(samples, channels);
            if sender.try_send(MicPacket { at, data }).is_err() {
                failed.store(true, Ordering::Relaxed);
            }
        },
        on_error,
        None,
    )?)
}

#[derive(Default)]
struct PacketClock {
    next: Option<Duration>,
}
impl PacketClock {
    fn timestamp(&mut self, candidate: Duration, frames: usize, rate: u32) -> Duration {
        // Keep packets disjoint under small native timestamp jitter. A positive
        // discontinuity remains visible as a gap instead of compressing real time.
        let at = self.next.map_or(candidate, |next| candidate.max(next));
        self.next =
            Some(at + Duration::from_nanos(frames as u64 * 1_000_000_000 / u64::from(rate)));
        at
    }
}

/// A voice input is mono even when the interface exposes two input channels.
/// Average the channels instead of treating an unused input as one ear's audio.
pub fn microphone_mono<T>(samples: &[T], channels: usize) -> Vec<u8>
where
    T: cpal::Sample,
    f32: cpal::FromSample<T>,
{
    samples
        .chunks_exact(channels)
        .flat_map(|frame| {
            let value = frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32;
            value.to_le_bytes()
        })
        .collect()
}

/// Normalize native samples to interleaved little-endian float PCM for the transport.
pub fn normalize(frame: &AudioFrame) -> Result<Vec<u8>> {
    let bytes = match frame.sample_format {
        SampleFormat::I16 => 2,
        SampleFormat::I32 | SampleFormat::F32 => 4,
        SampleFormat::F64 => 8,
    };
    let channels = usize::from(frame.channels);
    if channels == 0 || frame.sample_rate == 0 {
        bail!("Formato de áudio inválido");
    }
    let sample = |s: &[u8]| -> f32 {
        match frame.sample_format {
            SampleFormat::I16 => i16::from_ne_bytes(s.try_into().unwrap()) as f32 / 32768.0,
            SampleFormat::I32 => i32::from_ne_bytes(s.try_into().unwrap()) as f32 / 2147483648.0,
            SampleFormat::F32 => f32::from_ne_bytes(s.try_into().unwrap()),
            SampleFormat::F64 => f64::from_ne_bytes(s.try_into().unwrap()) as f32,
        }
    };
    let mut out = Vec::new();
    match &frame.data {
        AudioData::Interleaved(data) => {
            if data.len() % (bytes * channels) != 0 {
                bail!("Pacote de áudio incompleto");
            }
            out.reserve(data.len() / bytes * 4);
            for chunk in data.chunks_exact(bytes) {
                out.extend_from_slice(&sample(chunk).to_le_bytes());
            }
        }
        AudioData::Planar(planes) => {
            if planes.len() != channels
                || planes
                    .iter()
                    .any(|p| p.len() % bytes != 0 || p.len() != planes[0].len())
            {
                bail!("Áudio planar inválido");
            }
            for i in 0..planes[0].len() / bytes {
                for plane in planes {
                    out.extend_from_slice(
                        &sample(&plane[i * bytes..(i + 1) * bytes]).to_le_bytes(),
                    );
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_jitter_cannot_overlap_audio_packets_but_real_gaps_are_preserved() {
        let mut clock = PacketClock::default();
        assert_eq!(
            clock.timestamp(Duration::from_millis(10), 480, 48000),
            Duration::from_millis(10)
        );
        assert_eq!(
            clock.timestamp(Duration::from_millis(18), 480, 48000),
            Duration::from_millis(20)
        );
        assert_eq!(
            clock.timestamp(Duration::from_millis(17), 480, 48000),
            Duration::from_millis(30)
        );
        assert_eq!(
            clock.timestamp(Duration::from_millis(100), 480, 48000),
            Duration::from_millis(100)
        );
    }
    #[test]
    fn microphone_with_one_silent_input_becomes_mono_from_either_side() {
        for input in [[0.8f32, 0.0, -0.4, 0.0], [0.0, 0.8, 0.0, -0.4]] {
            let bytes = microphone_mono(&input, 2);
            let values: Vec<_> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            assert_eq!(values, vec![0.4, -0.2]);
        }
        assert_eq!(
            microphone_mono(&[0.8f32, -0.4], 1),
            [0.8f32, -0.4]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn planar_stereo_is_interleaved_and_integer_amplitude_is_preserved() {
        let f = AudioFrame {
            stream_time_ns: 0,
            sequence: 0,
            sample_rate: 48000,
            channels: 2,
            sample_format: SampleFormat::I16,
            data: AudioData::Planar(vec![
                [-32768i16, 0]
                    .into_iter()
                    .flat_map(i16::to_ne_bytes)
                    .collect(),
                [16384i16, 32767]
                    .into_iter()
                    .flat_map(i16::to_ne_bytes)
                    .collect(),
            ]),
        };
        let result = normalize(&f).unwrap();
        let samples: Vec<_> = result
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert_eq!(samples, vec![-1.0, 0.5, 0.0, 32767.0 / 32768.0]);
    }
}
