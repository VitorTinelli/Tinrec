//! Local recording library, independent of capture-session control.
use crate::*;

pub(super) fn recording_path(state: &AppState, file_name: &str) -> Reply<PathBuf> {
    if !file_name.ends_with(".mp4")
        || file_name.ends_with(".partial.mp4")
        || Path::new(file_name).file_name().and_then(|s| s.to_str()) != Some(file_name)
    {
        return Err("Arquivo de gravação inválido.".into());
    }
    let folder = fs::canonicalize(&*lock(&state.folder)?).map_err(|e| e.to_string())?;
    let videos = folder.join("Videos");
    let path = fs::canonicalize(videos.join(file_name))
        .or_else(|_| fs::canonicalize(folder.join(file_name)))
        .map_err(|_| "A gravação não está mais nessa pasta.".to_string())?;
    if (path.parent() != Some(folder.as_path()) && path.parent() != Some(videos.as_path()))
        || !path.is_file()
    {
        return Err("O arquivo não pertence à pasta de gravações.".into());
    }
    Ok(path)
}
#[tauri::command]
pub fn open_recording(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    file_name: String,
) -> Reply<()> {
    let path = recording_path(&state, &file_name)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn recording_thumbnail(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    file_name: String,
) -> Reply<String> {
    let path = recording_path(&state, &file_name)?;
    let ffmpeg = tool_path(&app, "ffmpeg");
    tauri::async_runtime::spawn_blocking(move || extract_thumbnail(&ffmpeg, &path))
        .await
        .map_err(|e| e.to_string())?
}
pub(super) fn extract_thumbnail(ffmpeg: &Path, path: &Path) -> Reply<String> {
    let mut command = native_command(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=480:270:force_original_aspect_ratio=decrease",
            "-threads",
            "1",
            "-f",
            "image2pipe",
            "-c:v",
            "mjpeg",
            "-q:v",
            "5",
            "pipe:1",
        ]);
    let output = tinrec_capture::process::output(&mut command, std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err("Miniatura indisponível para este vídeo.".into());
    }
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(output.stdout)
    ))
}
#[tauri::command]
pub fn open_folder(app: tauri::AppHandle, state: State<'_, Shared>) -> Reply<()> {
    let path = lock(&state.folder)?.clone();
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn list_recordings(state: State<'_, Shared>) -> Reply<Vec<RecordingFile>> {
    let folder = lock(&state.folder)?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if !folder.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        let entries = fs::read_dir(&folder)
            .map_err(|e| e.to_string())?
            .chain(fs::read_dir(folder.join("Videos")).into_iter().flatten());
        for entry in entries {
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
