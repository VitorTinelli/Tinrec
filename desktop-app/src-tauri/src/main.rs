#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cloud;
mod screenshots;
mod shortcuts;
mod tray;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tinrec_capture::{
    RecordOptions,
    recording::{self, Control},
};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    revision: u64,
    session_id: Option<String>,
    phase: String,
    elapsed_seconds: f64,
    video_frames: u64,
    audio_peak: [f32; 2],
    file_name: Option<String>,
    error: Option<String>,
    microphone_enabled: bool,
    system_enabled: bool,
    microphone_muted: bool,
    system_muted: bool,
    webcam_available: bool,
    webcam_enabled: bool,
}
impl Snapshot {
    fn busy(&self) -> bool {
        matches!(
            self.phase.as_str(),
            "starting" | "recording" | "paused" | "stopping" | "finalizing" | "discarding"
        )
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    session_id: String,
    sequence: u64,
    image: String,
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
}

struct Session {
    snapshot: Snapshot,
    controls: Option<mpsc::Sender<Control>>,
    id: String,
    exit_requested: bool,
    discard_requested: bool,
    preview: Option<Preview>,
    capture_mode: String,
    webcam_preview: Option<Preview>,
}
struct AppState {
    session: Mutex<Session>,
    folder: Mutex<PathBuf>,
    config: PathBuf,
    webcam_position: Mutex<(f64, f64)>,
    webcam_region: Mutex<Option<(f64, f64, f64, f64)>>,
    cloud: Arc<cloud::Cloud>,
    exiting: AtomicBool,
}
type Shared = Arc<AppState>;
type Reply<T> = Result<T, String>;

fn synthetic_enabled() -> bool {
    cfg!(debug_assertions) && std::env::var_os("TINREC_TEST_SYNTHETIC").is_some()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Display {
    id: String,
    name: String,
    width: u32,
    height: u32,
}
#[derive(Serialize)]
struct NamedSource {
    id: String,
    name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sources {
    synthetic: bool,
    platform: String,
    wayland: bool,
    displays: Vec<Display>,
    windows: Vec<NamedSource>,
    microphones: Vec<String>,
    warnings: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tools {
    ffmpeg: Option<String>,
    ffprobe: Option<String>,
    ready: bool,
    message: Option<String>,
}
fn default_capture_mode() -> String {
    "screen".into()
}
fn default_keep_local() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartRequest {
    #[serde(default = "default_keep_local")]
    keep_local: bool,
    #[serde(default = "default_capture_mode")]
    capture_mode: String,
    #[serde(default)]
    window: String,
    #[serde(default)]
    camera: String,
    #[serde(default)]
    webcam_enabled: bool,
    title: String,
    display: String,
    microphone_device: Option<String>,
    microphone: bool,
    system_audio: bool,
    microphone_volume: u8,
    system_volume: u8,
    resolution: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingFile {
    file_name: String,
    title: String,
    bytes: u64,
    modified_ms: u64,
    duration: Option<f64>,
    status: String,
}

fn lock<T>(mutex: &Mutex<T>) -> Reply<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| "Não foi possível acessar o estado do gravador. Reinicie o aplicativo.".into())
}
fn snapshot_value(state: &AppState) -> Reply<Snapshot> {
    Ok(lock(&state.session)?.snapshot.clone())
}
fn emit(app: &tauri::AppHandle, snapshot: &Snapshot) {
    let _ = app.emit("recorder-state", snapshot);
    if snapshot.phase != "starting"
        && let Some(window) = app.get_webview_window("area")
    {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("controls") {
        if snapshot.busy() && snapshot.phase != "starting" {
            if !window.is_visible().unwrap_or(false) {
                let _ = window.show();
            }
        } else {
            let _ = window.hide();
        }
    }
    if let Some(window) = app.get_webview_window("webcam") {
        if matches!(snapshot.phase.as_str(), "recording" | "paused") && snapshot.webcam_enabled {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
}
fn portal_handles_area(linux: bool, wayland: bool, mode: &str) -> bool {
    linux && wayland && mode == "area"
}
fn source_matches_monitor(
    width: u32,
    height: u32,
    scale: f64,
    source_width: u32,
    source_height: u32,
) -> bool {
    let matches = |w: f64, h: f64| {
        (w - f64::from(source_width)).abs() <= 1. && (h - f64::from(source_height)).abs() <= 1.
    };
    matches(f64::from(width), f64::from(height))
        || (scale.is_finite()
            && scale > 0.
            && matches(f64::from(width) / scale, f64::from(height) / scale))
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn safe_title(value: &str) -> String {
    let mut bytes = 0;
    let clean: String = value
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(90)
        .take_while(|c| {
            bytes += c.len_utf8();
            bytes <= 160
        })
        .collect();
    let clean = clean.trim().trim_matches('.').trim();
    if clean.is_empty() {
        "Gravação".into()
    } else {
        clean.into()
    }
}
fn native_command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
fn tool_path(app: &tauri::AppHandle, name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(format!("TINREC_{}", name.to_uppercase())) {
        return PathBuf::from(path);
    }
    let file = if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let mut folders = Vec::new();
    if let Ok(path) = std::env::current_exe()
        && let Some(parent) = path.parent()
    {
        folders.push(parent.to_owned());
    }
    if let Ok(path) = app.path().resource_dir() {
        folders.push(path.join("bin"));
    }
    #[cfg(target_os = "macos")]
    folders.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    folders
        .into_iter()
        .map(|p| p.join(&file))
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(file))
}
fn version(path: &Path) -> Reply<String> {
    let out = native_command(path)
        .arg("-version")
        .output()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !out.status.success() {
        return Err(format!("Não foi possível executar {}", path.display()));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .into())
}
fn check_tools(app: &tauri::AppHandle) -> Tools {
    let ffmpeg = version(&tool_path(app, "ffmpeg")).ok();
    let ffprobe = version(&tool_path(app, "ffprobe")).ok();
    let x264 = ffmpeg.is_some()
        && native_command(&tool_path(app, "ffmpeg"))
            .args(["-hide_banner", "-encoders"])
            .output()
            .is_ok_and(|o| {
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("libx264")
            });
    let ready = ffmpeg.is_some() && ffprobe.is_some() && x264;
    Tools {
        ffmpeg,
        ffprobe,
        ready,
        message: if ready {
            None
        } else {
            Some("Instale FFmpeg com libx264 e ffprobe. Eles devem estar no PATH ou ao lado do executável do Tinrec.".into())
        },
    }
}

#[tauri::command]
fn get_snapshot(state: State<'_, Shared>) -> Reply<Snapshot> {
    snapshot_value(&state)
}
#[tauri::command]
async fn get_tools(app: tauri::AppHandle) -> Reply<Tools> {
    tauri::async_runtime::spawn_blocking(move || check_tools(&app))
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn get_sources() -> Reply<Sources> {
    tauri::async_runtime::spawn_blocking(|| {
        if synthetic_enabled() {
            return Sources {
                synthetic: true,
                platform: std::env::consts::OS.into(),
                wayland: false,
                displays: Vec::new(),
                windows: Vec::new(),
                microphones: Vec::new(),
                warnings: Vec::new(),
            };
        }
        let mut warnings = Vec::new();
        let mut displays = Vec::new();
        let mut windows = Vec::new();
        match pinray::enumerate_sources() {
            Ok(sources) => {
                for source in sources {
                    match source {
                        pinray::CaptureSource::Display(display) => displays.push(Display {
                            id: display.id.0,
                            name: display.name,
                            width: display.width,
                            height: display.height,
                        }),
                        pinray::CaptureSource::Window(window)
                            if !window.title.trim().is_empty() =>
                        {
                            windows.push(NamedSource {
                                id: window.id.0,
                                name: window.title,
                            })
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => warnings.push(format!("Não foi possível listar fontes: {error}")),
        }
        let microphones = match tinrec_capture::audio::list() {
            Ok(m) => m,
            Err(e) => {
                warnings.push(format!("Não foi possível listar microfones: {e}"));
                Vec::new()
            }
        };
        Sources {
            synthetic: false,
            platform: std::env::consts::OS.into(),
            wayland: std::env::var_os("WAYLAND_DISPLAY").is_some(),
            displays,
            windows,
            microphones,
            warnings,
        }
    })
    .await
    .map_err(|e| e.to_string())
}
#[tauri::command]
async fn get_cameras(app: tauri::AppHandle) -> Reply<Vec<tinrec_capture::camera::Device>> {
    tauri::async_runtime::spawn_blocking(move || {
        tinrec_capture::camera::devices(&tool_path(&app, "ffmpeg")).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn select_area(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    session_id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Reply<()> {
    let session = lock(&state.session)?;
    if session.id != session_id
        || session.capture_mode != "area"
        || session.snapshot.phase != "starting"
    {
        return Err("Seleção de área encerrada.".into());
    }
    let preview = session
        .preview
        .as_ref()
        .ok_or("A imagem para seleção ainda não chegou.")?;
    if x < 0
        || y < 0
        || width < 2
        || height < 2
        || x as u64 + width as u64 > preview.source_width as u64
        || y as u64 + height as u64 > preview.source_height as u64
    {
        return Err("Selecione uma área dentro da imagem.".into());
    }
    *lock(&state.webcam_region)? = Some((
        f64::from(x) / f64::from(preview.source_width),
        f64::from(y) / f64::from(preview.source_height),
        f64::from(width) / f64::from(preview.source_width),
        f64::from(height) / f64::from(preview.source_height),
    ));
    let sender = session.controls.clone().ok_or("Captura encerrada")?;
    drop(session);
    position_webcam_window(&app);
    if let Some(window) = app.get_webview_window("area") {
        window.hide().map_err(|e| e.to_string())?;
    }
    // Let the compositor remove the overlay before the capture clock starts.
    std::thread::sleep(std::time::Duration::from_millis(100));
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    sender
        .send(Control::SelectArea(pinray::Rect {
            x,
            y,
            width,
            height,
        }))
        .map_err(|_| "Captura encerrada".to_string())
}
#[tauri::command]
fn get_folder(state: State<'_, Shared>) -> Reply<String> {
    Ok(lock(&state.folder)?.to_string_lossy().into())
}
#[tauri::command]
async fn choose_folder(app: tauri::AppHandle, state: State<'_, Shared>) -> Reply<Option<String>> {
    if snapshot_value(&state)?.busy() {
        return Err("Aguarde o término da gravação para mudar a pasta.".into());
    }
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Pasta de gravações do Tinrec")
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|e| e.to_string())?;
    let path = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    // Recheck after the native dialog: recording could have started meanwhile.
    let session = lock(&state.session)?;
    if session.snapshot.busy() {
        return Err("A gravação começou; a pasta foi mantida.".into());
    }
    let contents =
        serde_json::to_vec(&serde_json::json!({"folder":path})).map_err(|e| e.to_string())?;
    let temp = state.config.with_extension("tmp");
    fs::write(&temp, contents).map_err(|e| format!("Não foi possível salvar a pasta: {e}"))?;
    fs::rename(&temp, &state.config)
        .map_err(|e| format!("Não foi possível salvar a pasta: {e}"))?;
    *lock(&state.folder)? = path.clone();
    Ok(Some(path.to_string_lossy().into()))
}

#[tauri::command]
async fn start_recording(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    mut request: StartRequest,
) -> Reply<Snapshot> {
    if !["screen", "window", "area"].contains(&request.capture_mode.as_str()) {
        return Err("Modo de gravação indisponível. Escolha Tela, Janela ou Área.".into());
    }
    if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        request.display = "auto".into();
    }
    if ![720, 1080].contains(&request.resolution)
        || request.microphone_volume > 100
        || request.system_volume > 100
    {
        return Err("Resolução ou volume inválido.".into());
    }
    if (request.capture_mode == "camera" || request.webcam_enabled)
        && request.camera.trim().is_empty()
    {
        return Err("Escolha uma câmera para gravar.".into());
    }
    if request.capture_mode == "window"
        && request.window.is_empty()
        && !(cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some())
        && !synthetic_enabled()
    {
        return Err("Escolha uma janela para gravar.".into());
    }
    let title = request.title.trim();
    if title.is_empty()
        || title.chars().count() > 90
        || title.contains(['{', '}'])
        || safe_title(title) != title
    {
        return Err("Informe um nome de vídeo válido em Configurações antes de gravar.".into());
    }
    let mut session = lock(&state.session)?;
    if app
        .state::<screenshots::Prints>()
        .busy
        .load(Ordering::Acquire)
    {
        return Err("Finalize o print aberto antes de gravar.".into());
    }

    if state.exiting.load(Ordering::Acquire) {
        return Err("O Tinrec está sendo encerrado.".into());
    }
    if session.snapshot.busy() {
        return Err("Já existe uma gravação em andamento.".into());
    }
    if !request.keep_local && !state.cloud.view()?.connected {
        return Err("Conecte sua conta Google ou ative a cópia local em Configurações.".into());
    }
    let folder = if request.keep_local {
        lock(&state.folder)?.join("Videos")
    } else {
        state
            .config
            .parent()
            .ok_or("Pasta de dados indisponível")?
            .join("pending-recordings")
    };
    fs::create_dir_all(&folder)
        .map_err(|e| format!("Não foi possível criar a pasta de gravações: {e}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let file_name = format!(
        "{} - {}-{}.mp4",
        safe_title(&request.title),
        now_ms(),
        &id[..8]
    );
    let cloud_title = safe_title(&request.title);
    let cloud_resolution = request.resolution;
    let webcam = if request.capture_mode == "camera" {
        None
    } else if !request.camera.is_empty() {
        Some(request.camera.clone())
    } else {
        tinrec_capture::camera::devices(&tool_path(&app, "ffmpeg"))
            .ok()
            .and_then(|cameras| cameras.first().map(|camera| camera.id.clone()))
    };
    let webcam_available = webcam.is_some();
    let webcam_position = *lock(&state.webcam_position)?;
    let options = RecordOptions {
        output: folder.join(&file_name),
        fast_finalize: true,
        duration: 7200.0,
        system_audio: request.system_audio,
        microphone: request.microphone,
        microphone_device: request.microphone_device.filter(|s| !s.is_empty()),
        display: match request.capture_mode.as_str() {
            "window" => {
                if request.window.is_empty() {
                    "auto".into()
                } else {
                    request.window
                }
            }
            "camera" => request.camera,
            _ => request.display.clone(),
        },
        capture_mode: if portal_handles_area(
            cfg!(target_os = "linux"),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
            &request.capture_mode,
        ) {
            "screen".into()
        } else {
            request.capture_mode.clone()
        },
        crop: None,
        webcam,
        webcam_disabled: !request.webcam_enabled,
        webcam_visible_in_capture: cfg!(target_os = "linux") && request.capture_mode != "window",
        webcam_x: webcam_position.0,
        webcam_y: webcam_position.1,
        backend: "auto".into(),
        resolution: request.resolution,
        system_volume: request.system_volume,
        microphone_volume: request.microphone_volume,
        ffmpeg: tool_path(&app, "ffmpeg"),
        ffprobe: tool_path(&app, "ffprobe"),
        pause_after: None,
        pause_for: 2.0,
    };
    recording::validate_options(&options).map_err(|e| e.to_string())?;
    let queued = state.cloud.enqueue(
        id.clone(),
        options.output.clone(),
        cloud_title,
        cloud_resolution,
        0.0,
        true,
    );
    if !request.keep_local {
        queued?;
        state.cloud.remove_local_after_upload(&id)?;
    }
    let (sender, receiver) = mpsc::channel();
    session.id = id.clone();
    session.capture_mode = request.capture_mode.clone();
    session.controls = Some(sender);
    session.exit_requested = false;
    session.discard_requested = false;
    session.preview = None;
    session.webcam_preview = None;
    session.snapshot = Snapshot {
        revision: session.snapshot.revision + 1,
        phase: "starting".into(),
        session_id: Some(id.clone()),
        file_name: Some(file_name),
        webcam_available,
        webcam_enabled: webcam_available && request.webcam_enabled,
        microphone_enabled: options.microphone,
        system_enabled: options.system_audio,
        ..Default::default()
    };
    let initial = session.snapshot.clone();
    *lock(&state.webcam_region)? = None;
    drop(session);
    position_webcam_window(&app);
    emit(&app, &initial);
    let state = state.inner().clone();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }
    tauri::async_runtime::spawn_blocking(move || {
        let tools = check_tools(&app);
        let outcome = if !tools.ready {
            Err(tools.message.unwrap_or_default())
        } else {
            // A deterministic test source is available only in debug builds, through
            // the host environment; it is never a frontend recording option.
            let synthetic = synthetic_enabled();
            let selection_callback = |frame: recording::PreviewFrame| {
                let mut jpeg = Vec::new();
                if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
                    .encode(
                        &frame.rgb,
                        frame.width,
                        frame.height,
                        image::ExtendedColorType::Rgb8,
                    )
                    .is_err()
                {
                    return;
                }
                let preview = Preview {
                    session_id: id.clone(),
                    sequence: 1,
                    image: format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(jpeg)
                    ),
                    width: frame.width,
                    height: frame.height,
                    source_width: frame.source_width,
                    source_height: frame.source_height,
                };
                if let Ok(mut session) = state.session.lock()
                    && session.id == id
                    && session.snapshot.phase == "starting"
                    && !session.discard_requested
                {
                    session.preview = Some(preview.clone());
                    drop(session);
                    let overlay = (|| -> Reply<()> {
                        let window = app
                            .get_webview_window("area")
                            .ok_or("Seletor indisponível")?;
                        let monitors = window.available_monitors().map_err(|e| e.to_string())?;
                        let primary = window.primary_monitor().map_err(|e| e.to_string())?;
                        let source_name = pinray::enumerate_sources().ok().and_then(|sources| {
                            sources.into_iter().find_map(|source| match source {
                                pinray::CaptureSource::Display(display)
                                    if display.id.0 == options.display =>
                                {
                                    Some(display.name)
                                }
                                _ => None,
                            })
                        });
                        let named_monitor = monitors
                            .iter()
                            .find(|monitor| {
                                source_name.as_deref().is_some_and(|name| {
                                    monitor.name().is_some_and(|value| value == name)
                                })
                            })
                            .cloned();
                        let monitor = named_monitor
                            .or_else(|| (monitors.len() == 1).then(|| monitors[0].clone()))
                            .or_else(|| primary.filter(|monitor| source_matches_monitor(monitor.size().width, monitor.size().height, monitor.scale_factor(), preview.source_width, preview.source_height)))
                            .or_else(|| monitors.into_iter().find(|monitor| source_matches_monitor(monitor.size().width, monitor.size().height, monitor.scale_factor(), preview.source_width, preview.source_height)))
                            .ok_or("Não foi possível localizar o monitor da captura para selecionar a área.")?;
                        window
                            .set_position(*monitor.position())
                            .map_err(|e| e.to_string())?;
                        window
                            .set_size(*monitor.size())
                            .map_err(|e| e.to_string())?;
                        window.show().map_err(|e| e.to_string())?;
                        window.set_focus().map_err(|e| e.to_string())?;
                        app.emit_to("area", "area-selection", preview)
                            .map_err(|e| e.to_string())?;
                        Ok(())
                    })();
                    if let Err(error) = overlay
                        && let Ok(mut session) = state.session.lock()
                    {
                        eprintln!("Não foi possível abrir a seleção de área: {error}");
                        session.snapshot.error =
                            Some(format!("Não foi possível abrir a seleção de área: {error}"));
                        session.discard_requested = true;
                        if let Some(sender) = &session.controls {
                            let _ = sender.send(Control::Stop);
                        }
                    }
                }
            };
            let webcam_callback = |frame: recording::PreviewFrame| {
                let mut jpeg = Vec::new();
                if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 75)
                    .encode(
                        &frame.rgb,
                        frame.width,
                        frame.height,
                        image::ExtendedColorType::Rgb8,
                    )
                    .is_err()
                {
                    return;
                }
                let preview = Preview {
                    session_id: id.clone(),
                    sequence: now_ms(),
                    image: format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(jpeg)
                    ),
                    width: frame.width,
                    height: frame.height,
                    source_width: frame.source_width,
                    source_height: frame.source_height,
                };
                if let Ok(mut session) = state.session.lock()
                    && session.id == id
                    && session.snapshot.webcam_enabled
                {
                    session.webcam_preview = Some(preview.clone());
                    drop(session);
                    let _ = app.emit_to("webcam", "webcam-preview", preview);
                }
            };
            recording::run_observed_with_webcam(
                &options,
                synthetic,
                receiver,
                |progress| {
                    if let Ok(mut session) = state.session.lock()
                        && session.id == id
                    {
                        if !session.discard_requested
                            && (session.snapshot.phase != "stopping"
                                || progress.state == "finalizing")
                        {
                            session.snapshot.phase = progress.state.into();
                        }
                        session.snapshot.elapsed_seconds = progress.active_seconds;
                        session.snapshot.video_frames = progress.video_frames;
                        session.snapshot.audio_peak = progress.audio_peak;
                        session.snapshot.microphone_muted = progress.microphone_muted;
                        session.snapshot.system_muted = progress.system_muted;
                        session.snapshot.revision += 1;
                        let snapshot = session.snapshot.clone();
                        drop(session);
                        emit(&app, &snapshot);
                    }
                },
                if options.capture_mode == "area" {
                    Some(&selection_callback)
                } else {
                    None
                },
                Some(&webcam_callback),
            )
            .map_err(|e| format!("{e:#}"))
        };
        // The session mutex resolves Stop/Discard against finalization before the job can become uploadable.
        if let Ok(mut session) = state.session.lock()
            && session.id == id
        {
            if session.discard_requested {
                let cloud_result = state.cloud.discard(&id);
                let local_result = discard_files(&options.output);
                let selection_error = session.snapshot.error.take();
                session.snapshot.phase = if selection_error.is_some() {
                    "failed"
                } else {
                    "discarded"
                }
                .into();
                session.snapshot.error = local_result
                    .err()
                    .or_else(|| cloud_result.err())
                    .or(selection_error);
                session.snapshot.file_name = None;
                session.preview = None;
            } else {
                let (seconds, success) = match &outcome {
                    Ok(report) => (report.active_seconds, report.status == "completed"),
                    Err(_) => (0.0, false),
                };
                let _ = state.cloud.finish(&id, seconds, success);
                match outcome {
                    Ok(report) => {
                        session.snapshot.elapsed_seconds = report.active_seconds;
                        session.snapshot.phase =
                            if success { "completed" } else { "failed" }.into();
                        session.snapshot.error = report.error;
                    }
                    Err(error) => {
                        session.snapshot.phase = "failed".into();
                        session.snapshot.error = Some(error);
                    }
                }
            }
            session.controls = None;
            session.snapshot.revision += 1;
            let snapshot = session.snapshot.clone();
            let close = session.exit_requested;
            drop(session);
            if snapshot.phase != "completed" && !close {
                let _ = show_recorder(app.clone());
            }
            emit(&app, &snapshot);
            if close {
                app.exit(0);
            } else if snapshot.phase == "completed" {
                open_completed_video(app.clone(), state.clone(), id.clone());
            }
        }
    });
    Ok(initial)
}

// A page is normally reserved while recording. If that request is still pending,
// wait off the capture thread and open this session's page exactly once.
fn open_completed_video(app: tauri::AppHandle, state: Shared, id: String) {
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            if state.exiting.load(Ordering::Acquire) {
                return;
            }
            let Ok(view) = state.cloud.view() else { return };
            if !view.connected {
                return;
            }
            let Some(job) = view.jobs.iter().find(|job| job.id == id) else {
                return;
            };
            if matches!(job.status.as_str(), "failed" | "discarded") {
                return;
            }
            if let Some(url) = &job.url {
                if app.opener().open_url(url, None::<&str>).is_err()
                    && let Ok(mut session) = state.session.lock()
                    && session.id == id
                    && session.snapshot.phase == "completed"
                {
                    session.snapshot.error = Some("Vídeo salvo, mas não foi possível abrir o navegador. Abra a biblioteca no site para assistir.".into());
                    session.snapshot.revision += 1;
                    let snapshot = session.snapshot.clone();
                    drop(session);
                    emit(&app, &snapshot);
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

#[tauri::command]
fn control_recording(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    action: String,
) -> Reply<Snapshot> {
    let mut session = lock(&state.session)?;
    let control = match (action.as_str(), session.snapshot.phase.as_str()) {
        ("pause", "recording") => Control::Pause,
        ("resume", "paused") => Control::Resume,
        ("stop", "starting" | "recording" | "paused") => Control::Stop,
        ("discard", "starting" | "recording" | "paused" | "stopping" | "finalizing") => {
            Control::Stop
        }
        ("toggle-webcam", "recording" | "paused") if session.snapshot.webcam_available => {
            Control::SetWebcamEnabled(!session.snapshot.webcam_enabled)
        }
        ("mute-microphone", "recording" | "paused") if session.snapshot.microphone_enabled => {
            Control::SetMicrophoneMuted(!session.snapshot.microphone_muted)
        }
        ("mute-system", "recording" | "paused") if session.snapshot.system_enabled => {
            Control::SetSystemMuted(!session.snapshot.system_muted)
        }
        _ => return Err("Esse controle não está disponível no estado atual.".into()),
    };
    session
        .controls
        .as_ref()
        .ok_or("Não há gravação em andamento")?
        .send(control)
        .map_err(|_| "O motor de gravação encerrou".to_string())?;
    if action == "discard" {
        session.discard_requested = true;
        session.snapshot.phase = "discarding".into();
    } else if matches!(control, Control::Stop) {
        session.snapshot.phase = "stopping".into();
    }
    match control {
        Control::SetMicrophoneMuted(muted) => session.snapshot.microphone_muted = muted,
        Control::SetSystemMuted(muted) => session.snapshot.system_muted = muted,
        Control::SetWebcamEnabled(enabled) => {
            session.snapshot.webcam_enabled = enabled;
            session.webcam_preview = None;
        }
        _ => {}
    }
    session.snapshot.revision += 1;
    let snapshot = session.snapshot.clone();
    drop(session);
    emit(&app, &snapshot);
    Ok(snapshot)
}

fn discard_files(output: &Path) -> Reply<()> {
    let mut paths = vec![output.to_owned()];
    for suffix in [
        ".partial.mp4",
        ".capture.json",
        ".ffprobe.json",
        ".ffmpeg.log",
        ".camera.log",
    ] {
        paths.push(tinrec_capture::encoder::sidecar(output, suffix));
    }
    let mut failed = false;
    for path in paths {
        if let Err(e) = fs::remove_file(path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            failed = true;
        }
    }
    if failed {
        Err("O envio foi cancelado, mas alguns arquivos locais não puderam ser excluídos. Abra a pasta para removê-los.".into())
    } else {
        Ok(())
    }
}
fn load_webcam_position(config: &Path) -> (f64, f64) {
    fs::read(config.with_file_name("webcam-position.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<(f64, f64)>(&bytes).ok())
        .filter(|(x, y)| {
            x.is_finite() && y.is_finite() && (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y)
        })
        .unwrap_or((0.0, 1.0))
}
fn webcam_bounds(state: &AppState, monitor: &tauri::Monitor) -> (i32, i32, u32, u32) {
    let region = state
        .webcam_region
        .lock()
        .ok()
        .and_then(|value| *value)
        .unwrap_or((0., 0., 1., 1.));
    (
        monitor.position().x + (f64::from(monitor.size().width) * region.0).round() as i32,
        monitor.position().y + (f64::from(monitor.size().height) * region.1).round() as i32,
        (f64::from(monitor.size().width) * region.2).round().max(2.) as u32,
        (f64::from(monitor.size().height) * region.3)
            .round()
            .max(2.) as u32,
    )
}
fn position_webcam_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("webcam") else {
        return;
    };
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let Ok(Some(monitor)) = main.current_monitor() else {
        return;
    };
    let state = app.state::<Shared>();
    let Ok(position) = state.webcam_position.lock() else {
        return;
    };
    let (x, y) = *position;
    drop(position);
    let (origin_x, origin_y, width, height) = webcam_bounds(&state, &monitor);
    let diameter = (height / 5).min(width / 2).max(1);
    let margin = (height / 40).min(width.saturating_sub(diameter));
    let _ = window.set_size(tauri::PhysicalSize::new(diameter, diameter));
    let _ = window.set_position(tauri::PhysicalPosition::new(
        origin_x
            + margin as i32
            + (f64::from(width.saturating_sub(diameter + 2 * margin)) * x).round() as i32,
        origin_y
            + margin as i32
            + (f64::from(height.saturating_sub(diameter + 2 * margin)) * y).round() as i32,
    ));
}
fn save_webcam_position(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("webcam") else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let Ok(origin) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let state = app.state::<Shared>();
    if !snapshot_value(&state).is_ok_and(|snapshot| snapshot.busy()) {
        return;
    }
    let (origin_x, origin_y, width, height) = webcam_bounds(&state, &monitor);
    let margin = height / 40;
    let x = (f64::from(origin.x - origin_x - margin as i32)
        / f64::from(width.saturating_sub(size.width + 2 * margin).max(1)))
    .clamp(0., 1.);
    let y = (f64::from(origin.y - origin_y - margin as i32)
        / f64::from(height.saturating_sub(size.height + 2 * margin).max(1)))
    .clamp(0., 1.);
    if let Ok(mut position) = state.webcam_position.lock() {
        *position = (x, y);
    }
    if let Ok(bytes) = serde_json::to_vec(&(x, y)) {
        let _ = fs::write(state.config.with_file_name("webcam-position.json"), bytes);
    }
    if let Ok(session) = state.session.lock()
        && let Some(sender) = &session.controls
    {
        let _ = sender.send(Control::SetWebcamPosition(x, y));
    }
}
#[tauri::command]
fn get_webcam_preview(state: State<'_, Shared>) -> Reply<Option<Preview>> {
    let session = lock(&state.session)?;
    Ok(
        if session.snapshot.webcam_enabled && session.snapshot.busy() {
            session.webcam_preview.clone()
        } else {
            None
        },
    )
}
#[tauri::command]
fn get_preview(state: State<'_, Shared>) -> Reply<Option<Preview>> {
    Ok(lock(&state.session)?.preview.clone())
}
#[tauri::command]
fn show_recorder(app: tauri::AppHandle) -> Reply<()> {
    let window = app
        .get_webview_window("main")
        .ok_or("Janela principal indisponível")?;
    let _ = window.unminimize();
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}
#[tauri::command]
fn show_controls(app: tauri::AppHandle, state: State<'_, Shared>) -> Reply<()> {
    if !snapshot_value(&state)?.busy() {
        return Err("Inicie uma gravação para abrir os controles.".into());
    }
    app.get_webview_window("controls")
        .ok_or("Controles indisponíveis")?
        .show()
        .map_err(|e| e.to_string())
}

fn recording_path(state: &AppState, file_name: &str) -> Reply<PathBuf> {
    if !file_name.ends_with(".mp4")
        || file_name.ends_with(".partial.mp4")
        || Path::new(file_name).file_name().and_then(|s| s.to_str()) != Some(file_name)
    {
        return Err("Arquivo de gravação inválido.".into());
    }
    let folder = fs::canonicalize(&*lock(&state.folder)?).map_err(|e| e.to_string())?;
    let videos = folder.join("Videos");
    let path = fs::canonicalize(videos.join(file_name))
        .or_else(|_| fs::canonicalize(folder.join(file_name)))
        .map_err(|_| "A gravação não está mais nessa pasta.".to_string())?;
    if (path.parent() != Some(folder.as_path()) && path.parent() != Some(videos.as_path()))
        || !path.is_file()
    {
        return Err("O arquivo não pertence à pasta de gravações.".into());
    }
    Ok(path)
}
#[tauri::command]
fn open_recording(app: tauri::AppHandle, state: State<'_, Shared>, file_name: String) -> Reply<()> {
    let path = recording_path(&state, &file_name)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn recording_thumbnail(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    file_name: String,
) -> Reply<String> {
    let path = recording_path(&state, &file_name)?;
    let ffmpeg = tool_path(&app, "ffmpeg");
    tauri::async_runtime::spawn_blocking(move || extract_thumbnail(&ffmpeg, &path))
        .await
        .map_err(|e| e.to_string())?
}
fn extract_thumbnail(ffmpeg: &Path, path: &Path) -> Reply<String> {
    let output = native_command(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=480:270:force_original_aspect_ratio=decrease",
            "-threads",
            "1",
            "-f",
            "image2pipe",
            "-c:v",
            "mjpeg",
            "-q:v",
            "5",
            "pipe:1",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err("Miniatura indisponível para este vídeo.".into());
    }
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(output.stdout)
    ))
}
#[tauri::command]
fn open_folder(app: tauri::AppHandle, state: State<'_, Shared>) -> Reply<()> {
    let path = lock(&state.folder)?.clone();
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn list_recordings(state: State<'_, Shared>) -> Reply<Vec<RecordingFile>> {
    let folder = lock(&state.folder)?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if !folder.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        let entries = fs::read_dir(&folder)
            .map_err(|e| e.to_string())?
            .chain(fs::read_dir(folder.join("Videos")).into_iter().flatten());
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.ends_with(".mp4")
                || file_name.ends_with(".partial.mp4")
                || !entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                continue;
            }
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            let report = fs::read(tinrec_capture::encoder::sidecar(
                &entry.path(),
                ".capture.json",
            ))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            let status = report
                .as_ref()
                .and_then(|r| r["status"].as_str())
                .unwrap_or("unverified")
                .to_owned();
            let title = file_name.rsplit_once(" - ").map_or_else(
                || file_name.trim_end_matches(".mp4").to_owned(),
                |(title, _)| title.to_owned(),
            );
            files.push(RecordingFile {
                file_name,
                title,
                bytes: metadata.len(),
                modified_ms: metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as u64),
                duration: report.as_ref().and_then(|r| r["active_seconds"].as_f64()),
                status,
            });
        }
        files.sort_by_key(|f| std::cmp::Reverse(f.modified_ms));
        files.truncate(100);
        Ok(files)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cloud_state(state: State<'_, Shared>) -> Reply<cloud::View> {
    state.cloud.view()
}
#[tauri::command]
async fn cloud_connect(app: tauri::AppHandle, state: State<'_, Shared>) -> Reply<()> {
    let cloud = state.cloud.clone();
    let url = tauri::async_runtime::spawn_blocking(move || cloud.connect())
        .await
        .map_err(|e| e.to_string())??;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn cloud_disconnect(state: State<'_, Shared>) -> Reply<()> {
    let cloud = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || cloud.disconnect())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn cloud_retry(state: State<'_, Shared>, id: String) -> Reply<()> {
    state.cloud.retry(&id)
}
#[tauri::command]
fn cloud_enqueue(state: State<'_, Shared>, file_name: String) -> Reply<()> {
    let path = recording_path(&state, &file_name)?;
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(tinrec_capture::encoder::sidecar(&path, ".capture.json")).map_err(|_| {
            "Esta gravação não tem relatório de finalização. Confira o arquivo antes de enviar."
                .to_string()
        })?,
    )
    .map_err(|e| e.to_string())?;
    if report["status"] != "completed" {
        return Err("Somente gravações concluídas podem ser enviadas.".into());
    }
    let duration = report["active_seconds"]
        .as_f64()
        .ok_or("Duração não disponível")?;
    let title = file_name
        .rsplit_once(" - ")
        .map_or(file_name.trim_end_matches(".mp4"), |(title, _)| title);
    state.cloud.enqueue(
        uuid::Uuid::new_v4().to_string(),
        path,
        title.to_owned(),
        1080,
        duration,
        false,
    )
}
#[tauri::command]
fn cloud_open_recording(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    file_name: String,
) -> Reply<()> {
    let path = recording_path(&state, &file_name)?;
    let url = state.cloud.recording_url(&path)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn cloud_open(app: tauri::AppHandle, state: State<'_, Shared>, id: Option<String>) -> Reply<()> {
    let url = match id {
        None => "https://tinrec.vercel.app".to_owned(),
        Some(id) => state
            .cloud
            .view()?
            .jobs
            .into_iter()
            .find(|j| j.id == id)
            .and_then(|j| j.url)
            .ok_or("O link ainda não está disponível")?,
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cloud_copy(app: tauri::AppHandle, state: State<'_, Shared>, id: String) -> Reply<()> {
    let url = state
        .cloud
        .view()?
        .jobs
        .into_iter()
        .find(|j| j.id == id)
        .and_then(|j| j.url)
        .ok_or("O link ainda não está disponível")?;
    app.clipboard().write_text(url).map_err(|_| {
        "Não foi possível copiar o link. Copie o endereço exibido no aplicativo.".into()
    })
}

#[tauri::command]
fn hide_to_tray(app: tauri::AppHandle) -> Reply<()> {
    app.get_webview_window("main")
        .ok_or("Janela principal indisponível")?
        .close()
        .map_err(|e| e.to_string())
}

fn request_exit(app: &tauri::AppHandle) -> Reply<()> {
    if app
        .state::<screenshots::Prints>()
        .busy
        .load(Ordering::Acquire)
    {
        let _ = show_recorder(app.clone());
        return Err("Feche o editor ou cancele a captura do print antes de sair.".into());
    }
    let state = app.state::<Shared>();
    let mut session = lock(&state.session)?;
    if session.snapshot.busy() {
        if session.exit_requested {
            return Ok(());
        }
        // Explicit quit finishes the active file; closing a window never sends Stop.
        session
            .controls
            .as_ref()
            .ok_or("Motor de gravação indisponível")?
            .send(Control::Stop)
            .map_err(|_| {
                "O motor está encerrando; aguarde um instante e tente sair novamente.".to_string()
            })?;
        session.exit_requested = true;
        state.exiting.store(true, Ordering::Release);
        if !session.discard_requested && session.snapshot.phase != "finalizing" {
            session.snapshot.phase = "stopping".into();
        }
        session.snapshot.revision += 1;
        let snapshot = session.snapshot.clone();
        drop(session);
        emit(app, &snapshot);
        let _ = show_recorder(app.clone());
    } else {
        state.exiting.store(true, Ordering::Release);
        drop(session);
        app.exit(0);
    }
    Ok(())
}
#[tauri::command]
fn quit_app(app: tauri::AppHandle) -> Reply<()> {
    request_exit(&app)
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        let use_xwayland = std::env::var_os("GDK_BACKEND").is_none()
            && std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var_os("DISPLAY").is_some();
        let compatible_renderer = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none();
        if (use_xwayland || compatible_renderer)
            && let Ok(executable) = std::env::current_exe()
        {
            use std::os::unix::process::CommandExt;
            // Configure WebKit before GTK or worker threads start. DMA-BUF can
            // fail to allocate GBM buffers on otherwise working Linux desktops.
            // This changes only the webview renderer, not PipeWire/FFmpeg.
            let mut command = Command::new(executable);
            command.args(std::env::args_os().skip(1));
            if use_xwayland {
                command.env("GDK_BACKEND", "x11");
            }
            if compatible_renderer {
                command.env("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
            let error = command.exec();
            eprintln!("Não foi possível configurar a renderização do Tinrec: {error}");
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = show_recorder(app.clone());
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data = match std::env::var_os("TINREC_DATA_DIR") {
                Some(path) => PathBuf::from(path),
                None => app.path().app_config_dir()?,
            };
            fs::create_dir_all(&data)?;
            let config = data.join("desktop.json");
            let saved = fs::read(&config)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|v| v["folder"].as_str().map(PathBuf::from))
                .filter(|path| path.is_absolute());
            let default = if std::env::var_os("TINREC_DATA_DIR").is_some() {
                data.join("Recordings")
            } else {
                app.path()
                    .video_dir()
                    .or_else(|_| app.path().document_dir())
                    .or_else(|_| app.path().home_dir())?
                    .join("Tinrec")
            };
            let cloud = cloud::Cloud::new(&data).map_err(std::io::Error::other)?;
            cloud.start();
            app.manage(Arc::new(AppState {
                cloud,
                exiting: AtomicBool::new(false),
                folder: Mutex::new(saved.unwrap_or(default)),
                webcam_position: Mutex::new(load_webcam_position(&config)),
                webcam_region: Mutex::new(None),
                config,
                session: Mutex::new(Session {
                    snapshot: Snapshot {
                        phase: "idle".into(),
                        ..Default::default()
                    },
                    controls: None,
                    id: String::new(),
                    exit_requested: false,
                    discard_requested: false,
                    preview: None,
                    capture_mode: "screen".into(),
                    webcam_preview: None,
                }),
            }));
            app.manage(screenshots::Prints::default());
            app.manage(shortcuts::Shortcuts::new(data.join("shortcuts.json")));
            if std::env::var_os("WAYLAND_DISPLAY").is_none() || !cfg!(target_os = "linux") {
                app.handle()
                    .plugin(tauri_plugin_global_shortcut::Builder::new().build())?;
            }
            let shortcut_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let manager = shortcut_app.state::<shortcuts::Shortcuts>();
                if let Ok(view) = manager.view()
                    && !view.bindings.is_empty()
                {
                    let _ = manager.apply(&shortcut_app, view.bindings).await;
                }
            });
            // Webviews can invoke commands while setup is still running on Windows.
            // Register all managed state and plugins before loading any frontend.
            for window in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app, window)?.build()?;
            }
            #[cfg(target_os = "linux")]
            if let Some(area) = app.get_webview_window("print-area") {
                area.with_webview(|webview| {
                    use webkit2gtk::{SettingsExt, WebViewExt};
                    if let Some(settings) = webview.inner().settings() {
                        settings.set_hardware_acceleration_policy(
                            webkit2gtk::HardwareAccelerationPolicy::Never,
                        );
                    }
                })?;
            }
            if let Some(controls) = app.get_webview_window("controls")
                && let Some(main) = app.get_webview_window("main")
                && let Ok(Some(monitor)) = main.current_monitor()
            {
                let size = monitor.size();
                let origin = monitor.position();
                let scale = monitor.scale_factor();
                let width = (536.0 * scale) as i32;
                let height = (56.0 * scale) as i32;
                let _ = controls.set_position(tauri::PhysicalPosition::new(
                    origin.x + (size.width as i32 - width).max(0) / 2,
                    origin.y + (size.height as i32 - height - (40.0 * scale) as i32).max(0),
                ));
            }
            position_webcam_window(app.handle());
            tray::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            screenshots::begin_print_edit,
            screenshots::get_print_area,
            screenshots::select_print_area,
            screenshots::save_print_project,
            screenshots::open_print_project,
            screenshots::capture_print,
            screenshots::close_print,
            screenshots::export_print,
            screenshots::get_print_keep_local,
            screenshots::set_print_keep_local,
            screenshots::list_prints,
            screenshots::open_print,
            screenshots::print_site,
            screenshots::upload_print,
            shortcuts::local_shortcut,
            shortcuts::get_shortcuts,
            shortcuts::save_shortcuts,
            shortcuts::shortcut_capture,
            quit_app,
            hide_to_tray,
            get_snapshot,
            get_preview,
            show_recorder,
            show_controls,
            get_tools,
            get_sources,
            get_cameras,
            select_area,
            get_webcam_preview,
            get_folder,
            choose_folder,
            start_recording,
            control_recording,
            list_recordings,
            open_recording,
            recording_thumbnail,
            open_folder,
            cloud_state,
            cloud_connect,
            cloud_disconnect,
            cloud_enqueue,
            cloud_retry,
            cloud_open,
            cloud_open_recording,
            cloud_copy
        ])
        .on_window_event(|window, event| {
            if window.label() == "webcam" && matches!(event, tauri::WindowEvent::Moved(_)) {
                save_webcam_position(window.app_handle());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Não foi possível iniciar o Tinrec")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                let state = app.state::<Shared>();
                if snapshot_value(&state).is_ok_and(|snapshot| snapshot.busy()) {
                    api.prevent_exit();
                    let _ = request_exit(app);
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                let _ = show_recorder(app.clone());
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_monitor_matching_accepts_physical_and_scaled_dimensions() {
        assert!(source_matches_monitor(2560, 1440, 1.25, 2560, 1440));
        assert!(source_matches_monitor(2560, 1440, 1.25, 2048, 1152));
        assert!(source_matches_monitor(2560, 1440, 1.5, 1707, 960));
        assert!(!source_matches_monitor(2560, 1440, 1.25, 1920, 1080));
    }
    #[test]
    fn webcam_position_survives_restart_and_rejects_invalid_values() {
        let dir = std::env::temp_dir().join(format!("tinrec-position-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("desktop.json");
        assert_eq!(load_webcam_position(&config), (0., 1.));
        fs::write(dir.join("webcam-position.json"), "[0.75,0.2]").unwrap();
        assert_eq!(load_webcam_position(&config), (0.75, 0.2));
        fs::write(dir.join("webcam-position.json"), "[2,-1]").unwrap();
        assert_eq!(load_webcam_position(&config), (0., 1.));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recording_names_cannot_contain_paths_or_be_empty() {
        assert_eq!(safe_title("../../minha: gravação?"), "minha gravação");
        assert_eq!(safe_title(".. / "), "Gravação");
        assert_eq!(safe_title("Uma conversa"), "Uma conversa");
    }
    #[test]
    fn extracts_a_real_frame_and_rejects_invalid_media() {
        let file =
            std::env::temp_dir().join(format!("tinrec-thumbnail-{}.mp4", uuid::Uuid::new_v4()));
        let status = native_command(Path::new("ffmpeg"))
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=640x360:d=0.1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&file)
            .status()
            .expect("FFmpeg is required for the thumbnail test");
        assert!(status.success());
        let result = extract_thumbnail(Path::new("ffmpeg"), &file);
        fs::remove_file(&file).unwrap();
        let value = result.unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value.strip_prefix("data:image/jpeg;base64,").unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (480, 270));
        assert!(extract_thumbnail(Path::new("ffmpeg"), &file).is_err());
    }
}
