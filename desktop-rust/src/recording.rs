use crate::{
    audio,
    diagnostics::{self, Checks, Meter, Sample},
    encoder::{Encoder, Packet},
    matroska::AudioTrack,
    timeline::Timeline,
};
use anyhow::{Context, Result, bail};
use pinray::{
    AudioCapture, BackendPreference, CaptureEvent, CaptureSession, PixelFormat, SourceId,
    VideoCaptureTarget,
};
use serde::Serialize;
use std::{
    io::{self, BufRead},
    sync::{atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Serialize, Default)]
pub struct Report {
    pub mode: String,
    pub platform: String,
    pub backend: String,
    pub status: String,
    pub error: Option<String>,
    pub active_seconds: f64,
    pub wall_seconds: f64,
    pub captured_video_frames: u64,
    pub submitted_video_frames: u64,
    pub encoder_dropped_video_frames: u64,
    pub repeated_video_frames: u64,
    pub native_gap_events: u64,
    pub native_reported_dropped_frames: u64,
    pub audio_sample_frames: [u64; 2],
    pub audio_peak: [f32; 2],
    pub samples: Vec<Sample>,
    pub checks: Option<Checks>,
    pub clock_note: String,
}

#[derive(Clone, Copy, Debug)]
pub enum Control {
    Pause,
    Resume,
    Stop,
    SetMicrophoneMuted(bool),
    SetSystemMuted(bool),
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub state: &'static str,
    pub active_seconds: f64,
    pub video_frames: u64,
    pub audio_peak: [f32; 2],
    pub microphone_muted: bool,
    pub system_muted: bool,
}

#[derive(Clone, Debug)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub rgb: Vec<u8>,
}

struct Native(CaptureSession);
impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.0.stop();
    }
}

struct NativeClock {
    anchor: Option<(i64, Duration)>,
}
impl NativeClock {
    fn map(&mut self, timestamp: i64, received: Duration) -> Duration {
        let (native, wall) = *self.anchor.get_or_insert((timestamp, received));
        let ns = (i128::from(timestamp) - i128::from(native) + wall.as_nanos() as i128)
            .clamp(0, u64::MAX as i128);
        Duration::from_nanos(ns as u64)
    }
}

/// Cut audio at pause boundaries using each sample's capture time.
fn audio_packets(
    track: u8,
    at: Duration,
    rate: u32,
    channels: u16,
    data: &[u8],
    timeline: &Timeline,
    cutoff: Duration,
) -> Vec<Packet> {
    let mut packets = Vec::<Packet>::new();
    let mut previous_index = None;
    for (i, sample) in data.chunks_exact(usize::from(channels) * 4).enumerate() {
        let sample_time = at + Duration::from_nanos(i as u64 * 1_000_000_000 / u64::from(rate));
        let Some(mapped) = timeline.map(sample_time).filter(|t| *t < cutoff) else {
            previous_index = None;
            continue;
        };
        if previous_index != Some(i.saturating_sub(1)) || packets.is_empty() {
            packets.push(Packet {
                track,
                pts_ms: mapped.as_millis() as u64,
                data: Vec::new(),
            });
        }
        packets.last_mut().unwrap().data.extend_from_slice(sample);
        previous_index = Some(i);
    }
    packets
}

fn preview_frame(bgra: &[u8], width: u32, height: u32) -> Option<PreviewFrame> {
    if width == 0
        || height == 0
        || bgra.len() != usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?
    {
        return None;
    }
    let scale = (480.0 / f64::from(width))
        .min(270.0 / f64::from(height))
        .min(1.0);
    let out_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let out_height = (f64::from(height) * scale).round().max(1.0) as u32;
    let mut rgb = Vec::with_capacity((out_width * out_height * 3) as usize);
    for y in 0..out_height {
        for x in 0..out_width {
            let src_y = u64::from(y) * u64::from(height) / u64::from(out_height);
            let src_x = u64::from(x) * u64::from(width) / u64::from(out_width);
            let i = ((src_y * u64::from(width) + src_x) * 4) as usize;
            rgb.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i]]);
        }
    }
    Some(PreviewFrame {
        width: out_width,
        height: out_height,
        source_width: width,
        source_height: height,
        rgb,
    })
}

struct Sink<'a> {
    encoder: Option<Encoder>,
    audio: Vec<AudioTrack>,
    timeline: Timeline,
    limit: Duration,
    report: Report,
    last_frame: Option<Vec<u8>>,
    last_video_pts: Option<u64>,
    dimensions: (u32, u32),
    last_audio_pts: [Option<u64>; 2],
    volumes: [u8; 2],
    muted: [bool; 2],
    preview: Option<&'a dyn Fn(PreviewFrame)>,
    preview_at: Option<Duration>,
}
impl Sink<'_> {
    fn video(&mut self, at: Duration, data: Vec<u8>) -> Result<()> {
        self.report.captured_video_frames += 1;
        let Some(mapped) = self.timeline.map(at).filter(|t| *t < self.limit) else {
            return Ok(());
        };
        if let Some(preview) = self.preview
            && self
                .preview_at
                .is_none_or(|last| at.saturating_sub(last) >= Duration::from_millis(500))
        {
            if let Some(frame) = preview_frame(&data, self.dimensions.0, self.dimensions.1) {
                preview(frame);
            }
            self.preview_at = Some(at);
        }
        let pts = mapped.as_millis() as u64;
        if self.last_video_pts.is_some_and(|p| pts <= p) {
            return Ok(());
        }
        self.last_frame = Some(data.clone());
        if self.encoder.as_ref().unwrap().send(Packet {
            track: 1,
            pts_ms: pts,
            data,
        })? {
            self.last_video_pts = Some(pts);
            self.report.submitted_video_frames += 1;
        } else {
            self.report.encoder_dropped_video_frames += 1;
        }
        Ok(())
    }
    fn audio(&mut self, number: u8, at: Duration, data: &[u8]) -> Result<()> {
        let track = self
            .audio
            .iter()
            .find(|t| t.number == number)
            .context("Trilha de áudio desconhecida")?;
        for mut packet in audio_packets(
            number,
            at,
            track.rate,
            track.channels,
            data,
            &self.timeline,
            self.limit,
        ) {
            let index = usize::from(number - 2);
            if self.last_audio_pts[index].is_some_and(|last| packet.pts_ms < last) {
                bail!(
                    "Timestamp de áudio retrocedeu na trilha {} ({} → {} ms); arquivo parcial preservado.",
                    number,
                    self.last_audio_pts[index].unwrap(),
                    packet.pts_ms
                );
            }
            self.last_audio_pts[index] = Some(packet.pts_ms);
            self.report.audio_sample_frames[index] +=
                (packet.data.len() / 4 / usize::from(track.channels)) as u64;
            let gain = if self.muted[index] {
                0.0
            } else {
                f32::from(self.volumes[index]) / 100.0
            };
            for value in packet.data.as_chunks_mut::<4>().0 {
                let scaled = f32::from_le_bytes(*value) * gain;
                *value = scaled.to_le_bytes();
                self.report.audio_peak[index] = self.report.audio_peak[index].max(scaled.abs());
            }
            self.encoder.as_ref().unwrap().send(packet)?;
        }
        Ok(())
    }
    fn heartbeat(&mut self, elapsed: Duration) -> Result<()> {
        // Damage-driven capture can be completely still. Repeat the last frame
        // at 1 Hz so the MP4 duration includes still sections and the final second.
        let active = self.timeline.active(elapsed).min(self.limit);
        if !self.timeline.paused()
            && self
                .last_video_pts
                .is_some_and(|p| active.as_millis() as u64 > p + 1000)
            && let Some(last) = self.last_frame.clone()
        {
            self.video(elapsed, last)?;
            self.report.captured_video_frames -= 1;
            self.report.repeated_video_frames += 1;
        }
        Ok(())
    }
    fn finish_video(&mut self, active: Duration) -> Result<()> {
        let data = self
            .last_frame
            .take()
            .context("Nenhum quadro recebido da captura")?;
        let pts = active.as_millis().saturating_sub(1) as u64;
        if self.last_video_pts.is_none_or(|p| pts > p) {
            // Finish only after pending packets have drained. Audio overload still fails.
            for _ in 0..200 {
                if self.encoder.as_ref().unwrap().send(Packet {
                    track: 1,
                    pts_ms: pts,
                    data: data.clone(),
                })? {
                    self.report.submitted_video_frames += 1;
                    self.report.repeated_video_frames += 1;
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(5));
            }
            bail!("Encoder não aceitou o quadro final");
        }
        Ok(())
    }
}

pub fn run(options: &crate::RecordOptions, synthetic: bool) -> Result<Report> {
    let (sender, commands) = mpsc::channel();
    let signal = sender.clone();
    ctrlc::set_handler(move || {
        let _ = signal.send(Control::Stop);
    })?;
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            let command = match line.trim() {
                "p" => Control::Pause,
                "r" => Control::Resume,
                "q" => Control::Stop,
                _ => continue,
            };
            if sender.send(command).is_err() {
                break;
            }
        }
    });
    run_controlled(options, synthetic, commands, |_| {})
}

/// Shared recording engine for the CLI and native desktop. No stdin or process
/// signal handlers are installed here, so sessions may run sequentially in a GUI.
pub fn run_controlled(
    options: &crate::RecordOptions,
    synthetic: bool,
    commands: mpsc::Receiver<Control>,
    on_progress: impl Fn(Progress),
) -> Result<Report> {
    run_observed(options, synthetic, commands, on_progress, None)
}
/// Preview callbacks must be non-blocking; thumbnails are limited to two per second.
pub fn run_observed(
    options: &crate::RecordOptions,
    synthetic: bool,
    commands: mpsc::Receiver<Control>,
    on_progress: impl Fn(Progress),
    preview: Option<&dyn Fn(PreviewFrame)>,
) -> Result<Report> {
    validate_options(options)?;
    on_progress(Progress {
        state: "starting",
        active_seconds: 0.0,
        video_frames: 0,
        audio_peak: [0.0; 2],
        microphone_muted: false,
        system_muted: false,
    });
    let mut report=Report { mode:if synthetic {"synthetic"} else {"native"}.into(),
        platform:std::env::consts::OS.into(), status:"failed".into(),
        clock_note:"Áudio do sistema e vídeo usam o relógio nativo compartilhado. Microfone CPAL usa relógio monotônico do host com compensação do atraso de callback; alinhamento entre APIs ainda requer teste perceptivo em cada sistema.".into(),..Default::default() };
    let result = run_inner(
        options,
        synthetic,
        &mut report,
        commands,
        &on_progress,
        preview,
    );
    match result {
        Ok(()) => report.status = "completed".into(),
        Err(error) => report.error = Some(format!("{error:#}")),
    }
    let path = crate::encoder::sidecar(&options.output, ".capture.json");
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let file =
        std::fs::File::create_new(&path).context("O relatório já existe ou não pode ser criado")?;
    serde_json::to_writer_pretty(file, &report)?;
    Ok(report)
}

pub fn validate_options(options: &crate::RecordOptions) -> Result<()> {
    if !options.duration.is_finite() || !(0.1..=7200.0).contains(&options.duration) {
        bail!("Duração inválida");
    }
    if ![720, 1080].contains(&options.resolution)
        || options.microphone_volume > 100
        || options.system_volume > 100
    {
        bail!("Configuração de captura inválida");
    }
    if !["auto", "wayland", "x11", "wgc", "dxgi"].contains(&options.backend.as_str()) {
        bail!("Backend inválido");
    }
    if options.output.extension().and_then(|s| s.to_str()) != Some("mp4") {
        bail!("Use um arquivo .mp4");
    }
    for suffix in [
        "",
        ".partial.mp4",
        ".ffmpeg.log",
        ".capture.json",
        ".ffprobe.json",
    ] {
        if crate::encoder::sidecar(&options.output, suffix).exists() {
            bail!("O destino já existe. Escolha outro nome.");
        }
    }
    Ok(())
}

fn run_inner(
    options: &crate::RecordOptions,
    synthetic: bool,
    report: &mut Report,
    commands: mpsc::Receiver<Control>,
    on_progress: &impl Fn(Progress),
    preview: Option<&dyn Fn(PreviewFrame)>,
) -> Result<()> {
    let mut native = if synthetic {
        None
    } else {
        eprintln!("Solicitando captura. No Wayland, selecione a tela no diálogo do sistema.");
        let backend = match options.backend.as_str() {
            "auto" => BackendPreference::Auto,
            "x11" => BackendPreference::LinuxX11,
            "wayland" => BackendPreference::LinuxWaylandPortal,
            "wgc" => BackendPreference::WindowsWgc,
            "dxgi" => BackendPreference::WindowsDxgi,
            _ => unreachable!(),
        };
        let mut builder = CaptureSession::builder()
            .video_target(VideoCaptureTarget::Display(SourceId::new(&options.display)))
            .backend_preference(backend)
            .pixel_format(PixelFormat::Bgra8888)
            .frame_rate(Some(30))
            .queue_depth(4);
        if options.system_audio {
            builder = builder.audio(AudioCapture::SystemMix);
        }
        let mut native = Native(builder.build()?);
        report.backend = format!("{:?}", native.0.backend_info());
        native.0.start()?;
        Some(native)
    };
    if synthetic {
        report.backend = "synthetic BGRA + distinct 440/880 Hz tones (no hardware capture)".into();
    }
    let epoch = Instant::now();
    let mic = if options.microphone && !synthetic {
        Some(audio::Microphone::start(
            options.microphone_device.as_deref(),
            epoch,
        )?)
    } else {
        None
    };
    let mut tracks = Vec::new();
    let mut first_video = None;
    let mut first_audio = None;
    let mut clock = NativeClock { anchor: None };
    let dimensions = if let Some(native) = native.as_mut() {
        let deadline = Instant::now() + Duration::from_secs(20);
        while first_video.is_none() || (options.system_audio && first_audio.is_none()) {
            if commands.try_iter().any(|c| matches!(c, Control::Stop)) {
                bail!("Captura cancelada durante inicialização");
            }
            if Instant::now() >= deadline {
                bail!(
                    "Não chegaram vídeo/áudio solicitados em 20 s. Confira permissões e dispositivos."
                );
            }
            match native.0.next_event(Some(Duration::from_millis(20))) {
                Ok(CaptureEvent::Video(frame)) => {
                    let at = clock.map(frame.stream_time_ns, epoch.elapsed());
                    if first_video.is_none() {
                        first_video = Some((at, frame));
                    }
                }
                Ok(CaptureEvent::Audio(frame)) => {
                    let at = clock.map(frame.stream_time_ns, epoch.elapsed());
                    if first_audio.is_none() {
                        first_audio = Some((at, frame));
                    }
                }
                Ok(CaptureEvent::End) => bail!("Captura encerrada durante inicialização"),
                Err(pinray::PinrayError::Timeout(_)) => {}
                Err(error) => return Err(error.into()),
                _ => {}
            }
            // Do not accumulate microphone callbacks during slow portal/stream startup.
            if let Some(mic) = &mic {
                while mic.receiver.try_recv().is_ok() {}
            }
        }
        let frame = &first_video.as_ref().unwrap().1;
        (frame.width, frame.height)
    } else {
        (640, 360)
    };
    if options.system_audio {
        let (rate, channels) = first_audio
            .as_ref()
            .map(|(_, a)| (a.sample_rate, a.channels))
            .unwrap_or((48000, 2));
        tracks.push(AudioTrack {
            number: 2,
            rate,
            channels,
            name: "System",
        });
    }
    if options.microphone {
        tracks.push(AudioTrack {
            number: if options.system_audio { 3 } else { 2 },
            rate: mic.as_ref().map_or(48000, |m| m.rate),
            channels: mic.as_ref().map_or(1, |m| m.channels),
            name: "Microphone",
        });
    }
    let encoder = Encoder::start(
        &options.output,
        &options.ffmpeg,
        dimensions.0,
        dimensions.1,
        options.resolution,
        &tracks,
    )?;
    let mut meter = Meter::new(encoder.pid());
    let mut sink = Sink {
        encoder: Some(encoder),
        audio: tracks,
        timeline: Timeline::default(),
        limit: Duration::from_secs_f64(options.duration),
        report: std::mem::take(report),
        last_frame: None,
        last_video_pts: None,
        dimensions,
        last_audio_pts: [None, None],
        muted: [false, false],
        preview,
        preview_at: None,
        volumes: [
            if options.system_audio {
                options.system_volume
            } else {
                options.microphone_volume
            },
            options.microphone_volume,
        ],
    };
    let outcome = (|| -> Result<()> {
        if let Some((at, frame)) = first_video {
            sink.video(
                at,
                frame
                    .to_tight_bytes()
                    .context("Formato de vídeo não suportado")?,
            )?;
        }
        if let Some((at, frame)) = first_audio {
            sink.audio(2, at, &audio::normalize(&frame)?)?;
        }
        eprintln!(
            "Gravando: {}. Comandos + Enter: p (pausar), r (retomar), q (parar).",
            options.output.display()
        );
        let mut auto_paused = false;
        let mut resume_at = None;
        let mut next_sample = Duration::ZERO;
        let mut synthetic_index = 0u64;
        let mut next_progress = Duration::ZERO;
        let mut last_state = "";
        loop {
            let elapsed = epoch.elapsed();
            let mut stopped = false;
            for command in commands.try_iter() {
                match command {
                    Control::Pause => sink.timeline.pause(elapsed),
                    Control::Resume => sink.timeline.resume(elapsed),
                    Control::Stop => stopped = true,
                    Control::SetMicrophoneMuted(muted) => {
                        if options.microphone {
                            sink.muted[usize::from(options.system_audio)] = muted;
                        }
                    }
                    Control::SetSystemMuted(muted) => {
                        if options.system_audio {
                            sink.muted[0] = muted;
                        }
                    }
                }
            }
            if !auto_paused
                && options
                    .pause_after
                    .is_some_and(|s| elapsed.as_secs_f64() >= s)
            {
                auto_paused = true;
                sink.timeline.pause(elapsed);
                resume_at = Some(elapsed + Duration::from_secs_f64(options.pause_for));
            }
            if resume_at.is_some_and(|at| elapsed >= at) {
                sink.timeline.resume(elapsed);
                resume_at = None;
            }
            let active = sink.timeline.active(elapsed);
            if stopped || active >= sink.limit {
                break;
            }
            let state = if sink.timeline.paused() {
                "paused"
            } else {
                "recording"
            };
            if elapsed >= next_progress || state != last_state {
                on_progress(Progress {
                    state,
                    active_seconds: active.as_secs_f64(),
                    video_frames: sink.report.submitted_video_frames,
                    audio_peak: sink.report.audio_peak,
                    microphone_muted: options.microphone
                        && sink.muted[usize::from(options.system_audio)],
                    system_muted: options.system_audio && sink.muted[0],
                });
                next_progress = elapsed + Duration::from_millis(200);
                last_state = state;
            }
            if elapsed >= next_sample {
                sink.report
                    .samples
                    .push(meter.sample(elapsed.as_secs_f64()));
                next_sample = elapsed + Duration::from_secs(1);
            }
            if let Some(mic) = &mic {
                if mic.failed.load(Ordering::Relaxed) {
                    bail!("Microfone falhou ou a fila de áudio excedeu o limite");
                }
                for packet in mic.receiver.try_iter() {
                    sink.audio(
                        if options.system_audio { 3 } else { 2 },
                        packet.at,
                        &packet.data,
                    )?;
                }
            }
            if let Some(native) = native.as_mut() {
                match native.0.next_event(Some(Duration::from_millis(10))) {
                    Ok(CaptureEvent::Video(frame)) => {
                        if (frame.width, frame.height) != sink.dimensions
                            || frame.pixel_format != PixelFormat::Bgra8888
                        {
                            bail!("Formato/resolução da captura mudou; reinicie a gravação.");
                        }
                        let at = clock.map(frame.stream_time_ns, epoch.elapsed());
                        sink.video(
                            at,
                            frame
                                .to_tight_bytes()
                                .context("Quadro de vídeo sem bytes acessíveis")?,
                        )?;
                    }
                    Ok(CaptureEvent::Audio(frame)) => {
                        let track = &sink.audio[0];
                        if frame.sample_rate != track.rate || frame.channels != track.channels {
                            bail!("Formato de áudio mudou durante a captura");
                        }
                        let at = clock.map(frame.stream_time_ns, epoch.elapsed());
                        sink.audio(2, at, &audio::normalize(&frame)?)?;
                    }
                    Ok(CaptureEvent::Gap(gap)) => {
                        sink.report.native_gap_events += 1;
                        sink.report.native_reported_dropped_frames +=
                            u64::from(gap.dropped_frames.unwrap_or(0));
                    }
                    Ok(CaptureEvent::End) => {
                        bail!("Captura foi encerrada pelo sistema antes do término solicitado")
                    }
                    Err(pinray::PinrayError::Timeout(_)) => {}
                    Err(error) => return Err(error.into()),
                }
            } else {
                let due = Duration::from_nanos(synthetic_index * 1_000_000_000 / 30);
                if elapsed >= due {
                    sink.video(due, synthetic_frame(synthetic_index))?;
                    let tracks = sink.audio.clone();
                    for track in tracks {
                        let tone = synthetic_audio(
                            synthetic_index,
                            if track.name == "System" { 440.0 } else { 880.0 },
                            track.channels,
                        );
                        sink.audio(track.number, due, &tone)?;
                    }
                    synthetic_index += 1;
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            sink.heartbeat(epoch.elapsed())?;
        }
        let stopped_at = epoch.elapsed();
        let active = sink.timeline.active(stopped_at).min(sink.limit);
        drop(mic);
        if let Some(native) = native.as_mut() {
            native.0.stop()?;
        }
        on_progress(Progress {
            state: "finalizing",
            active_seconds: active.as_secs_f64(),
            video_frames: sink.report.submitted_video_frames,
            audio_peak: sink.report.audio_peak,
            microphone_muted: options.microphone && sink.muted[usize::from(options.system_audio)],
            system_muted: options.system_audio && sink.muted[0],
        });
        sink.finish_video(active)?;
        sink.report.active_seconds = active.as_secs_f64();
        sink.report.wall_seconds = stopped_at.as_secs_f64();
        sink.report
            .samples
            .push(meter.sample(sink.report.wall_seconds));
        sink.encoder.take().unwrap().finish()?;
        let probe =
            diagnostics::probe_metadata(&options.ffprobe, &options.output, !options.fast_finalize)?;
        let expected_audio = if sink.audio.len() == 2 {
            3
        } else {
            sink.audio.len()
        };
        sink.report.checks = Some(diagnostics::check(
            &probe,
            sink.report.active_seconds,
            expected_audio,
        )?);
        std::fs::write(
            crate::encoder::sidecar(&options.output, ".ffprobe.json"),
            serde_json::to_vec_pretty(&probe)?,
        )?;
        if !sink.report.checks.as_ref().unwrap().passed {
            bail!("MP4 produzido, mas checagens de duração/trilhas falharam; confira o relatório.");
        }
        Ok(())
    })();
    if sink.report.wall_seconds == 0.0 {
        sink.report.wall_seconds = epoch.elapsed().as_secs_f64();
        sink.report.active_seconds = sink
            .timeline
            .active(epoch.elapsed())
            .min(sink.limit)
            .as_secs_f64();
    }
    *report = sink.report;
    outcome
}

fn synthetic_frame(index: u64) -> Vec<u8> {
    let mut pixels = vec![0u8; 640 * 360 * 4];
    for (i, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = i % 640;
        let y = i / 640;
        pixel.copy_from_slice(&[
            (x % 256) as u8,
            (y % 256) as u8,
            if x.abs_diff((index as usize * 8) % 640) < 16 {
                255
            } else {
                32
            },
            255,
        ]);
    }
    pixels
}
fn synthetic_audio(index: u64, frequency: f64, channels: u16) -> Vec<u8> {
    let mut data = Vec::with_capacity(1600 * usize::from(channels) * 4);
    for i in 0..1600 {
        let value = (0.1
            * (std::f64::consts::TAU * frequency * (index * 1600 + i) as f64 / 48000.0).sin())
            as f32;
        for _ in 0..channels {
            data.extend_from_slice(&value.to_le_bytes());
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_preserves_source_aspect_and_bgra_colors_without_upscaling() {
        let frame = preview_frame(&[10, 20, 30, 255].repeat(6), 3, 2).unwrap();
        assert_eq!(
            (
                frame.width,
                frame.height,
                frame.source_width,
                frame.source_height
            ),
            (3, 2, 3, 2)
        );
        assert_eq!(frame.rgb, [30, 20, 10].repeat(6));
        assert!(preview_frame(&[], 3, 2).is_none());
        assert!(preview_frame(&[], 0, 0).is_none());
        let frame = preview_frame(&[0; 4].repeat(1920 * 1080), 1920, 1080).unwrap();
        assert_eq!((frame.width, frame.height), (480, 270));
    }
    #[test]
    fn audio_packet_straddling_pause_is_cut_at_sample_boundaries() {
        let mut t = Timeline::default();
        t.pause(Duration::from_millis(3));
        t.resume(Duration::from_millis(7));
        let data: Vec<u8> = (0..10).flat_map(|s| (s as f32).to_le_bytes()).collect();
        let packets = audio_packets(
            2,
            Duration::ZERO,
            1000,
            1,
            &data,
            &t,
            Duration::from_secs(1),
        );
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].data.len(), 3 * 4);
        assert_eq!(packets[1].data.len(), 3 * 4);
        assert_eq!(packets[1].pts_ms, 3);
    }
}
