//! Device discovery runs away from the UI and never holds a recording-session lock.
use crate::*;

#[tauri::command]
pub async fn get_tools(app: tauri::AppHandle) -> Reply<Tools> {
    tauri::async_runtime::spawn_blocking(move || check_tools(&app))
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn get_sources() -> Reply<Sources> {
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
pub async fn get_cameras(app: tauri::AppHandle) -> Reply<Vec<tinrec_capture::camera::Device>> {
    tauri::async_runtime::spawn_blocking(move || {
        tinrec_capture::camera::devices(&tool_path(&app, "ffmpeg")).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
