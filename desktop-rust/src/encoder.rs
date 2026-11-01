use crate::matroska::{self, AudioTrack};
use anyhow::{Context, Result, bail};
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Packet {
    pub track: u8,
    pub pts_ms: u64,
    pub data: Vec<u8>,
}
pub struct Encoder {
    child: Child,
    sender: Option<SyncSender<Packet>>,
    writer: Option<JoinHandle<Result<()>>>,
    partial: PathBuf,
    output: PathBuf,
    pending_video: Arc<AtomicUsize>,
}
pub fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
impl Encoder {
    pub fn start(
        output: &Path,
        ffmpeg: &Path,
        width: u32,
        height: u32,
        resolution: u32,
        audio: &[AudioTrack],
        crop: Option<pinray::Rect>,
    ) -> Result<Self> {
        let partial = sidecar(output, ".partial.mp4");
        if output.exists() || partial.exists() {
            bail!(
                "O destino ou arquivo parcial já existe: {}",
                output.display()
            );
        }
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let log = File::create_new(sidecar(output, ".ffmpeg.log"))?;
        let mut cmd = Command::new(ffmpeg);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-nostdin",
            "-n",
            "-f",
            "matroska",
            "-i",
            "pipe:0",
        ]);
        cmd.args(["-map", "0:v:0"]);
        if audio.len() == 2 {
            cmd.args(["-filter_complex","[0:a:0]aresample=async=1000:first_pts=0,asplit=2[s][skeep];[0:a:1]aresample=async=1000:first_pts=0,asplit=2[m][mkeep];[s][m]amix=inputs=2:duration=longest:normalize=1[mix]",
                "-map","[mix]","-map","[skeep]","-map","[mkeep]",
                "-metadata:s:a:0","title=Mix","-metadata:s:a:1","title=System","-metadata:s:a:2","title=Microphone"]);
        } else if audio.len() == 1 {
            cmd.args(["-map", "0:a:0", "-af", "aresample=async=1000:first_pts=0"]);
        } else {
            cmd.arg("-an");
        }
        // amix/asplit can negotiate stereo for both branches. Keep the isolated
        // voice track explicitly mono even when the mixed track is stereo.
        if audio.iter().any(|track| track.name == "Microphone") {
            let microphone_index = if audio.len() == 2 { 2 } else { 0 };
            cmd.arg(format!("-ac:a:{microphone_index}")).arg("1");
        }
        cmd.args([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-threads",
            "2",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "vfr",
            "-vf",
        ])
        .arg(match crop {
            Some(rect) => format!(
                "crop={}:{}:{}:{}:exact=1,scale=-2:{resolution}",
                rect.width, rect.height, rect.x, rect.y
            ),
            None => format!("scale=-2:{resolution}"),
        })
        .args([
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-ar",
            "48000",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ])
        .arg(&partial)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
        let mut child = cmd
            .spawn()
            .context("Não foi possível iniciar FFmpeg. Instale-o e confira --ffmpeg.")?;
        let pipe = child.stdin.take().context("FFmpeg sem stdin")?;
        let audio = audio.to_vec();
        // Audio callbacks can arrive in bursts during FFmpeg startup. Keep a larger
        // packet queue while independently capping expensive video frames at four.
        let (sender, receiver) = mpsc::sync_channel::<Packet>(256);
        let pending_video = Arc::new(AtomicUsize::new(0));
        let pending = pending_video.clone();
        let writer = thread::spawn(move || {
            let mut pipe = BufWriter::new(pipe);
            matroska::header(&mut pipe, width, height, &audio)?;
            for packet in receiver {
                matroska::packet(&mut pipe, packet.track, packet.pts_ms, &packet.data)?;
                if packet.track == 1 {
                    pending.fetch_sub(1, Ordering::Relaxed);
                }
            }
            std::io::Write::flush(&mut pipe)?;
            Ok(())
        });
        Ok(Self {
            child,
            sender: Some(sender),
            writer: Some(writer),
            partial,
            output: output.to_owned(),
            pending_video,
        })
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    /// Drop video under pressure, but never silently discard audio.
    pub fn send(&self, packet: Packet) -> Result<bool> {
        // A failed writer may leave its video counter full. Check it before
        // treating that state as normal backpressure in a video-only recording.
        if self
            .writer
            .as_ref()
            .is_none_or(|writer| writer.is_finished())
        {
            bail!("O transporte para FFmpeg encerrou; confira o arquivo .ffmpeg.log.");
        }
        let video = packet.track == 1;
        if video && self.pending_video.fetch_add(1, Ordering::Relaxed) >= 4 {
            self.pending_video.fetch_sub(1, Ordering::Relaxed);
            return Ok(false);
        }
        let result = self
            .sender
            .as_ref()
            .context("Encoder encerrado")?
            .try_send(packet);
        if video && result.is_err() {
            self.pending_video.fetch_sub(1, Ordering::Relaxed);
        }
        match result {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(p)) if p.track == 1 => Ok(false),
            Err(TrySendError::Full(_)) => bail!(
                "Encoder não acompanha o áudio; gravação interrompida para evitar perda silenciosa."
            ),
            Err(TrySendError::Disconnected(_)) => {
                bail!("FFmpeg encerrou; confira o arquivo .ffmpeg.log.")
            }
        }
    }
    pub fn finish(mut self) -> Result<()> {
        self.sender.take();
        // A crashed/hung encoder must not prevent finalization forever.
        let deadline = Instant::now() + Duration::from_secs(60);
        while !self.writer.as_ref().unwrap().is_finished() {
            if Instant::now() >= deadline {
                bail!("FFmpeg excedeu 60 s ao finalizar; arquivo parcial preservado.");
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.writer
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow::anyhow!("Falha na thread do encoder"))??;
        loop {
            if let Some(status) = self.child.try_wait()? {
                if !status.success() {
                    bail!("FFmpeg falhou ({status}); confira o arquivo .ffmpeg.log.");
                }
                break;
            }
            if Instant::now() >= deadline {
                bail!("FFmpeg excedeu 60 s ao finalizar.");
            }
            thread::sleep(Duration::from_millis(20));
        }
        // Atomic no-clobber publication on the same filesystem.
        std::fs::hard_link(&self.partial, &self.output)
            .context("Não foi possível publicar o MP4 sem sobrescrever o destino")?;
        std::fs::remove_file(&self.partial)?;
        Ok(())
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        self.sender.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}
