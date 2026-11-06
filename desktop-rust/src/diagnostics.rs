use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{path::Path, process::Command};
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Serialize)]
pub struct Sample {
    pub wall_seconds: f64,
    pub rust_rss_bytes: u64,
    pub ffmpeg_rss_bytes: u64,
    /// sysinfo CPU percentage: 100% = one logical core, may exceed 100%.
    pub cpu_percent_one_core: f32,
}
pub struct Meter {
    system: System,
    ids: [Pid; 2],
}
impl Meter {
    pub fn new(encoder_pid: u32) -> Self {
        Self {
            system: System::new(),
            ids: [
                Pid::from_u32(std::process::id()),
                Pid::from_u32(encoder_pid),
            ],
        }
    }
    pub fn sample(&mut self, wall_seconds: f64) -> Sample {
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&self.ids), true);
        let rss = |id| self.system.process(id).map_or(0, |p| p.memory());
        Sample {
            wall_seconds,
            rust_rss_bytes: rss(self.ids[0]),
            ffmpeg_rss_bytes: rss(self.ids[1]),
            cpu_percent_one_core: self
                .ids
                .iter()
                .filter_map(|id| self.system.process(*id))
                .map(|p| p.cpu_usage())
                .sum(),
        }
    }
}

pub fn probe(ffprobe: &Path, file: &Path) -> Result<Value> {
    probe_metadata(ffprobe, file, true)
}
pub fn probe_metadata(ffprobe: &Path, file: &Path, count_frames: bool) -> Result<Value> {
    probe_with_timeout(
        ffprobe,
        file,
        count_frames,
        std::time::Duration::from_secs(if count_frames { 600 } else { 30 }),
    )
}
fn probe_with_timeout(
    ffprobe: &Path,
    file: &Path,
    count_frames: bool,
    timeout: std::time::Duration,
) -> Result<Value> {
    let mut command = Command::new(ffprobe);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    if count_frames {
        command.arg("-count_frames");
    }
    command
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(file);
    let output = crate::process::output(&mut command, timeout)
        .context("Não foi possível concluir a inspeção ffprobe no tempo limite")?;
    if !output.status.success() {
        bail!("ffprobe: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn number(v: &Value, key: &str) -> Option<f64> {
    v[key].as_str()?.parse().ok()
}
#[derive(Debug, Serialize)]
pub struct Checks {
    pub video_frames: u64,
    pub video_duration_seconds: f64,
    pub actual_file_fps: f64,
    pub max_audio_video_end_difference_seconds: f64,
    pub duration_error_seconds: f64,
    pub expected_audio_tracks: usize,
    pub actual_audio_tracks: usize,
    pub passed: bool,
    pub limitation: &'static str,
}
pub fn check(probe: &Value, expected_duration: f64, expected_audio: usize) -> Result<Checks> {
    let streams = probe["streams"].as_array().context("ffprobe sem streams")?;
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .context("Arquivo sem vídeo")?;
    let frames = video["nb_read_frames"]
        .as_str()
        .or_else(|| video["nb_frames"].as_str())
        .and_then(|s| s.parse().ok())
        .context("ffprobe sem contagem de quadros")?;
    let duration = number(video, "duration").context("Vídeo sem duração")?;
    let video_end = number(video, "start_time").unwrap_or(0.0) + duration;
    let audio: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .collect();
    let end_difference = audio
        .iter()
        .map(|a| {
            number(a, "duration")
                .map(|d| (number(a, "start_time").unwrap_or(0.0) + d - video_end).abs())
                .unwrap_or(f64::INFINITY)
        })
        .fold(0.0, f64::max);
    let duration_error = (video_end - expected_duration).abs();
    Ok(Checks {
        video_frames: frames,
        video_duration_seconds: duration,
        actual_file_fps: frames as f64 / duration,
        max_audio_video_end_difference_seconds: end_difference,
        duration_error_seconds: duration_error,
        expected_audio_tracks: expected_audio,
        actual_audio_tracks: audio.len(),
        passed: frames > 0
            && duration > 0.0
            && duration_error <= 0.25
            && end_difference <= 0.25
            && audio.len() == expected_audio,
        limitation: "Checagens técnicas de duração e trilhas; contagem obtida por decodificação ou metadados do contêiner; não comprovam audibilidade, sincronização perceptiva, fluidez ou estabilidade de memória em teste prolongado.",
    })
}

#[cfg(all(test, target_os = "windows"))]
mod timeout_tests {
    use super::*;
    #[test]
    fn hung_probe_is_terminated_without_blocking_finalization() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("slow-probe.cmd");
        std::fs::write(
            &program,
            b"@echo off
:wait
goto wait
",
        )
        .unwrap();
        let start = std::time::Instant::now();
        let error = probe_with_timeout(
            &program,
            &dir.path().join("capture.mp4"),
            false,
            std::time::Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("excedeu o tempo limite"));
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }
}
