pub mod audio;
pub mod camera;
pub mod diagnostics;
pub mod encoder;
mod matroska;
pub mod recording;
mod timeline;
mod webcam;

use clap::Args;
use std::path::PathBuf;

#[derive(Args, Clone, Debug)]
pub struct RecordOptions {
    #[arg(long)]
    pub output: PathBuf,
    /// Finaliza usando metadados do MP4, sem decodificar todos os quadros.
    #[arg(long)]
    pub fast_finalize: bool,
    /// Segundos ativos; pausas são excluídas (máximo de duas horas).
    #[arg(long,default_value="10",value_parser=positive_seconds)]
    pub duration: f64,
    /// Volume de captura, entre 0 e 100%.
    #[arg(long, default_value="100", value_parser=clap::value_parser!(u8).range(0..=100))]
    pub system_volume: u8,
    #[arg(long, default_value="100", value_parser=clap::value_parser!(u8).range(0..=100))]
    pub microphone_volume: u8,
    #[arg(long)]
    pub system_audio: bool,
    #[arg(long)]
    pub microphone: bool,
    /// Nome exato retornado por sources; sem esta opção usa o microfone padrão.
    #[arg(long, requires = "microphone")]
    pub microphone_device: Option<String>,
    #[arg(long, default_value = "auto")]
    pub display: String,
    #[arg(long, default_value="screen", value_parser=["screen","window","area","camera"])]
    pub capture_mode: String,
    /// Recorte em pixels da fonte: x,y,largura,altura. Sem valor, a GUI pede a seleção.
    #[arg(long)]
    pub crop: Option<String>,
    /// Camera device embedded over screen, window or area capture.
    #[arg(long)]
    pub webcam: Option<String>,
    #[arg(long)]
    pub webcam_disabled: bool,
    /// Desktop preview is already present in the screen capture (Linux).
    #[arg(long, hide = true)]
    pub webcam_visible_in_capture: bool,
    #[arg(long, default_value_t = 0.0)]
    pub webcam_x: f64,
    #[arg(long, default_value_t = 1.0)]
    pub webcam_y: f64,
    #[arg(long,default_value="auto",value_parser=["auto","wayland","x11","wgc","dxgi"])]
    pub backend: String,
    #[arg(long,default_value="720",value_parser=parse_resolution)]
    pub resolution: u32,
    #[arg(long, default_value = "ffmpeg")]
    pub ffmpeg: PathBuf,
    #[arg(long, default_value = "ffprobe")]
    pub ffprobe: PathBuf,
    /// Pausa automática de diagnóstico após N segundos de relógio.
    #[arg(long,value_parser=positive_seconds)]
    pub pause_after: Option<f64>,
    #[arg(long,default_value="2",value_parser=positive_seconds)]
    pub pause_for: f64,
}
fn positive_seconds(value: &str) -> std::result::Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.1 && *n <= 7200.0)
        .ok_or("Informe segundos entre 0.1 e 7200".into())
}
fn parse_resolution(value: &str) -> std::result::Result<u32, String> {
    match value {
        "720" => Ok(720),
        "1080" => Ok(1080),
        _ => Err("Use 720 ou 1080".into()),
    }
}
