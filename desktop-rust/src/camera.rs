//! Camera frames are decoded by FFmpeg into a bounded BGRA queue. No screen is opened.
use anyhow::{Context, Result, bail};
use pinray::{
    AudioCapture, BackendPreference, CaptureEvent, CaptureSession, FrameData, PixelFormat,
    VideoFrame,
};
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
#[derive(Clone, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
}
fn command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
pub fn devices(ffmpeg: &Path) -> Result<Vec<Device>> {
    #[cfg(target_os = "linux")]
    {
        let _ = ffmpeg;
        let mut cameras = Vec::new();
        if let Ok(entries) = fs::read_dir("/sys/class/video4linux") {
            for entry in entries.flatten() {
                let node = entry.file_name().to_string_lossy().into_owned();
                if node
                    .strip_prefix("video")
                    .is_some_and(|n| n.parse::<u32>().is_ok())
                {
                    let name = fs::read_to_string(entry.path().join("name"))
                        .unwrap_or_else(|_| node.clone())
                        .trim()
                        .to_string();
                    cameras.push(Device {
                        id: format!("/dev/{node}"),
                        name: format!("{name} · {node}"),
                    });
                }
            }
        }
        cameras.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(cameras)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut cmd = command(ffmpeg);
        cmd.args(["-hide_banner", "-nostdin", "-list_devices", "true", "-f"]);
        #[cfg(target_os = "windows")]
        cmd.args(["dshow", "-i", "dummy"]);
        #[cfg(target_os = "macos")]
        cmd.args(["avfoundation", "-i", ""]);
        let output = crate::process::output(&mut cmd, Duration::from_secs(5))
            .context("Não foi possível listar câmeras pelo FFmpeg")?;
        Ok(parse_devices(
            &String::from_utf8_lossy(&output.stderr),
            cfg!(target_os = "macos"),
        ))
    }
}
#[cfg(any(test, not(target_os = "linux")))]
fn parse_devices(text: &str, macos: bool) -> Vec<Device> {
    let mut devices = Vec::new();
    let mut video = !macos;
    for line in text.lines() {
        if !macos && line.contains("(audio)") {
            video = false;
            continue;
        }
        if !macos && line.contains("(video)") {
            video = true;
        }
        if macos {
            if line.contains("video devices:") {
                video = true;
                continue;
            }
            if line.contains("audio devices:") {
                video = false;
            }
            if !video || line.contains("Capture screen") {
                continue;
            }
            if let Some((head, name)) = line.rsplit_once("] ")
                && let Some((_, index)) = head.rsplit_once('[')
                && index.parse::<u32>().is_ok()
            {
                devices.push(Device {
                    id: index.into(),
                    name: name.trim().into(),
                });
            }
        } else if video && line.contains("Alternative name") {
            if let Some((_, rest)) = line.split_once('"')
                && let Some((id, _)) = rest.split_once('"')
                && let Some(device) = devices.last_mut()
            {
                device.id = id.into();
            }
        } else if line.contains("(video)")
            && let Some((_, rest)) = line.split_once('"')
            && let Some((name, _)) = rest.split_once('"')
        {
            devices.push(Device {
                id: name.into(),
                name: name.into(),
            });
        }
    }
    devices
}
pub struct Camera {
    child: Child,
    reader: Option<JoinHandle<()>>,
    receiver: mpsc::Receiver<Result<VideoFrame, String>>,
    audio: Option<CaptureSession>,
    alternate: bool,
}
impl Camera {
    pub fn start(options: &crate::RecordOptions, backend: BackendPreference) -> Result<Self> {
        Self::start_device(options, backend, None)
    }
    fn start_device(
        options: &crate::RecordOptions,
        backend: BackendPreference,
        square: Option<u32>,
    ) -> Result<Self> {
        if !devices(&options.ffmpeg)?
            .iter()
            .any(|camera| camera.id == options.display)
        {
            bail!("Escolha uma câmera disponível antes de gravar.");
        }
        let mut cmd = command(&options.ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        #[cfg(target_os = "linux")]
        cmd.args(["-f", "v4l2", "-i", &options.display]);
        #[cfg(target_os = "windows")]
        cmd.args(["-f", "dshow", "-i", &format!("video={}", options.display)]);
        #[cfg(target_os = "macos")]
        cmd.args([
            "-f",
            "avfoundation",
            "-framerate",
            "30",
            "-i",
            &format!("{}:none", options.display),
        ]);
        Self::from_input(cmd, options, backend, square)
    }
    pub fn start_webcam(options: &crate::RecordOptions, device: &str) -> Result<Self> {
        let mut options = options.clone();
        options.display = device.into();
        options.system_audio = false;
        Self::start_device(&options, BackendPreference::Auto, Some(320))
    }
    fn from_input(
        mut cmd: Command,
        options: &crate::RecordOptions,
        backend: BackendPreference,
        square: Option<u32>,
    ) -> Result<Self> {
        let height = square.unwrap_or(options.resolution);
        let width = square.unwrap_or(height / 9 * 16);
        let filter = if square.is_some() {
            format!(
                "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
            )
        } else {
            format!(
                "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
            )
        };
        cmd.args([
            "-an", "-vf", &filter, "-r", "30", "-pix_fmt", "bgra", "-threads", "1", "-f",
            "rawvideo", "pipe:1",
        ]);
        let log_path = crate::encoder::sidecar(&options.output, ".camera.log");
        let log = if square.is_some() {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)?
        } else {
            fs::File::create_new(log_path)?
        };
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .stdin(Stdio::null())
            .spawn()
            .context("Não foi possível abrir a câmera")?;
        let mut stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::sync_channel(2);
        let reader = std::thread::spawn(move || {
            let epoch = Instant::now();
            let mut sequence = 0;
            loop {
                let mut bytes = vec![0; (width * height * 4) as usize];
                if stdout.read_exact(&mut bytes).is_err() {
                    let _=sender.try_send(Err("A câmera foi encerrada. Confira a permissão, o dispositivo selecionado e se outro aplicativo está usando a câmera.".into()));
                    break;
                }
                sequence += 1;
                let frame = VideoFrame {
                    stream_time_ns: epoch.elapsed().as_nanos().min(i64::MAX as u128) as i64,
                    sequence,
                    width,
                    height,
                    stride: width * 4,
                    pixel_format: PixelFormat::Bgra8888,
                    color_space: None,
                    data: FrameData::Host(bytes),
                    damage: None,
                };
                if let Err(mpsc::TrySendError::Disconnected(_)) = sender.try_send(Ok(frame)) {
                    break;
                }
            }
        });
        let mut camera = Self {
            child,
            reader: Some(reader),
            receiver,
            audio: None,
            alternate: false,
        };
        if options.system_audio {
            let mut audio = CaptureSession::builder()
                .audio(AudioCapture::SystemMix)
                .backend_preference(backend)
                .queue_depth(8)
                .build()?;
            audio.start()?;
            camera.audio = Some(audio);
        }
        Ok(camera)
    }
    pub fn next_event(
        &mut self,
        timeout: Option<Duration>,
    ) -> std::result::Result<CaptureEvent, pinray::PinrayError> {
        self.alternate = !self.alternate;
        if self.alternate
            && let Some(audio) = &mut self.audio
        {
            match audio.next_event(Some(Duration::ZERO)) {
                Err(pinray::PinrayError::Timeout(_)) => {}
                result => return result,
            }
        }
        match self
            .receiver
            .recv_timeout(timeout.unwrap_or(Duration::from_millis(10)))
        {
            Ok(Ok(frame)) => Ok(CaptureEvent::Video(frame)),
            Ok(Err(error)) => Err(pinray::PinrayError::Platform(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(pinray::PinrayError::Timeout(
                timeout.unwrap_or(Duration::from_millis(10)),
            )),
            Err(_) => Err(pinray::PinrayError::Platform(
                "A câmera foi desconectada.".into(),
            )),
        }
    }
    pub fn stop(&mut self) -> std::result::Result<(), pinray::PinrayError> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(audio) = &mut self.audio {
            audio.stop()?;
        }
        Ok(())
    }
}
impl Drop for Camera {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_and_square_webcam_decode_pixels_and_release_the_child() {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Arguments {
            #[command(flatten)]
            options: crate::RecordOptions,
        }
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("camera.mp4");
        let options = Arguments::parse_from(["test", "--output", output.to_str().unwrap()]).options;
        for (square, source, sessions) in [(None, "320x240", 1), (Some(320), "640x360", 2)] {
            for _ in 0..sessions {
                let mut cmd = command(&options.ffmpeg);
                cmd.args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-nostdin",
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                ])
                .arg(format!("color=blue:s={source}:r=30"));
                let mut camera =
                    Camera::from_input(cmd, &options, BackendPreference::Auto, square).unwrap();
                let CaptureEvent::Video(frame) =
                    camera.next_event(Some(Duration::from_secs(5))).unwrap()
                else {
                    panic!("Expected camera frame: square={square:?}")
                };
                let bytes = frame.to_tight_bytes().unwrap();
                let pixel = if square.is_some() {
                    assert_eq!((frame.width, frame.height), (320, 320));
                    &bytes[..3]
                } else {
                    assert_eq!((frame.width, frame.height), (1280, 720));
                    assert_eq!(&bytes[..3], &[0, 0, 0], "letterbox must stay black");
                    &bytes[(360 * 1280 + 640) * 4..][..3]
                };
                assert!(
                    pixel[0] > 240 && pixel[1] < 10 && pixel[2] < 10,
                    "{square:?}: {pixel:?}"
                );
                camera.stop().unwrap();
                assert!(camera.child.try_wait().unwrap().is_some());
                assert!(camera.reader.is_none());
            }
        }
    }
    #[test]
    fn parses_video_devices_without_audio_or_screen_devices() {
        let win = parse_devices(
            "[dshow] \"USB Camera\" (video)\n[dshow] \"Microfone\" (audio)",
            false,
        );
        assert_eq!(win.len(), 1);
        assert_eq!(win[0].id, "USB Camera");
        let alternatives = parse_devices(
            "[dshow] \"USB Camera\" (video)\n[dshow] Alternative name \"camera-device\"\n[dshow] \"Microphone\" (audio)\n[dshow] Alternative name \"audio-device\"",
            false,
        );
        assert_eq!(alternatives[0].id, "camera-device");
        let mac = parse_devices(
            "[AVFoundation @ x] AVFoundation video devices:\n[AVFoundation @ x] [0] FaceTime HD\n[AVFoundation @ x] [1] Capture screen 0\n[AVFoundation @ x] AVFoundation audio devices:\n[AVFoundation @ x] [0] Microphone",
            true,
        );
        assert_eq!(mac.len(), 1);
        assert_eq!(mac[0].id, "0");
    }
}
