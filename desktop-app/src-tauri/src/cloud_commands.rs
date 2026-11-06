//! IPC boundary for account linking and upload actions.
use crate::*;

#[tauri::command]
pub fn cloud_state(state: State<'_, Shared>) -> Reply<cloud::View> {
    state.cloud.view()
}
#[tauri::command]
pub async fn cloud_connect(app: tauri::AppHandle, state: State<'_, Shared>) -> Reply<()> {
    let cloud = state.cloud.clone();
    let url = tauri::async_runtime::spawn_blocking(move || cloud.connect())
        .await
        .map_err(|e| e.to_string())??;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn cloud_disconnect(state: State<'_, Shared>) -> Reply<()> {
    let cloud = state.cloud.clone();
    tauri::async_runtime::spawn_blocking(move || cloud.disconnect())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn cloud_retry(state: State<'_, Shared>, id: String) -> Reply<()> {
    state.cloud.retry(&id)
}
#[tauri::command]
pub fn cloud_enqueue(state: State<'_, Shared>, file_name: String) -> Reply<()> {
    let path = recording_path(&state, &file_name)?;
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(tinrec_capture::encoder::sidecar(&path, ".capture.json")).map_err(|_| {
            "Esta gravação não tem relatório de finalização. Confira o arquivo antes de enviar."
                .to_string()
        })?,
    )
    .map_err(|e| e.to_string())?;
    if report["status"] != "completed" {
        return Err("Somente gravações concluídas podem ser enviadas.".into());
    }
    let duration = report["active_seconds"]
        .as_f64()
        .ok_or("Duração não disponível")?;
    let title = file_name
        .rsplit_once(" - ")
        .map_or(file_name.trim_end_matches(".mp4"), |(title, _)| title);
    state
        .cloud
        .enqueue(
            uuid::Uuid::new_v4().to_string(),
            path,
            title.to_owned(),
            1080,
            duration,
            false,
        )
        .map(|_| ())
}
#[tauri::command]
pub fn cloud_open_recording(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    file_name: String,
) -> Reply<()> {
    let path = recording_path(&state, &file_name)?;
    let url = state.cloud.recording_url(&path)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn cloud_open(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    id: Option<String>,
) -> Reply<()> {
    let url = match id {
        None => "https://tinrec.vercel.app".to_owned(),
        Some(id) => state
            .cloud
            .view()?
            .jobs
            .into_iter()
            .find(|j| j.id == id)
            .and_then(|j| j.url)
            .ok_or("O link ainda não está disponível")?,
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cloud_copy(app: tauri::AppHandle, state: State<'_, Shared>, id: String) -> Reply<()> {
    let url = state
        .cloud
        .view()?
        .jobs
        .into_iter()
        .find(|j| j.id == id)
        .and_then(|j| j.url)
        .ok_or("O link ainda não está disponível")?;
    app.clipboard().write_text(url).map_err(|_| {
        "Não foi possível copiar o link. Copie o endereço exibido no aplicativo.".into()
    })
}
