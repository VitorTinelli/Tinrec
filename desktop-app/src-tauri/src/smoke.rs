//! Installed-app diagnostic: native controller IPC followed by synthetic A/V verification.
use crate::*;
static STARTED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn self_test_mode() -> bool {
    std::env::args().any(|arg| arg == "--self-test")
        && std::env::var_os("TINREC_DATA_DIR").is_some()
}

#[tauri::command]
pub fn self_test_close_selector(app: tauri::AppHandle, label: String) -> Reply<()> {
    if !self_test_mode() || !["area", "print-area"].contains(&label.as_str()) {
        return Err("Comando disponível apenas no autoteste isolado".into());
    }
    let label = if label == "print-area" {
        screenshots::editor_label(&app).unwrap_or(label)
    } else {
        label
    };
    app.get_webview_window(&label)
        .ok_or("Seletor ausente")?
        .close()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn self_test_window(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    action: String,
) -> Reply<serde_json::Value> {
    if !self_test_mode() {
        return Err("Comando disponível apenas no autoteste isolado".into());
    }
    #[cfg(target_os = "windows")]
    {
        if action == "open" {
            let window = tauri::WebviewWindowBuilder::new(
                &app,
                "capture-fixture",
                tauri::WebviewUrl::App("index.html?window=capture-fixture".into()),
            )
            .title("Tinrec autoteste de janela")
            .inner_size(400., 300.)
            .position(100., 100.)
            .build()
            .map_err(|e| e.to_string())?;
            let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
            return Ok(serde_json::json!(format!("window:{hwnd}")));
        }
        let window = app
            .get_webview_window("capture-fixture")
            .ok_or("Janela de teste ausente")?;
        match action.as_str() {
            "probe" => {
                let monitor = app
                    .available_monitors()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .last()
                    .ok_or("Monitor ausente")?;
                window
                    .set_position(tauri::PhysicalPosition::new(
                        monitor.position().x + 40,
                        monitor.position().y + 40,
                    ))
                    .map_err(|e| e.to_string())?;
                window.show().map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
                let point = window.inner_position().map_err(|e| e.to_string())?;
                let sample =
                    crate::print_frame::capture(point.x + 8, point.y + 8, 1, 1)?.to_rgba8();
                let rgba = sample.get_pixel(0, 0).0;
                return Ok(
                    serde_json::json!({"x":point.x-monitor.position().x+8,"y":point.y-monitor.position().y+8,"rgba":rgba}),
                );
            }
            "move" => {
                let p = window.outer_position().map_err(|e| e.to_string())?;
                window
                    .set_position(tauri::PhysicalPosition::new(p.x + 80, p.y + 60))
                    .map_err(|e| e.to_string())?;
            }
            "aligned" => {
                let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
                let target = windows::window_target(&app, &format!("window:{hwnd}"))?;
                return Ok(serde_json::json!(
                    *lock(&state.webcam_region)? == Some(target.region)
                ));
            }
            "close" => window.close().map_err(|e| e.to_string())?,
            _ => return Err("Ação de teste inválida".into()),
        }
        Ok(serde_json::Value::Null)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, state, action);
        Err("Autoteste de janela disponível no Windows".into())
    }
}

#[tauri::command]
pub fn desktop_ready(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    error: Option<String>,
) -> Reply<bool> {
    if !std::env::args().any(|arg| arg == "--self-test") {
        return Ok(false);
    }
    if std::env::var_os("TINREC_DATA_DIR").is_none() {
        return Err("O autoteste exige uma pasta de dados isolada.".into());
    }
    if STARTED.swap(true, Ordering::AcqRel) {
        return Ok(true);
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| -> Reply<serde_json::Value> {
            if let Some(error) = error {
                return Err(error);
            }
            if snapshot_value(&state)?.phase != "discarded" {
                return Err("Sessão inicial inválida".into());
            }
            if lock(&state.capture_monitor)?.is_none() {
                return Err("Monitor de captura não foi selecionado pelo controlador".into());
            }
            let prints = app.state::<screenshots::Prints>();
            if prints.busy.load(Ordering::Acquire) {
                return Err("Captura de print não foi liberada".into());
            }
            let print_folder = lock(&state.folder)?.join("Prints");
            let native_print = fs::read_dir(print_folder)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .any(|entry| image::open(entry.path()).is_ok());
            if !native_print {
                return Err("Print nativo salvo não foi encontrado".into());
            }
            let tools = check_tools(&app);
            if !tools.ready {
                return Err(tools.message.unwrap_or_default());
            }
            let folder = state.config.parent().ok_or("Pasta de teste indisponível")?;
            let output = folder.join(format!("self-test-{}.mp4", uuid::Uuid::new_v4()));
            let options = RecordOptions {
                output,
                fast_finalize: false,
                duration: 1.2,
                system_audio: true,
                microphone: true,
                microphone_device: None,
                display: "auto".into(),
                capture_mode: "screen".into(),
                crop: None,
                webcam: None,
                webcam_disabled: true,
                webcam_visible_in_capture: false,
                webcam_x: 0.,
                webcam_y: 1.,
                backend: "auto".into(),
                resolution: 720,
                system_volume: 80,
                microphone_volume: 80,
                ffmpeg: tool_path(&app, "ffmpeg"),
                ffprobe: tool_path(&app, "ffprobe"),
                pause_after: None,
                pause_for: 1.,
            };
            let (_sender, receiver) = mpsc::channel();
            let report = recording::run_controlled(&options, true, receiver, |_| {})
                .map_err(|e| e.to_string())?;
            if report.status != "completed" {
                return Err(report.error.unwrap_or("Falha no encoder".into()));
            }
            Ok(
                serde_json::json!({"status":"passed", "version":env!("CARGO_PKG_VERSION"), "frontendIpc":true, "nativeController":true, "nativePrints":true, "selectorClose":true, "bundledTools":true, "recording":report}),
            )
        })();
        let success = result.is_ok();
        let report = result.unwrap_or_else(|e| serde_json::json!({"status":"failed","error":e}));
        let saved = serde_json::to_vec_pretty(&report).ok().is_some_and(|data| {
            fs::write(state.config.with_file_name("self-test-result.json"), data).is_ok()
        });
        app.exit(if success && saved { 0 } else { 1 });
    });
    Ok(true)
}

#[tauri::command]
pub async fn self_test_capture_border(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Reply<bool> {
    if !self_test_mode() {
        return Err("Comando exclusivo do autoteste".into());
    }
    let window = app
        .get_webview_window("capture-border")
        .ok_or("Contorno ausente")?;
    let Some(expected) = capture_border::current(&app, &state) else {
        return Ok(!window.is_visible().map_err(|e| e.to_string())?);
    };
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
        };
        let handle = window.hwnd().map_err(|e| e.to_string())?.0;
        let protected = capture_border::excluded(&window);
        let style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32;
        if !protected || style & WS_EX_TRANSPARENT == 0 || style & WS_EX_NOACTIVATE == 0 {
            return Err("Contorno não está excluído da captura ou bloqueia interação".into());
        }
    }
    Ok(window.is_visible().map_err(|e| e.to_string())?
        && position.x == expected.x
        && position.y == expected.y
        && size.width == expected.width
        && size.height == expected.height)
}

#[tauri::command]
pub async fn self_test_print_transition(app: tauri::AppHandle, label: Option<String>) -> Reply<()> {
    if !self_test_mode() {
        return Err("Comando exclusivo do autoteste".into());
    }
    let main = app
        .get_webview_window("main")
        .ok_or("Janela principal ausente")?;
    if main.is_visible().map_err(|e| e.to_string())? {
        return Err("O print reabriu a janela principal sem solicitação".into());
    }
    if let Some(label) = label {
        if screenshots::editor_label(&app).as_ref() != Some(&label) {
            return Err("O editor recriou a janela de seleção".into());
        }
        let editor = app.get_webview_window(&label).ok_or("Editor ausente")?;
        if !editor.is_visible().map_err(|e| e.to_string())? {
            return Err("O seletor foi ocultado durante a transição".into());
        }
        if app
            .get_webview_window("print-area")
            .is_some_and(|w| w.is_visible().unwrap_or(false))
        {
            return Err("O editor antigo foi aberto na transição".into());
        }
    } else if screenshots::editor_label(&app).is_some() {
        return Err("O editor do print ficou aberto".into());
    }
    Ok(())
}
