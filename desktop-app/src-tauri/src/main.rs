#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cloud;
mod cloud_commands;
mod devices;
mod diagnostics;
mod library;
mod pages;
mod recorder;
use recorder::control_recording;
mod area_picker;
mod capture_border;
mod print_frame;
mod screenshots;
mod shortcuts;
mod smoke;
mod tray;
mod windows;
use base64::Engine;
#[cfg(test)]
use library::extract_thumbnail;
use library::recording_path;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_selection: Option<area_picker::Region>,
}

struct Session {
    snapshot: Snapshot,
    controls: Option<mpsc::Sender<Control>>,
    id: String,
    exit_requested: bool,
    discard_requested: bool,
    preview: Option<Preview>,
    capture_mode: String,
    window_source: String,
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
    capture_monitor: Mutex<Option<tauri::Monitor>>,
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
    #[serde(default)]
    crop: Option<area_picker::Selection>,
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
    let out = tinrec_capture::process::output(
        native_command(path).arg("-version"),
        std::time::Duration::from_secs(5),
    )
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
        && tinrec_capture::process::output(
            native_command(&tool_path(app, "ffmpeg")).args(["-hide_banner", "-encoders"]),
            std::time::Duration::from_secs(5),
        )
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
fn state_monitor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    app.state::<Shared>()
        .capture_monitor
        .lock()
        .ok()
        .and_then(|m| m.clone())
}
fn position_webcam_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("webcam") else {
        return;
    };
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let monitor = state_monitor(app).or_else(|| main.current_monitor().ok().flatten());
    let Some(monitor) = monitor else {
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
    let Some(monitor) = state_monitor(app).or_else(|| window.current_monitor().ok().flatten())
    else {
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
        .register_uri_scheme_protocol("tinrec-print", |context, request| {
            screenshots::frame_response(
                context.app_handle(),
                context.webview_label(),
                request.uri().path(),
            )
        })
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
                capture_monitor: Mutex::new(None),
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
                    window_source: String::new(),
                    webcam_preview: None,
                }),
            }));
            app.manage(screenshots::Prints::default());
            app.manage(area_picker::Picker::default());
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
            if cfg!(target_os = "windows") {
                let border = app
                    .get_webview_window("capture-border")
                    .ok_or("Contorno indisponível")?;
                border.set_ignore_cursor_events(true)?;
                border.set_focusable(false)?;
                capture_border::follow(app.handle().clone(), app.state::<Shared>().inner().clone());
            }
            position_webcam_window(app.handle());
            tray::setup(app)?;
            if cfg!(target_os = "windows") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = area_picker::warm(&handle).await {
                        eprintln!("Seletores: {error}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            area_picker::choose_area,
            area_picker::get_area_pick,
            area_picker::finish_area_pick,
            area_picker::cancel_area_pick,
            area_picker::self_test_area_pick,
            smoke::desktop_ready,
            smoke::self_test_mode,
            smoke::self_test_close_selector,
            smoke::self_test_print_transition,
            smoke::self_test_window,
            smoke::self_test_capture_border,
            screenshots::begin_print_edit,
            screenshots::get_print_area,
            screenshots::print_editor_ready,
            screenshots::self_test_print_ready,
            screenshots::select_print_area,
            screenshots::save_print_project,
            screenshots::open_print_project,
            screenshots::capture_print,
            screenshots::start_print_capture,
            screenshots::print_source_visibility,
            screenshots::close_print_source,
            screenshots::close_print,
            screenshots::export_print,
            screenshots::export_print_selection,
            screenshots::get_print_keep_local,
            screenshots::set_print_keep_local,
            screenshots::list_prints,
            screenshots::open_print,
            screenshots::print_site,
            screenshots::upload_print,
            shortcuts::local_shortcut,
            shortcuts::get_shortcuts,
            shortcuts::self_test_shortcuts,
            shortcuts::save_shortcuts,
            shortcuts::shortcut_capture,
            quit_app,
            hide_to_tray,
            recorder::get_snapshot,
            get_preview,
            show_recorder,
            show_controls,
            devices::get_tools,
            devices::get_sources,
            devices::get_cameras,
            recorder::select_area,
            get_webcam_preview,
            recorder::get_folder,
            recorder::choose_folder,
            recorder::start_recording,
            recorder::control_recording,
            library::list_recordings,
            library::open_recording,
            library::recording_thumbnail,
            library::open_folder,
            cloud_commands::cloud_state,
            cloud_commands::cloud_connect,
            cloud_commands::cloud_disconnect,
            cloud_commands::cloud_enqueue,
            cloud_commands::cloud_retry,
            cloud_commands::cloud_open,
            cloud_commands::cloud_open_recording,
            cloud_commands::cloud_copy
        ])
        .on_window_event(|window, event| {
            if window.label() == "webcam" && matches!(event, tauri::WindowEvent::Moved(_)) {
                save_webcam_position(window.app_handle());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "print-source" { return; }
                api.prevent_close();
                if area_picker::is_window(window.label()) {
                    area_picker::cancel(window.app_handle(), window.label());
                    screenshots::close_area_selector(window.app_handle());
                }
                match window.label() {
                    "area" => {
                        let _ = recorder::control_recording(
                            window.app_handle().clone(),
                            window.app_handle().state(),
                            "discard".into(),
                        );
                    }
                    "print-area" => screenshots::close_print(
                        window.app_handle().clone(),
                        window.app_handle().state(),
                    ),
                    _ => {}
                }
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
