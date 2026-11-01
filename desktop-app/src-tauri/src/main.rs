#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cloud;
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
}
struct AppState {
    session: Mutex<Session>,
    folder: Mutex<PathBuf>,
    config: PathBuf,
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
#[serde(rename_all = "camelCase")]
struct Sources {
    synthetic: bool,
    platform: String,
    wayland: bool,
    displays: Vec<Display>,
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartRequest {
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
    if let Some(window) = app.get_webview_window("controls") {
        if snapshot.busy() && snapshot.phase != "starting" {
            if !window.is_visible().unwrap_or(false) {
                let _ = window.show();
            }
        } else {
            let _ = window.hide();
        }
    }
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
                microphones: Vec::new(),
                warnings: Vec::new(),
            };
        }
        let mut warnings = Vec::new();
        let displays = match pinray::enumerate_sources() {
            Ok(sources) => sources
                .into_iter()
                .filter_map(|s| match s {
                    pinray::CaptureSource::Display(d) => Some(Display {
                        id: d.id.0,
                        name: d.name,
                        width: d.width,
                        height: d.height,
                    }),
                    _ => None,
                })
                .collect(),
            Err(e) => {
                warnings.push(format!("Não foi possível listar telas: {e}"));
                Vec::new()
            }
        };
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
            microphones,
            warnings,
        }
    })
    .await
    .map_err(|e| e.to_string())
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
    request: StartRequest,
) -> Reply<Snapshot> {
    if ![720, 1080].contains(&request.resolution)
        || request.microphone_volume > 100
        || request.system_volume > 100
    {
        return Err("Resolução ou volume inválido.".into());
    }
    let mut session = lock(&state.session)?;
    if state.exiting.load(Ordering::Acquire) {
        return Err("O Tinrec está sendo encerrado.".into());
    }
    if session.snapshot.busy() {
        return Err("Já existe uma gravação em andamento.".into());
    }
    let folder = lock(&state.folder)?.clone();
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
    let options = RecordOptions {
        output: folder.join(&file_name),
        fast_finalize: true,
        duration: 7200.0,
        system_audio: request.system_audio,
        microphone: request.microphone,
        microphone_device: request.microphone_device.filter(|s| !s.is_empty()),
        display: request.display,
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
    let (sender, receiver) = mpsc::channel();
    session.id = id.clone();
    session.controls = Some(sender);
    session.exit_requested = false;
    session.discard_requested = false;
    session.preview = None;
    session.snapshot = Snapshot {
        revision: session.snapshot.revision + 1,
        phase: "starting".into(),
        session_id: Some(id.clone()),
        file_name: Some(file_name),
        microphone_enabled: options.microphone,
        system_enabled: options.system_audio,
        ..Default::default()
    };
    let initial = session.snapshot.clone();
    drop(session);
    emit(&app, &initial);
    let state = state.inner().clone();
    let _ = state.cloud.enqueue(
        id.clone(),
        options.output.clone(),
        cloud_title,
        cloud_resolution,
        0.0,
        true,
    );
    let (preview_sender, preview_receiver) = mpsc::sync_channel::<recording::PreviewFrame>(1);
    let preview_app = app.clone();
    let preview_state = state.clone();
    let preview_id = id.clone();
    std::thread::spawn(move || {
        let mut sequence = 0;
        while let Ok(frame) = preview_receiver.recv() {
            let mut jpeg = Vec::new();
            if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 65)
                .encode(
                    &frame.rgb,
                    frame.width,
                    frame.height,
                    image::ExtendedColorType::Rgb8,
                )
                .is_err()
            {
                continue;
            }
            sequence += 1;
            let preview = Preview {
                session_id: preview_id.clone(),
                sequence,
                image: format!(
                    "data:image/jpeg;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(jpeg)
                ),
                width: frame.width,
                height: frame.height,
                source_width: frame.source_width,
                source_height: frame.source_height,
            };
            if let Ok(mut session) = preview_state.session.lock()
                && session.id == preview_id
                && matches!(session.snapshot.phase.as_str(), "recording" | "paused")
            {
                session.preview = Some(preview.clone());
                drop(session);
                let _ = preview_app.emit_to("main", "recorder-preview", preview);
            }
        }
    });
    tauri::async_runtime::spawn_blocking(move || {
        let tools = check_tools(&app);
        let outcome = if !tools.ready {
            Err(tools.message.unwrap_or_default())
        } else {
            // A deterministic test source is available only in debug builds, through
            // the host environment; it is never a frontend recording option.
            let synthetic = synthetic_enabled();
            recording::run_observed(
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
                Some(&|frame| {
                    let _ = preview_sender.try_send(frame);
                }),
            )
            .map_err(|e| format!("{e:#}"))
        };
        drop(preview_sender);
        // The session mutex resolves Stop/Discard against finalization before the job can become uploadable.
        if let Ok(mut session) = state.session.lock()
            && session.id == id
        {
            if session.discard_requested {
                let cloud_result = state.cloud.discard(&id);
                let local_result = discard_files(&options.output);
                session.snapshot.phase = "discarded".into();
                session.snapshot.error = local_result.err().or_else(|| cloud_result.err());
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
            emit(&app, &snapshot);
            if close {
                app.exit(0);
            }
        }
    });
    Ok(initial)
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
    let path = fs::canonicalize(folder.join(file_name))
        .map_err(|_| "A gravação não está mais nessa pasta.".to_string())?;
    if path.parent() != Some(folder.as_path()) || !path.is_file() {
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
        for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
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
            if let Some(controls) = app.get_webview_window("controls")
                && let Some(main) = app.get_webview_window("main")
                && let Ok(Some(monitor)) = main.current_monitor()
            {
                let size = monitor.size();
                let origin = monitor.position();
                let scale = monitor.scale_factor();
                let width = (490.0 * scale) as i32;
                let height = (56.0 * scale) as i32;
                let _ = controls.set_position(tauri::PhysicalPosition::new(
                    origin.x + (size.width as i32 - width).max(0) / 2,
                    origin.y + (size.height as i32 - height - (40.0 * scale) as i32).max(0),
                ));
            }
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
                }),
            }));
            tray::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quit_app,
            hide_to_tray,
            get_snapshot,
            get_preview,
            show_recorder,
            show_controls,
            get_tools,
            get_sources,
            get_folder,
            choose_folder,
            start_recording,
            control_recording,
            list_recordings,
            open_recording,
            open_folder,
            cloud_state,
            cloud_connect,
            cloud_disconnect,
            cloud_enqueue,
            cloud_retry,
            cloud_open,
            cloud_copy
        ])
        .on_window_event(|window, event| {
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
    fn recording_names_cannot_contain_paths_or_be_empty() {
        assert_eq!(safe_title("../../minha: gravação?"), "minha gravação");
        assert_eq!(safe_title(".. / "), "Gravação");
        assert_eq!(safe_title("Uma conversa"), "Uma conversa");
    }
}
