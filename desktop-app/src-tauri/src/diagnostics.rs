//! Persistent diagnostics and user-visible errors; never write credentials.
use crate::*;
pub fn notify(app: &tauri::AppHandle, message: &str) {
    use std::io::Write;
    let _ = app.emit("desktop-error", message);
    let state = app.state::<Shared>();
    let path = state.config.with_file_name("diagnostics.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
        let _ = fs::rename(&path, path.with_extension("previous.log"));
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {message}", now_ms());
    }
}
