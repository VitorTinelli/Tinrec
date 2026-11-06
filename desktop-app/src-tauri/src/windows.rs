//! Monitor identity and overlay placement used by the Windows desktop shell.
use crate::*;

pub struct WindowTarget {
    pub monitor: tauri::Monitor,
    pub region: (f64, f64, f64, f64),
}

pub fn window_target(app: &tauri::AppHandle, source: &str) -> Reply<WindowTarget> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::{
            Foundation::RECT,
            Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
            UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindow},
        };
        let handle = source
            .strip_prefix("window:")
            .and_then(|s| s.parse::<isize>().ok())
            .ok_or("Janela de captura inválida")? as *mut std::ffi::c_void;
        if unsafe { IsWindow(handle) } == 0 {
            return Err("A janela selecionada foi fechada. Atualize as fontes.".into());
        }
        if unsafe { IsIconic(handle) } != 0 {
            return Err("A janela selecionada está minimizada.".into());
        }
        let mut bounds = RECT::default();
        if unsafe {
            DwmGetWindowAttribute(
                handle,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut bounds as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            )
        } < 0
            && unsafe { GetWindowRect(handle, &mut bounds) } == 0
        {
            return Err("Não foi possível localizar a janela selecionada.".into());
        }
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            return Err("A janela selecionada não possui uma área válida.".into());
        }
        let monitor = app
            .monitor_from_point(
                (f64::from(bounds.left) + f64::from(bounds.right)) / 2.,
                (f64::from(bounds.top) + f64::from(bounds.bottom)) / 2.,
            )
            .map_err(|e| e.to_string())?
            .ok_or("Monitor da janela indisponível")?;
        let width = f64::from(monitor.size().width);
        let height = f64::from(monitor.size().height);
        let region = (
            f64::from(bounds.left - monitor.position().x) / width,
            f64::from(bounds.top - monitor.position().y) / height,
            f64::from(bounds.right - bounds.left) / width,
            f64::from(bounds.bottom - bounds.top) / height,
        );
        Ok(WindowTarget { monitor, region })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, source);
        Err("Identificação de janela nativa disponível apenas no Windows".into())
    }
}

pub fn capture_monitor(app: &tauri::AppHandle, display: &str) -> Reply<tauri::Monitor> {
    let main = app
        .get_webview_window("main")
        .ok_or("Janela indisponível")?;
    let monitors = main.available_monitors().map_err(|e| e.to_string())?;
    if display == "auto" || display.is_empty() {
        return main
            .primary_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("Monitor principal indisponível".into());
    }
    let name = display
        .strip_prefix("display:")
        .ok_or("Identificador de monitor inválido")?;
    monitors
        .into_iter()
        .find(|m| m.name().is_some_and(|n| n == name))
        .ok_or("Não foi possível identificar o monitor selecionado com segurança.".into())
}

/// Follow only the selected window, and stop as soon as its recording ends.
pub fn follow_capture_window(app: tauri::AppHandle, state: Shared, id: String, source: String) {
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            let active = state
                .session
                .lock()
                .is_ok_and(|session| session.id == id && session.snapshot.busy());
            if !active || state.exiting.load(Ordering::Acquire) {
                break;
            }
            if let Ok(target) = window_target(&app, &source) {
                let Ok(session) = state.session.lock() else {
                    break;
                };
                if session.id != id || !session.snapshot.busy() {
                    break;
                }
                let changed = state.capture_monitor.lock().is_ok_and(|current| {
                    current.as_ref().is_none_or(|m| {
                        m.position() != target.monitor.position()
                            || m.size() != target.monitor.size()
                            || m.scale_factor() != target.monitor.scale_factor()
                    })
                }) || state
                    .webcam_region
                    .lock()
                    .is_ok_and(|r| *r != Some(target.region));
                if changed {
                    if let Ok(mut current) = state.capture_monitor.lock() {
                        *current = Some(target.monitor);
                    }
                    if let Ok(mut region) = state.webcam_region.lock() {
                        *region = Some(target.region);
                    }
                    drop(session);
                    position_webcam_window(&app);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
}
