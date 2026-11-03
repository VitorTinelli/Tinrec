use crate::{Reply, Shared, lock, now_ms, safe_title};
use base64::Engine;
use image::{GenericImageView, ImageEncoder};
use serde::Serialize;
use std::{
    fs,
    io::Cursor,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
pub struct Prints {
    pub busy: AtomicBool,
    area: Mutex<Option<PrintArea>>,
    #[cfg(target_os = "linux")]
    portal_ready: AtomicBool,
}
struct PrintArea {
    preview: crate::Preview,
    sender: mpsc::Sender<Option<pinray::Rect>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capture {
    id: String,
    image: String,
    width: u32,
    height: u32,
    needs_crop: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintFile {
    file_name: String,
    title: String,
    bytes: u64,
    modified_ms: u64,
    thumbnail: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    file_name: Option<String>,
    upload_id: Option<String>,
}
fn decode(bytes: &[u8]) -> Reply<image::DynamicImage> {
    if bytes.len() > 80 * 1024 * 1024 {
        return Err("Imagem muito grande.".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16000);
    limits.max_image_height = Some(16000);
    limits.max_alloc = Some(200 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "PNG inválido ou muito grande.")?;
    if u64::from(image.width()) * u64::from(image.height()) > 40_000_000 {
        return Err("Limite de 40 megapixels excedido.".into());
    }
    Ok(image)
}
fn png(image: &image::DynamicImage) -> Reply<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let bytes = bytes.into_inner();
    if bytes.len() > 80 * 1024 * 1024 {
        return Err("PNG acima do limite de 80 MiB.".into());
    }
    Ok(bytes)
}
fn encoded(image: &image::DynamicImage) -> Reply<String> {
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png(image)?)
    ))
}
#[cfg(target_os = "linux")]
async fn portal_frame() -> Reply<image::DynamicImage> {
    let response = ashpd::desktop::screenshot::Screenshot::request()
        .interactive(false)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    let uri = reqwest::Url::parse(response.uri().as_str()).map_err(|e| e.to_string())?;
    if uri.scheme() != "file" || uri.host_str().is_some_and(|host| host != "localhost") {
        return Err("Imagem não local".into());
    }
    let path = uri.to_file_path().map_err(|_| "Imagem inacessível")?;
    tauri::async_runtime::spawn_blocking(move || {
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > 80 * 1024 * 1024 {
            return Err("Imagem inválida ou muito grande".into());
        }
        decode(&fs::read(path).map_err(|e| e.to_string())?)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn print_display(wayland: bool, display: String) -> String {
    if wayland && display == "auto" {
        "print-monitor".to_string()
    } else {
        display
    }
}
fn native_frame(mode: &str, display: &str, window: &str) -> Reply<image::DynamicImage> {
    use pinray::{CaptureEvent, CaptureSession, PixelFormat, SourceId, VideoCaptureTarget};
    let target = if mode == "window" {
        VideoCaptureTarget::Window(SourceId::new(window))
    } else {
        VideoCaptureTarget::Display(SourceId::new(display))
    };
    let builder = CaptureSession::builder()
        .video_target(target)
        .pixel_format(PixelFormat::Bgra8888)
        .queue_depth(2);
    let mut capture = builder.build().map_err(|e| e.to_string())?;
    struct Stop<'a>(&'a mut CaptureSession);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            let _ = self.0.stop();
        }
    }
    let session = Stop(&mut capture);
    session.0.start().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match session.0.next_event(Some(Duration::from_millis(50))) {
            Ok(CaptureEvent::Video(frame)) => {
                if u64::from(frame.width) * u64::from(frame.height) > 40_000_000 {
                    return Err("Limite de 40 megapixels excedido.".into());
                }
                let mut data = frame
                    .to_tight_bytes()
                    .ok_or("Imagem da captura indisponível")?;
                if frame.pixel_format != PixelFormat::Bgra8888 {
                    return Err("Formato de pixels não suportado.".into());
                }
                for pixel in data.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2)
                }
                return image::RgbaImage::from_raw(frame.width, frame.height, data)
                    .map(image::DynamicImage::ImageRgba8)
                    .ok_or("Buffer de imagem inválido".into());
            }
            Err(pinray::PinrayError::Timeout(_)) => {}
            Err(e) => return Err(e.to_string()),
            Ok(CaptureEvent::End) => return Err("Captura encerrada.".into()),
            _ => {}
        }
    }
    Err("A captura não enviou uma imagem.".into())
}
#[tauri::command]
pub fn begin_print_edit(state: State<'_, Shared>, prints: State<'_, Prints>) -> Reply<()> {
    let session = lock(&state.session)?;
    if session.snapshot.busy() || prints.busy.swap(true, Ordering::AcqRel) {
        return Err("Finalize a captura atual.".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn capture_print(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    prints: State<'_, Prints>,
    mode: String,
    display: String,
    window: String,
) -> Reply<Capture> {
    if !["screen", "window", "area"].contains(&mode.as_str()) {
        return Err("Modo inválido.".into());
    }
    {
        let session = lock(&state.session)?;
        if session.snapshot.busy() || prints.busy.swap(true, Ordering::AcqRel) {
            return Err("Finalize a captura atual antes de tirar um print.".into());
        }
    }
    let main = app
        .get_webview_window("main")
        .ok_or("Janela indisponível")?;
    let restore = main.is_visible().unwrap_or(false);
    let _ = main.hide();
    let result: Reply<Capture> = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
        let requested_display = display.clone();
        let monitors = main.available_monitors().map_err(|e| e.to_string())?;
        let simple_portal = wayland && mode != "window" && monitors.len() == 1;
        let image = if crate::synthetic_enabled() {
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(640, 360, |x, y| {
                image::Rgba([(x % 256) as u8, (y % 256) as u8, 80, 255])
            }))
        } else if simple_portal {
            #[cfg(target_os = "linux")]
            {
                if !prints.portal_ready.load(Ordering::Acquire) {
                    let _ = main.show();
                    let _ = main.set_focus();
                    portal_frame().await?;
                    prints.portal_ready.store(true, Ordering::Release);
                }
                let _ = main.hide();
                tokio::time::sleep(Duration::from_millis(200)).await;
                let frame = portal_frame().await?;
                let monitor = &monitors[0];
                if frame.width() != monitor.size().width || frame.height() != monitor.size().height
                {
                    return Err(
                        "O portal retornou uma imagem diferente do monitor autorizado.".into(),
                    );
                }
                frame
            }
            #[cfg(not(target_os = "linux"))]
            {
                return Err("Portal indisponível".into());
            }
        } else {
            let m = mode.clone();
            // A non-auto display target restricts the Wayland portal to monitors.
            // Area selection belongs to Tinrec, never to the portal's region picker.
            let capture_display = print_display(wayland, display);
            tauri::async_runtime::spawn_blocking(move || {
                native_frame(&m, &capture_display, &window)
            })
            .await
            .map_err(|e| e.to_string())??
        };
        let image = if mode == "area" {
            let (sender, receiver) = mpsc::channel();
            let preview = crate::Preview {
                session_id: uuid::Uuid::new_v4().to_string(),
                sequence: 1,
                image: encoded(&image)?,
                width: image.width(),
                height: image.height(),
                source_width: image.width(),
                source_height: image.height(),
            };
            let monitors = main.available_monitors().map_err(|e| e.to_string())?;
            let source_name = pinray::enumerate_sources().ok().and_then(|sources| {
                sources.into_iter().find_map(|source| match source {
                    pinray::CaptureSource::Display(value) if value.id.0 == requested_display => {
                        Some(value.name)
                    }
                    _ => None,
                })
            });
            let matches = monitors
                .iter()
                .filter(|m| {
                    crate::source_matches_monitor(
                        m.size().width,
                        m.size().height,
                        m.scale_factor(),
                        image.width(),
                        image.height(),
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            let monitor = monitors
                .iter()
                .find(|m| {
                    source_name
                        .as_deref()
                        .is_some_and(|name| m.name().is_some_and(|v| v == name))
                })
                .cloned()
                .or_else(|| {
                    if requested_display == "auto" && !wayland {
                        main.primary_monitor().ok().flatten()
                    } else {
                        None
                    }
                })
                .or_else(|| (matches.len() == 1).then(|| matches[0].clone()))
                .or_else(|| (monitors.len() == 1).then(|| monitors[0].clone()))
                .ok_or(
                    "Não foi possível identificar o monitor sem ambiguidade. Escolha outra fonte.",
                )?;
            *lock(&prints.area)? = Some(PrintArea {
                preview: preview.clone(),
                sender,
            });
            let overlay = app
                .get_webview_window("print-area")
                .ok_or("Seletor indisponível")?;
            overlay
                .set_position(*monitor.position())
                .map_err(|e| e.to_string())?;
            overlay
                .set_size(*monitor.size())
                .map_err(|e| e.to_string())?;
            // Resizing across different DPI monitors can change the origin.
            overlay
                .set_position(*monitor.position())
                .map_err(|e| e.to_string())?;
            app.emit_to("print-area", "print-area-selection", preview)
                .map_err(|e| e.to_string())?;
            overlay.show().map_err(|e| e.to_string())?;
            overlay.set_focus().map_err(|e| e.to_string())?;
            let result = tauri::async_runtime::spawn_blocking(move || {
                receiver.recv_timeout(Duration::from_secs(1800))
            })
            .await
            .map_err(|e| e.to_string())?;
            let _ = overlay.hide();
            lock(&prints.area)?.take();
            let rect = result
                .map_err(|_| "Tempo de seleção esgotado")?
                .ok_or("Captura cancelada")?;
            image.crop_imm(rect.x as u32, rect.y as u32, rect.width, rect.height)
        } else {
            image
        };
        let (width, height) = image.dimensions();
        Ok(Capture {
            id: uuid::Uuid::new_v4().to_string(),
            image: encoded(&image)?,
            width,
            height,
            needs_crop: false,
        })
    }
    .await;
    if restore || result.is_ok() {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if result.is_err() {
        if let Ok(mut pending) = prints.area.lock() {
            pending.take();
        }
        if let Some(window) = app.get_webview_window("print-area") {
            let _ = window.hide();
        }
        prints.busy.store(false, Ordering::Release);
    }
    result
}
#[tauri::command]
pub fn close_print(app: tauri::AppHandle, prints: State<'_, Prints>) {
    if let Ok(mut area) = prints.area.lock()
        && let Some(pending) = area.take()
    {
        let _ = pending.sender.send(None);
    }
    if let Some(window) = app.get_webview_window("print-area") {
        let _ = window.hide();
    }
    prints.busy.store(false, Ordering::Release);
}
#[tauri::command]
pub fn get_print_area(prints: State<'_, Prints>) -> Reply<Option<crate::Preview>> {
    Ok(lock(&prints.area)?
        .as_ref()
        .map(|area| area.preview.clone()))
}
#[tauri::command]
pub fn select_print_area(
    app: tauri::AppHandle,
    prints: State<'_, Prints>,
    session_id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Reply<()> {
    let mut pending = lock(&prints.area)?;
    let area = pending.as_ref().ok_or("Seleção encerrada")?;
    if area.preview.session_id != session_id
        || x < 0
        || y < 0
        || width < 2
        || height < 2
        || x as u64 + u64::from(width) > u64::from(area.preview.source_width)
        || y as u64 + u64::from(height) > u64::from(area.preview.source_height)
    {
        return Err("Área inválida.".into());
    }
    if let Some(window) = app.get_webview_window("print-area") {
        window.hide().map_err(|e| e.to_string())?;
    }
    area.sender
        .send(Some(pinray::Rect {
            x,
            y,
            width,
            height,
        }))
        .map_err(|_| "Captura encerrada")?;
    pending.take();
    Ok(())
}
fn print_path(state: &crate::AppState, name: &str) -> Reply<std::path::PathBuf> {
    if std::path::Path::new(name)
        .file_name()
        .and_then(|v| v.to_str())
        != Some(name)
        || !name.ends_with(".png")
    {
        return Err("Print inválido.".into());
    }
    let root = fs::canonicalize(lock(&state.folder)?.join("Prints")).map_err(|e| e.to_string())?;
    let path = fs::canonicalize(root.join(name)).map_err(|e| e.to_string())?;
    if path.parent() != Some(root.as_path()) || !path.is_file() {
        return Err("Print fora da biblioteca.".into());
    }
    Ok(path)
}
#[tauri::command]
pub fn get_print_keep_local(state: State<'_, Shared>) -> Reply<bool> {
    Ok(fs::read(state.config.with_file_name("print-settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<bool>(&b).ok())
        .unwrap_or(true))
}
#[tauri::command]
pub fn set_print_keep_local(state: State<'_, Shared>, keep: bool) -> Reply<()> {
    fs::write(
        state.config.with_file_name("print-settings.json"),
        if keep { "true" } else { "false" },
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn export_print(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    image: String,
    title: String,
    action: String,
) -> Reply<Export> {
    if !["copy", "save", "upload"].contains(&action.as_str()) {
        return Err("Ação inválida.".into());
    }
    if image.len() > 110 * 1024 * 1024 {
        return Err("Imagem muito grande.".into());
    }
    let data = image
        .strip_prefix("data:image/png;base64,")
        .ok_or("Formato de imagem inválido")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "Imagem inválida")?;
    let decoded = tauri::async_runtime::spawn_blocking(move || decode(&bytes))
        .await
        .map_err(|e| e.to_string())??;
    let keep_local = action == "save" || get_print_keep_local(state.clone())?;
    let mut clipboard_error = None;
    if action == "copy" {
        let rgba = decoded.to_rgba8();
        clipboard_error = app
            .clipboard()
            .write_image(&tauri::image::Image::new(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
            ))
            .err()
            .map(|e| e.to_string());
        if !keep_local {
            if let Some(error) = clipboard_error {
                return Err(error);
            }
            return Ok(Export {
                file_name: None,
                upload_id: None,
            });
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let title = if title.trim().is_empty() {
        format!("Print - {}", now_ms())
    } else {
        safe_title(title.trim())
    };
    let folder = if keep_local {
        lock(&state.folder)?.join("Prints")
    } else {
        state.config.with_file_name("pending-prints")
    };
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name = format!("{title} - {}.png", &id[..8]);
    let path = folder.join(&name);
    let bytes = png(&decoded)?;
    let temporary = folder.join(format!(".{id}.partial"));
    {
        use std::io::Write;
        let mut file = fs::File::create_new(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    let upload_id = if action == "upload" {
        state
            .cloud
            .enqueue(id.clone(), path, title, 1080, 0., false)?;
        if !keep_local {
            state.cloud.remove_local_after_upload(&id)?;
        }
        Some(id)
    } else {
        None
    };
    if let Some(error) = clipboard_error {
        return Err(format!("Print salvo. Não foi possível copiar: {error}"));
    }
    Ok(Export {
        file_name: Some(name),
        upload_id,
    })
}
#[tauri::command]
pub async fn list_prints(state: State<'_, Shared>) -> Reply<Vec<PrintFile>> {
    let folder = lock(&state.folder)?.join("Prints");
    tauri::async_runtime::spawn_blocking(move || {
        if !folder.exists() {
            return Ok(Vec::new());
        }
        let mut files = fs::read_dir(&folder)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().is_some_and(|e| e == "png")
                    && entry.file_type().is_ok_and(|t| t.is_file())
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|f| std::cmp::Reverse(f.metadata().ok().and_then(|m| m.modified().ok())));
        let mut out = Vec::new();
        for file in files.into_iter().take(100) {
            let meta = file.metadata().map_err(|e| e.to_string())?;
            if meta.len() > 80 * 1024 * 1024 {
                continue;
            }
            let image = match decode(&fs::read(file.path()).map_err(|e| e.to_string())?) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let small = image.thumbnail(360, 200).to_rgb8();
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 75)
                .write_image(
                    small.as_raw(),
                    small.width(),
                    small.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| e.to_string())?;
            let name = file.file_name().to_string_lossy().to_string();
            out.push(PrintFile {
                title: name.trim_end_matches(".png").into(),
                file_name: name,
                bytes: meta.len(),
                modified_ms: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |v| v.as_millis() as u64),
                thumbnail: format!(
                    "data:image/jpeg;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(jpeg)
                ),
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn open_print(app: tauri::AppHandle, state: State<'_, Shared>, file_name: String) -> Reply<()> {
    let path = print_path(&state, &file_name)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn upload_print(state: State<'_, Shared>, file_name: String) -> Reply<()> {
    let path = print_path(&state, &file_name)?;
    state.cloud.enqueue(
        uuid::Uuid::new_v4().to_string(),
        path,
        file_name.trim_end_matches(".png").to_string(),
        1080,
        0.,
        false,
    )
}
#[tauri::command]
pub fn print_site(app: tauri::AppHandle, state: State<'_, Shared>, file_name: String) -> Reply<()> {
    let url = state
        .cloud
        .recording_url(&print_path(&state, &file_name)?)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_print_project(
    state: State<'_, Shared>,
    document: String,
    title: String,
) -> Reply<String> {
    if document.len() > 110 * 1024 * 1024 {
        return Err("Rascunho muito grande.".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(&document).map_err(|_| "Rascunho inválido")?;
    if value["version"] != 1 || !value["operations"].is_array() {
        return Err("Rascunho inválido.".into());
    }
    let folder = state.config.with_file_name("print-projects");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let path = folder.join(format!(
        "{}-{}.tinrecprint.json",
        safe_title(&title),
        uuid::Uuid::new_v4()
    ));
    use std::io::Write;
    let mut file = fs::File::create_new(&path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    file.write_all(document.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into())
}
#[tauri::command]
pub async fn open_print_project(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Reply<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let folder = state.config.with_file_name("print-projects");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(selected) = app
            .dialog()
            .file()
            .set_directory(folder)
            .add_filter("Rascunho Tinrec", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|e| e.to_string())?;
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 110 * 1024 * 1024 {
            return Err("Rascunho muito grande.".into());
        }
        Ok(Some(fs::read_to_string(path).map_err(|e| e.to_string())?))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wayland_prints_restrict_portal_to_monitors_without_changing_other_platforms() {
        assert_ne!(print_display(true, "auto".into()), "auto");
        assert_eq!(print_display(false, "auto".into()), "auto");
        assert_eq!(print_display(true, "DP-1".into()), "DP-1");
    }
    #[test]
    fn png_roundtrip_preserves_pixels() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            2,
            image::Rgba([10, 20, 30, 255]),
        ));
        let result = decode(&png(&image).unwrap()).unwrap();
        assert_eq!(result.to_rgba8().get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert_eq!(result.dimensions(), (3, 2));
    }
    #[test]
    fn invalid_images_are_rejected() {
        assert!(decode(b"not an image").is_err());
    }
}
