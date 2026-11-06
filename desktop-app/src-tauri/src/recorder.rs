//! Recording-session orchestration: preparation, control, progress and finalization.
use crate::*;

#[tauri::command]
pub fn get_snapshot(state: State<'_, Shared>) -> Reply<Snapshot> {
    snapshot_value(&state)
}
#[tauri::command]
pub fn select_area(
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
pub fn get_folder(state: State<'_, Shared>) -> Reply<String> {
    Ok(lock(&state.folder)?.to_string_lossy().into())
}
#[tauri::command]
pub async fn choose_folder(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Reply<Option<String>> {
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
pub async fn start_recording(
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
    // Monitor APIs may dispatch to the UI thread, whose IPC handlers also read
    // session state. Resolve geometry before acquiring the session mutex.
    let (selected_monitor, mut selected_region) = if cfg!(target_os = "windows") {
        if request.capture_mode == "window" {
            let target = windows::window_target(&app, &request.window)?;
            (Some(target.monitor), Some(target.region))
        } else {
            (
                Some(windows::capture_monitor(&app, &request.display)?),
                None,
            )
        }
    } else {
        (
            app.get_webview_window("main")
                .and_then(|w| w.current_monitor().ok().flatten()),
            None,
        )
    };
    if let Some(crop) = &request.crop {
        let monitor = selected_monitor.as_ref().ok_or("Monitor indisponível")?;
        if request.capture_mode != "area"
            || crop.display != request.display
            || (crop.source_width, crop.source_height)
                != (monitor.size().width, monitor.size().height)
            || !crop.region.valid(crop.source_width, crop.source_height)
        {
            return Err("O monitor mudou. Selecione a área novamente.".into());
        }
        selected_region = Some((
            f64::from(crop.region.x) / f64::from(crop.source_width),
            f64::from(crop.region.y) / f64::from(crop.source_height),
            f64::from(crop.region.width) / f64::from(crop.source_width),
            f64::from(crop.region.height) / f64::from(crop.source_height),
        ));
    }
    let mut session = lock(&state.session)?;
    if app.state::<area_picker::Picker>().active() {
        return Err("Conclua a seleção de área.".into());
    }
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
    let followed_window = (cfg!(target_os = "windows") && request.capture_mode == "window")
        .then(|| request.window.clone());
    let webcam = (!request.camera.is_empty()).then(|| request.camera.clone());
    let webcam_available = webcam.is_some();
    let webcam_position = *lock(&state.webcam_position)?;
    *lock(&state.capture_monitor)? = selected_monitor;
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
        crop: request.crop.as_ref().map(|c| c.region.crop()),
        webcam,
        webcam_disabled: !request.webcam_enabled,
        webcam_visible_in_capture: cfg!(target_os = "linux") && request.capture_mode != "window",
        webcam_x: webcam_position.0,
        webcam_y: webcam_position.1,
        backend: if cfg!(target_os = "windows") {
            "wgc"
        } else {
            "auto"
        }
        .into(),
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
    let open_page = queued.is_ok();
    if !request.keep_local {
        queued?;
        state.cloud.remove_local_after_upload(&id)?;
    }
    let (sender, receiver) = mpsc::channel();
    session.id = id.clone();
    session.capture_mode = request.capture_mode.clone();
    session.window_source = followed_window.clone().unwrap_or_default();
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
    *lock(&state.webcam_region)? = selected_region;
    drop(session);
    position_webcam_window(&app);
    emit(&app, &initial);
    let state = state.inner().clone();
    if let Some(source) = followed_window {
        windows::follow_capture_window(app.clone(), state.clone(), id.clone(), source);
    }
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
                    initial_selection: None,
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
                        let monitor = if cfg!(target_os = "windows") {
                            state_monitor(&app)
                                .ok_or("O monitor selecionado não está disponível.")?
                        } else {
                            named_monitor
                            .or_else(|| (monitors.len() == 1).then(|| monitors[0].clone()))
                            .or_else(|| primary.filter(|monitor| source_matches_monitor(monitor.size().width, monitor.size().height, monitor.scale_factor(), preview.source_width, preview.source_height)))
                            .or_else(|| monitors.into_iter().find(|monitor| source_matches_monitor(monitor.size().width, monitor.size().height, monitor.scale_factor(), preview.source_width, preview.source_height)))
                            .ok_or("Não foi possível localizar o monitor da captura para selecionar a área.")?
                        };
                        window
                            .set_position(*monitor.position())
                            .map_err(|e| e.to_string())?;
                        window
                            .set_size(*monitor.size())
                            .map_err(|e| e.to_string())?;
                        window
                            .set_position(*monitor.position())
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
                    initial_selection: None,
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
            if snapshot.phase == "failed"
                && let Some(error) = &snapshot.error
            {
                diagnostics::notify(&app, error);
            }
            if snapshot.phase != "completed" && !close {
                let _ = show_recorder(app.clone());
            }
            emit(&app, &snapshot);
            if close {
                app.exit(0);
            } else if snapshot.phase == "completed" && open_page && !smoke::self_test_mode() {
                pages::open(app.clone(), state.clone(), pages::Page::Video(id.clone()));
            }
        }
    });
    Ok(initial)
}

#[tauri::command]
pub fn control_recording(
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
