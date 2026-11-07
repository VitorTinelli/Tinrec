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

async fn prepare_capture(app: tauri::AppHandle) -> Reply<()> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let ui_app = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| -> Reply<()> {
            for label in ["main", "print-source"] {
                if let Some(window) = ui_app.get_webview_window(label) {
                    #[cfg(target_os = "windows")]
                    window
                        .set_content_protected(true)
                        .map_err(|e| e.to_string())?;
                    window.hide().map_err(|e| e.to_string())?;
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{
                            IsWindowVisible, SW_HIDE, ShowWindow,
                        };
                        let handle = window.hwnd().map_err(|e| e.to_string())?.0;
                        ShowWindow(handle, SW_HIDE);
                        if IsWindowVisible(handle) != 0 {
                            return Err(
                                "Não foi possível ocultar o Tinrec antes da captura.".into()
                            );
                        }
                    }
                }
            }
            Ok(())
        })();
        let _ = send.send(result);
    })
    .map_err(|e| e.to_string())?;
    receive
        .await
        .map_err(|_| "Preparação da captura interrompida".to_string())??;
    // Window visibility is not proof that the compositor has repainted the desktop.
    tokio::time::sleep(Duration::from_millis(250)).await;
    #[cfg(target_os = "windows")]
    tauri::async_runtime::spawn_blocking(|| unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmFlush();
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn start_print_capture(app: tauri::AppHandle, mode: String) -> Reply<()> {
    if !["screen", "window", "area"].contains(&mode.as_str()) {
        return Err("Modo inválido".into());
    }
    if let Some(window) = app.get_webview_window("print-source") {
        if !app.state::<Prints>().busy.load(Ordering::Acquire) {
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    if snapshot_busy(&app) {
        return Err("Finalize a captura atual.".into());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "print-source",
        tauri::WebviewUrl::App(format!("index.html?window=print-source&mode={mode}").into()),
    )
    .title("Tinrec — Selecionar captura")
    .inner_size(560.0, 440.0)
    .min_inner_size(360.0, 280.0)
    .center()
    .visible(false)
    .skip_taskbar(true)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn snapshot_busy(app: &tauri::AppHandle) -> bool {
    crate::snapshot_value(&app.state::<Shared>()).map_or(true, |s| s.busy())
        || app.state::<Prints>().busy.load(Ordering::Acquire)
        || app.state::<crate::area_picker::Picker>().active()
}
#[tauri::command]
pub fn print_source_visibility(app: tauri::AppHandle, visible: bool) -> Reply<()> {
    let window = app
        .get_webview_window("print-source")
        .ok_or("Seleção encerrada")?;
    if visible {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())
    } else {
        window.hide().map_err(|e| e.to_string())
    }
}
#[tauri::command]
pub fn close_print_source(app: tauri::AppHandle) -> Reply<()> {
    if let Some(window) = app.get_webview_window("print-source") {
        window.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Default)]
pub struct Prints {
    pub busy: AtomicBool,
    capturing: AtomicBool,
    cancelled: AtomicBool,
    editor: Mutex<Option<String>>,
    bitmap: Mutex<Option<(String, std::sync::Arc<Vec<u8>>)>>,
    area: Mutex<Option<PrintArea>>,
    #[cfg(target_os = "linux")]
    portal_ready: AtomicBool,
}
struct PrintArea {
    source: std::sync::Arc<image::DynamicImage>,
    prepared: Instant,
    ready: bool,
    preview: crate::Preview,
    sender: mpsc::Sender<Option<(pinray::Rect, bool)>>,
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
fn validated_export_png(bytes: Vec<u8>) -> Reply<(image::DynamicImage, Vec<u8>)> {
    if image::guess_format(&bytes).ok() != Some(image::ImageFormat::Png) {
        return Err("Formato de imagem inválido: esperado PNG".into());
    }
    let decoded = decode(&bytes)?;
    Ok((decoded, bytes))
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
fn preview_png(image: &image::DynamicImage) -> Reply<Vec<u8>> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut bytes = Vec::new();
    // A fixed lossless filter avoids evaluating every PNG filter for every row.
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Sub)
        .write_image(
            image.as_bytes(),
            image.width(),
            image.height(),
            image.color().into(),
        )
        .map_err(|e| e.to_string())?;
    if bytes.len() > 80 * 1024 * 1024 {
        return Err("PNG acima do limite de 80 MiB.".into());
    }
    Ok(bytes)
}
fn encoded_preview(image: &image::DynamicImage) -> Reply<String> {
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(preview_png(image)?)
    ))
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
#[cfg(any(not(target_os = "windows"), test))]
fn print_display(wayland: bool, display: String) -> String {
    if wayland && display == "auto" {
        "print-monitor".to_string()
    } else {
        display
    }
}
fn native_frame(
    mode: &str,
    display: &str,
    window: &str,
    cancelled: &AtomicBool,
) -> Reply<image::DynamicImage> {
    use pinray::{CaptureEvent, CaptureSession, PixelFormat, SourceId, VideoCaptureTarget};
    let target = if mode == "window" {
        VideoCaptureTarget::Window(SourceId::new(window))
    } else {
        VideoCaptureTarget::Display(SourceId::new(display))
    };
    let builder = CaptureSession::builder()
        .backend_preference(tinrec_capture::platform::capture_backend())
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
    if cancelled.load(Ordering::Acquire) {
        return Err("Captura cancelada".into());
    }
    session.0.start().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if cancelled.load(Ordering::Acquire) {
            return Err("Captura cancelada".into());
        }
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
pub fn begin_print_edit(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    prints: State<'_, Prints>,
) -> Reply<()> {
    let session = lock(&state.session)?;
    if session.snapshot.busy()
        || app.state::<crate::area_picker::Picker>().active()
        || prints.busy.swap(true, Ordering::AcqRel)
    {
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
    action: Option<String>,
) -> Reply<Capture> {
    let interactive = mode == "area" || action.as_deref() == Some("edit");
    if action
        .as_deref()
        .is_some_and(|action| !["copy", "save", "edit"].contains(&action))
        || (mode == "area" && action.is_some())
    {
        return Err("Ação de captura inválida".into());
    }
    if !["screen", "window", "area"].contains(&mode.as_str()) {
        return Err("Modo inválido.".into());
    }
    let main = app
        .get_webview_window("main")
        .ok_or("Janela indisponível")?;
    {
        let session = lock(&state.session)?;
        if session.snapshot.busy()
            || app.state::<crate::area_picker::Picker>().active()
            || prints.busy.swap(true, Ordering::AcqRel)
        {
            return Err("Finalize a captura atual antes de tirar um print.".into());
        }
        prints.cancelled.store(false, Ordering::Release);
        prints.capturing.store(true, Ordering::Release);
    }
    let restore = main.is_visible().unwrap_or(false) && !main.is_minimized().unwrap_or(false);
    let result: Reply<Capture> = async {
        prepare_capture(app.clone()).await?;
        let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
        let selection =
            if mode == "area" && cfg!(target_os = "windows") && !crate::synthetic_enabled() {
                Some(
                    crate::area_picker::pick(app.clone(), state.inner().clone(), true)
                        .await?
                        .ok_or("Captura cancelada")?,
                )
            } else {
                None
            };
        let prepare_started = Instant::now();
        let retained = selection.as_ref().and_then(|s| s.editor_window.clone());
        *lock(&prints.editor)? = retained.clone();
        if prints.cancelled.load(Ordering::Acquire) {
            return Err("Captura cancelada".into());
        }
        let display = selection.as_ref().map_or(display, |s| s.display.clone());
        if selection.is_some() && retained.is_none() && !cfg!(target_os = "windows") {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
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
            #[cfg(target_os = "windows")]
            if mode != "window" {
                let (origin, width, height) = if let Some(selection) = &selection {
                    let current = monitors
                        .iter()
                        .find(|monitor| {
                            monitor
                                .name()
                                .is_some_and(|name| selection.display == format!("display:{name}"))
                        })
                        .ok_or("O monitor foi desconectado. Selecione novamente.")?;
                    if selection.origin != Some((current.position().x, current.position().y))
                        || (selection.source_width, selection.source_height)
                            != (current.size().width, current.size().height)
                    {
                        return Err("O monitor mudou. Selecione novamente.".into());
                    }
                    (
                        selection.origin.ok_or("Origem do monitor indisponível")?,
                        selection.source_width,
                        selection.source_height,
                    )
                } else {
                    let monitor = crate::windows::capture_monitor(&app, &display)?;
                    (
                        (monitor.position().x, monitor.position().y),
                        monitor.size().width,
                        monitor.size().height,
                    )
                };
                let capture_app = app.clone();
                let source = display.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::print_frame::capture(origin.0, origin.1, width, height).or_else(|_| {
                        native_frame(
                            "screen",
                            &source,
                            "",
                            &capture_app.state::<Prints>().cancelled,
                        )
                    })
                })
                .await
                .map_err(|e| e.to_string())??
            } else {
                let capture_app = app.clone();
                let source = window.clone();
                let display = display.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    native_frame(
                        "window",
                        &display,
                        &source,
                        &capture_app.state::<Prints>().cancelled,
                    )
                })
                .await
                .map_err(|e| e.to_string())??
            }
            #[cfg(not(target_os = "windows"))]
            {
                let m = mode.clone();
                // A non-auto display target restricts the Wayland portal to monitors.
                // Area selection belongs to Tinrec, never to the portal's region picker.
                let capture_display = print_display(wayland, display);
                let capture_app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    native_frame(
                        &m,
                        &capture_display,
                        &window,
                        &capture_app.state::<Prints>().cancelled,
                    )
                })
                .await
                .map_err(|e| e.to_string())??
            }
        };
        if prints.cancelled.load(Ordering::Acquire) {
            return Err("Captura cancelada".into());
        }
        if mode != "area"
            && !interactive
            && let Some(action) = action
        {
            let (width, height) = (image.width(), image.height());
            let export_app = app.clone();
            let export_state = state.inner().clone();
            tauri::async_runtime::spawn_blocking(move || {
                export_image(
                    &export_app,
                    &export_state,
                    image,
                    None,
                    String::new(),
                    &action,
                )
            })
            .await
            .map_err(|e| e.to_string())??;
            return Ok(Capture {
                id: uuid::Uuid::new_v4().to_string(),
                image: String::new(),
                width,
                height,
                needs_crop: false,
            });
        }
        let image = if interactive {
            let image = std::sync::Arc::new(image);
            let (sender, receiver) = mpsc::channel();
            if let Some(selection) = &selection
                && (image.width(), image.height())
                    != (selection.source_width, selection.source_height)
            {
                return Err("O monitor mudou. Selecione a área novamente.".into());
            }
            let frame_id = uuid::Uuid::new_v4().to_string();
            let image_source = if cfg!(target_os = "windows") {
                let bitmap = crate::print_frame::bitmap(&image)?;
                if crate::smoke::self_test_mode() {
                    eprintln!(
                        "Print capture + bitmap: {} ms",
                        prepare_started.elapsed().as_millis()
                    );
                }
                *lock(&prints.bitmap)? = Some((frame_id.clone(), std::sync::Arc::new(bitmap)));
                format!("http://tinrec-print.localhost/{frame_id}")
            } else {
                encoded_preview(&image)?
            };
            let preview = crate::Preview {
                initial_selection: if mode == "area" {
                    selection.as_ref().map(|s| s.region)
                } else {
                    Some(crate::area_picker::Region {
                        x: 0,
                        y: 0,
                        width: image.width(),
                        height: image.height(),
                    })
                },
                session_id: frame_id,
                sequence: 1,
                image: image_source,
                width: image.width(),
                height: image.height(),
                source_width: image.width(),
                source_height: image.height(),
            };
            // A retained selector already occupies the correct monitor.
            if retained.is_none() {
                let monitors = main.available_monitors().map_err(|e| e.to_string())?;
                let source_name = pinray::enumerate_sources().ok().and_then(|sources| {
                    sources.into_iter().find_map(|source| match source {
                        pinray::CaptureSource::Display(value)
                            if value.id.0 == requested_display =>
                        {
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
                let monitor = if mode == "window" {
                    main.current_monitor()
                        .map_err(|e| e.to_string())?
                        .or_else(|| main.primary_monitor().ok().flatten())
                        .ok_or("Monitor indisponível")?
                } else if cfg!(target_os = "windows") && !crate::synthetic_enabled() {
                    crate::windows::capture_monitor(&app, &requested_display)?
                } else {
                    monitors
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
                )?
                };
                let overlay = app
                    .get_webview_window("print-area")
                    .ok_or("Seletor indisponível")?;
                overlay
                    .set_position(*monitor.position())
                    .map_err(|e| e.to_string())?;
                if mode == "window" {
                    let scale = (monitor.size().width as f64 / image.width() as f64)
                        .min(monitor.size().height as f64 / image.height() as f64)
                        .min(1.0);
                    let width = (image.width() as f64 * scale).round() as u32;
                    let height = (image.height() as f64 * scale).round() as u32;
                    overlay
                        .set_size(tauri::PhysicalSize::new(width, height))
                        .map_err(|e| e.to_string())?;
                    overlay
                        .set_position(tauri::PhysicalPosition::new(
                            monitor.position().x + ((monitor.size().width - width) / 2) as i32,
                            monitor.position().y + ((monitor.size().height - height) / 2) as i32,
                        ))
                        .map_err(|e| e.to_string())?;
                } else {
                    overlay
                        .set_size(*monitor.size())
                        .map_err(|e| e.to_string())?;
                    overlay
                        .set_position(*monitor.position())
                        .map_err(|e| e.to_string())?;
                }
            }
            *lock(&prints.area)? = Some(PrintArea {
                source: image.clone(),
                prepared: prepare_started,
                ready: false,
                preview: preview.clone(),
                sender,
            });
            let editor = retained.as_deref().unwrap_or("print-area");
            *lock(&prints.editor)? = Some(editor.into());
            let overlay = app
                .get_webview_window(editor)
                .ok_or("Seletor indisponível")?;
            // Focus the selected monitor, including keyboard-driven handoffs.
            overlay.set_focus().map_err(|e| e.to_string())?;
            app.emit_to(editor, "print-area-selection", preview)
                .map_err(|e| e.to_string())?;
            if retained.is_none() {
                overlay.show().map_err(|e| e.to_string())?;
                overlay.set_focus().map_err(|e| e.to_string())?;
            }
            let result = tauri::async_runtime::spawn_blocking(move || {
                receiver.recv_timeout(Duration::from_secs(1800))
            })
            .await
            .map_err(|e| e.to_string())?;
            lock(&prints.area)?.take();
            let (rect, return_image) = result
                .map_err(|_| "Tempo de seleção esgotado")?
                .ok_or("Captura cancelada")?;
            if !return_image {
                return Ok(Capture {
                    id: uuid::Uuid::new_v4().to_string(),
                    image: String::new(),
                    width: rect.width,
                    height: rect.height,
                    needs_crop: false,
                });
            }
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
    #[cfg(target_os = "windows")]
    for label in ["main", "print-source"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.set_content_protected(false);
        }
    }
    if restore {
        let _ = main.show();
        let _ = main.set_focus();
    }
    close_editor(&app, &prints);
    if result.is_err() {
        if let Ok(mut pending) = prints.area.lock() {
            pending.take();
        }
        if let Some(window) = app.get_webview_window("print-area") {
            let _ = window.hide();
        }
    }
    {
        let _session = lock(&state.session)?;
        prints.capturing.store(false, Ordering::Release);
        if result.is_err() {
            prints.busy.store(false, Ordering::Release);
        }
    }
    result
}
#[tauri::command]
pub fn close_print(app: tauri::AppHandle, prints: State<'_, Prints>) {
    if prints.capturing.load(Ordering::Acquire) {
        prints.cancelled.store(true, Ordering::Release);
    }
    close_editor(&app, &prints);
    if let Ok(mut area) = prints.area.lock()
        && let Some(pending) = area.take()
    {
        let _ = pending.sender.send(None);
    }
    if let Some(window) = app.get_webview_window("print-area") {
        let _ = window.hide();
    }
    if !prints.capturing.load(Ordering::Acquire) {
        prints.busy.store(false, Ordering::Release);
    }
}
#[tauri::command]
pub fn get_print_area(prints: State<'_, Prints>) -> Reply<Option<crate::Preview>> {
    Ok(lock(&prints.area)?
        .as_ref()
        .map(|area| area.preview.clone()))
}
#[tauri::command]
pub fn select_print_area(
    prints: State<'_, Prints>,
    session_id: String,
    return_image: Option<bool>,
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
    area.sender
        .send(Some((
            pinray::Rect {
                x,
                y,
                width,
                height,
            },
            return_image.unwrap_or(true),
        )))
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
fn preserve_unsent_print(path: &std::path::Path, library: &std::path::Path, error: &str) -> String {
    let recovery = library.join(path.file_name().unwrap_or_default());
    if path != recovery {
        let saved = fs::create_dir_all(library).and_then(|_| {
            tinrec_capture::platform::publish_file(path, &recovery).map_err(std::io::Error::other)
        });
        if let Err(recovery_error) = saved {
            return format!(
                "Envio não iniciado: {error}. Print preservado em {}. Não foi possível movê-lo para a biblioteca: {recovery_error}",
                path.display()
            );
        }
    }
    format!(
        "Envio não iniciado: {error}. Print preservado na biblioteca: {}",
        recovery.display()
    )
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
    let (decoded, bytes) =
        tauri::async_runtime::spawn_blocking(move || validated_export_png(bytes))
            .await
            .map_err(|e| e.to_string())??;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        export_image(&app, &state, decoded, Some(bytes), title, &action)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn export_image(
    app: &tauri::AppHandle,
    state: &Shared,
    decoded: image::DynamicImage,
    encoded: Option<Vec<u8>>,
    title: String,
    action: &str,
) -> Reply<Export> {
    let export_started = Instant::now();
    let keep_local = action == "save" || get_print_keep_local(app.state::<Shared>())?;
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
    let bytes = match encoded {
        Some(bytes) => bytes,
        None => preview_png(&decoded)?,
    };
    let temporary = folder.join(format!(".{id}.partial"));
    {
        use std::io::Write;
        let mut file = fs::File::create_new(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    let upload_id = if action == "upload" {
        let job_id = match state
            .cloud
            .enqueue(id.clone(), path.clone(), title, 1080, 0., false)
        {
            Ok(job) => job,
            Err(error) => {
                let library = lock(&state.folder)?.join("Prints");
                return Err(preserve_unsent_print(&path, &library, &error));
            }
        };
        if !keep_local {
            state.cloud.remove_local_after_upload(&job_id).map_err(|error| format!("O print está na fila, mas a política de limpeza não pôde ser salva: {error}. Arquivo preservado em {}", path.display()))?;
        }
        Some(job_id)
    } else {
        None
    };
    if let Some(error) = clipboard_error {
        return Err(format!("Print salvo. Não foi possível copiar: {error}"));
    }
    if crate::smoke::self_test_mode() {
        eprintln!(
            "Print export ({action}): {} ms",
            export_started.elapsed().as_millis()
        );
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
        path.clone(),
        file_name.trim_end_matches(".png").to_string(),
        1080,
        0.,
        false,
    )?;
    Ok(())
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

pub(crate) fn editor_label(app: &tauri::AppHandle) -> Option<String> {
    app.state::<Prints>()
        .editor
        .lock()
        .ok()
        .and_then(|editor| editor.clone())
}
fn close_editor(app: &tauri::AppHandle, prints: &Prints) {
    if let Ok(mut bitmap) = prints.bitmap.lock() {
        bitmap.take();
    }
    let editor = prints
        .editor
        .lock()
        .ok()
        .and_then(|mut editor| editor.take());
    if let Some(label) = editor
        && let Some(window) = app.get_webview_window(&label)
    {
        let _ = window.hide();
        if crate::area_picker::is_window(&label) {
            let _ = app.emit_to(&label, "area-reset", ());
        }
    }
}
pub(crate) fn close_area_selector(app: &tauri::AppHandle) {
    let prints = app.state::<Prints>();
    if prints.capturing.load(Ordering::Acquire) {
        close_print(app.clone(), prints);
    }
}

pub(crate) fn frame_response(
    app: &tauri::AppHandle,
    label: &str,
    path: &str,
) -> tauri::http::Response<Vec<u8>> {
    let prints = app.state::<Prints>();
    let allowed = editor_label(app).is_some_and(|editor| editor == label)
        || (label == "main" && crate::smoke::self_test_mode());
    let frame = if allowed {
        prints.bitmap.lock().ok().and_then(|frame| {
            frame
                .as_ref()
                .filter(|(id, _)| path.strip_prefix('/') == Some(id.as_str()))
                .map(|(_, bytes)| bytes.clone())
        })
    } else {
        None
    };
    let mut response = tauri::http::Response::builder()
        .header("Content-Type", "image/bmp")
        .header("Cache-Control", "no-store")
        .header("Access-Control-Allow-Origin", "*");
    let bytes = if let Some(frame) = frame {
        (*frame).clone()
    } else {
        response = response.status(404);
        Vec::new()
    };
    response.body(bytes).expect("valid frame response")
}
#[tauri::command]
pub fn print_editor_ready(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    prints: State<'_, Prints>,
    session_id: String,
) -> Reply<()> {
    if editor_label(&app).as_deref() != Some(window.label()) {
        return Err("Editor encerrado".into());
    }
    let mut area = lock(&prints.area)?;
    let area = area
        .as_mut()
        .filter(|area| area.preview.session_id == session_id)
        .ok_or("Captura encerrada")?;
    if !area.ready && crate::smoke::self_test_mode() {
        eprintln!(
            "Print selection -> editor painted: {} ms",
            area.prepared.elapsed().as_millis()
        );
    }
    area.ready = true;
    Ok(())
}
#[tauri::command]
pub fn self_test_print_ready(app: tauri::AppHandle) -> Reply<bool> {
    if !crate::smoke::self_test_mode() {
        return Err("Comando exclusivo do autoteste".into());
    }
    Ok(lock(&app.state::<Prints>().area)?
        .as_ref()
        .is_some_and(|area| area.ready))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionExport {
    session_id: String,
    rect: crate::area_picker::Region,
    title: String,
    action: String,
}
#[tauri::command]
pub async fn export_print_selection(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, Shared>,
    prints: State<'_, Prints>,
    request: SelectionExport,
) -> Reply<Export> {
    if !["copy", "save", "upload"].contains(&request.action.as_str()) {
        return Err("Ação inválida".into());
    }
    if editor_label(&app).as_deref() != Some(window.label())
        && !(window.label() == "main" && crate::smoke::self_test_mode())
    {
        return Err("Editor encerrado".into());
    }
    let source = {
        let area = lock(&prints.area)?;
        let area = area
            .as_ref()
            .filter(|area| area.preview.session_id == request.session_id)
            .ok_or("Captura encerrada")?;
        if !request
            .rect
            .valid(area.source.width(), area.source.height())
        {
            return Err("Área inválida".into());
        }
        area.source.clone()
    };
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rect = request.rect;
        let cropped = source.crop_imm(rect.x as u32, rect.y as u32, rect.width, rect.height);
        export_image(&app, &state, cropped, None, request.title, &request.action)
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
    fn failed_upload_moves_pending_print_into_recoverable_library_without_overwrite() {
        let dir =
            std::env::temp_dir().join(format!("tinrec-print-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("print.png");
        fs::write(&source, b"original png").unwrap();
        let library = dir.join("Prints");
        let message = preserve_unsent_print(&source, &library, "Conta desconectada");
        assert!(message.contains("biblioteca"));
        assert_eq!(
            fs::read(library.join("print.png")).unwrap(),
            b"original png"
        );
        assert!(!source.exists());
        fs::write(&source, b"another png").unwrap();
        let message = preserve_unsent_print(&source, &library, "offline");
        assert!(message.contains(&source.display().to_string()));
        assert_eq!(fs::read(&source).unwrap(), b"another png");
        assert_eq!(
            fs::read(library.join("print.png")).unwrap(),
            b"original png"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn png_roundtrip_preserves_pixels_and_rejects_invalid_data() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(7, 5, |x, y| {
            image::Rgba([
                (x * 30) as u8,
                (y * 40) as u8,
                ((x + y) * 10) as u8,
                128 + (x * y) as u8,
            ])
        }));
        for bytes in [png(&image).unwrap(), preview_png(&image).unwrap()] {
            let (result, preserved) = validated_export_png(bytes.clone()).unwrap();
            assert_eq!(preserved, bytes);
            assert_eq!(result.to_rgba8(), image.to_rgba8());
            assert_eq!(result.dimensions(), (7, 5));
        }
        assert!(decode(b"not an image").is_err());
        let mut jpeg = Vec::new();
        let rgb = image.to_rgb8();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .write_image(rgb.as_raw(), 7, 5, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert!(validated_export_png(jpeg).is_err());
    }
}
