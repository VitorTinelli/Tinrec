//! Non-interactive, capture-excluded outline. Native calls never hold session locks.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
fn bounds(origin: (i32, i32), size: (u32, u32), region: (f64, f64, f64, f64)) -> Option<Bounds> {
    let (x, y, width, height) = region;
    if ![x, y, width, height].iter().all(|v| v.is_finite()) || width <= 0. || height <= 0. {
        return None;
    }
    let left = f64::from(origin.0) + (x * f64::from(size.0)).round();
    let top = f64::from(origin.1) + (y * f64::from(size.1)).round();
    Some(Bounds {
        x: left as i32,
        y: top as i32,
        width: (width * f64::from(size.0)).round().max(1.) as u32,
        height: (height * f64::from(size.1)).round().max(1.) as u32,
    })
}
pub(crate) fn target_bounds(target: &windows::WindowTarget) -> Option<Bounds> {
    bounds(
        (target.monitor.position().x, target.monitor.position().y),
        (target.monitor.size().width, target.monitor.size().height),
        target.region,
    )
}
pub(crate) fn current(app: &tauri::AppHandle, state: &AppState) -> Option<Bounds> {
    // Copy geometry while holding the session lock; release it before monitor APIs.
    let (mode, source, monitor, region) = {
        let session = state.session.lock().ok()?;
        if !matches!(session.snapshot.phase.as_str(), "recording" | "paused") {
            return None;
        }
        (
            session.capture_mode.clone(),
            session.window_source.clone(),
            state.capture_monitor.lock().ok()?.clone(),
            *state.webcam_region.lock().ok()?,
        )
    };
    match mode.as_str() {
        "window" => target_bounds(&windows::window_target(app, &source).ok()?),
        "area" => {
            let monitor = monitor?;
            bounds(
                (monitor.position().x, monitor.position().y),
                (monitor.size().width, monitor.size().height),
                region?,
            )
        }
        _ => None,
    }
}
pub(crate) fn excluded(window: &tauri::WebviewWindow) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowDisplayAffinity;
        let Ok(handle) = window.hwnd() else {
            return false;
        };
        let mut affinity = 0;
        unsafe { GetWindowDisplayAffinity(handle.0, &mut affinity) != 0 && affinity == 0x11 }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        false
    }
}
pub fn follow(app: tauri::AppHandle, state: Shared) {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(window) = app.get_webview_window("capture-border") else {
            return;
        };
        // Older Windows versions can turn a protected transparent window into
        // a black rectangle in recordings. Show it only with true exclusion.
        if !excluded(&window) {
            eprintln!("Contorno desativado: exclusão da captura indisponível neste Windows.");
            return;
        }
        let mut previous = None;
        while !state.exiting.load(Ordering::Acquire) {
            let next = current(&app, &state);
            if next != previous {
                let placed = if let Some(b) = next {
                    window
                        .set_position(tauri::PhysicalPosition::new(b.x, b.y))
                        .and_then(|_| window.set_size(tauri::PhysicalSize::new(b.width, b.height)))
                        // Crossing DPI boundaries can adjust the origin during resize.
                        .and_then(|_| window.set_position(tauri::PhysicalPosition::new(b.x, b.y)))
                        .and_then(|_| window.show())
                } else {
                    window.hide()
                };
                if placed.is_ok() {
                    previous = next;
                } else {
                    let _ = window.hide();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = window.hide();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outline_uses_physical_pixels_with_negative_origins_and_cross_monitor_windows() {
        assert_eq!(
            bounds((-2560, -400), (2560, 1440), (0.25, 0.25, 0.5, 0.5)),
            Some(Bounds {
                x: -1920,
                y: -40,
                width: 1280,
                height: 720
            })
        );
        // A window can extend beyond its center monitor; never clamp it to that screen.
        assert_eq!(
            bounds((0, 0), (1920, 1080), (-0.25, 0., 1.25, 1.)),
            Some(Bounds {
                x: -480,
                y: 0,
                width: 2400,
                height: 1080
            })
        );
        assert!(bounds((0, 0), (1920, 1080), (f64::NAN, 0., 1., 1.)).is_none());
    }
}
