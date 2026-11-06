//! One lightweight selector per monitor. Coordinates are local physical pixels.
use crate::*;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Region {
    pub fn valid(self, width: u32, height: u32) -> bool {
        self.x >= 0
            && self.y >= 0
            && self.width >= 2
            && self.height >= 2
            && self.x as u64 + u64::from(self.width) <= u64::from(width)
            && self.y as u64 + u64::from(self.height) <= u64::from(height)
    }
    pub fn crop(self) -> String {
        format!("{},{},{},{}", self.x, self.y, self.width, self.height)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    #[serde(skip)]
    pub editor_window: Option<String>,
    #[serde(skip)]
    pub origin: Option<(i32, i32)>,
    pub display: String,
    pub source_width: u32,
    pub source_height: u32,
    #[serde(flatten)]
    pub region: Region,
}
struct Target {
    origin: (i32, i32),
    label: String,
    display: String,
    preview: Preview,
}
struct Pending {
    id: String,
    targets: Vec<Target>,
    sender: Option<mpsc::Sender<Option<Selection>>>,
    ready: bool,
}
#[derive(Default)]
pub struct Picker {
    windows: tokio::sync::Mutex<()>,
    pending: Mutex<Option<Pending>>,
}
impl Picker {
    pub fn active(&self) -> bool {
        self.pending.lock().map_or(true, |p| p.is_some())
    }
}
pub fn is_window(label: &str) -> bool {
    label.starts_with("monitor-area-")
}

type Placements = Vec<(String, tauri::Monitor)>;
fn targets(app: &tauri::AppHandle, id: &str) -> Reply<(Vec<Target>, Placements)> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut placements = Vec::new();
    for (index, monitor) in monitors.into_iter().enumerate() {
        let name = monitor.name().ok_or("Monitor sem identificação")?;
        let display = format!("display:{name}");
        let (width, height) = (monitor.size().width, monitor.size().height);
        let origin = (monitor.position().x, monitor.position().y);
        let label = format!("monitor-area-{index}");
        placements.push((label.clone(), monitor));
        out.push(Target {
            label: label.clone(),
            origin,
            display,
            preview: Preview {
                session_id: format!("{id}:{label}"),
                sequence: 1,
                image: String::new(),
                width,
                height,
                source_width: width,
                source_height: height,
                initial_selection: None,
            },
        });
    }
    if out.is_empty() {
        return Err("Nenhum monitor disponível".into());
    }
    Ok((out, placements))
}

pub async fn pick(
    app: tauri::AppHandle,
    state: Shared,
    from_print: bool,
) -> Reply<Option<Selection>> {
    if !cfg!(target_os = "windows") {
        return Err("Seletor de monitores disponível no Windows".into());
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    // Native monitor calls must happen outside session locks.
    let (targets, placements) = targets(&app, &id)?;
    let labels: Vec<_> = targets.iter().map(|t| t.label.clone()).collect();
    let (sender, receiver) = mpsc::channel();
    {
        let session = lock(&state.session)?;
        if session.snapshot.busy()
            || (!from_print
                && app
                    .state::<screenshots::Prints>()
                    .busy
                    .load(Ordering::Acquire))
        {
            return Err("Finalize a captura atual antes de escolher outra área".into());
        }
        let picker = app.state::<Picker>();
        let mut pending = lock(&picker.pending)?;
        if pending.is_some() {
            return Err("Já existe uma seleção de área aberta".into());
        }
        *pending = Some(Pending {
            id: id.clone(),
            targets,
            sender: Some(sender),
            ready: false,
        });
    }
    let main = app.get_webview_window("main");
    let restore = main
        .as_ref()
        .is_some_and(|w| w.is_visible().unwrap_or(false));
    if restore && let Some(main) = &main {
        let _ = main.hide();
    }
    let mut result = async {
        ensure_windows(&app, placements).await?;
        let previews = {
            let picker = app.state::<Picker>();
            let pending = lock(&picker.pending)?;
            pending
                .as_ref()
                .map(|p| {
                    p.targets
                        .iter()
                        .map(|t| (t.label.clone(), t.preview.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        for (label, preview) in previews {
            app.emit_to(&label, "area-pick", preview)
                .map_err(|e| e.to_string())?;
        }
        for label in &labels {
            if let Some(w) = app.get_webview_window(label) {
                w.show().map_err(|e| e.to_string())?;
            }
        }
        if let Some(w) = labels.first().and_then(|l| app.get_webview_window(l)) {
            let _ = w.set_focus();
        }
        if let Some(pending) = lock(&app.state::<Picker>().pending)?.as_mut() {
            pending.ready = true;
        }
        tauri::async_runtime::spawn_blocking(move || {
            receiver.recv_timeout(Duration::from_secs(1800))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "Tempo de seleção esgotado".to_string())
    }
    .await;
    let retained = if from_print {
        result
            .as_ref()
            .ok()
            .and_then(|selection| selection.as_ref())
            .and_then(|selection| selection.editor_window.clone())
            .filter(|label| {
                app.get_webview_window(label)
                    .is_some_and(|w| capture_border::excluded(&w))
            })
    } else {
        None
    };
    if let Ok(Some(selection)) = &mut result {
        selection.editor_window = retained.clone();
    }
    for label in labels {
        if retained.as_ref() == Some(&label) {
            continue;
        }
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.hide();
            let _ = w.is_visible();
            let _ = app.emit_to(w.label(), "area-reset", ());
        }
    }
    {
        let _session = lock(&state.session)?;
        let picker = app.state::<Picker>();
        let mut pending = lock(&picker.pending)?;
        if pending.as_ref().is_some_and(|p| p.id == id) {
            pending.take();
        }
    }
    if !matches!(&result, Ok(Some(_)))
        && restore
        && let Some(main) = main
    {
        let _ = main.show();
        let _ = main.set_focus();
    }
    result
}
#[tauri::command]
pub async fn choose_area(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Reply<Option<Selection>> {
    pick(app, state.inner().clone(), false).await
}
#[tauri::command]
pub fn get_area_pick(
    window: tauri::WebviewWindow,
    picker: State<'_, Picker>,
) -> Reply<Option<Preview>> {
    Ok(lock(&picker.pending)?.as_ref().and_then(|p| {
        p.targets
            .iter()
            .find(|t| t.label == window.label())
            .map(|t| t.preview.clone())
    }))
}
fn confirm(picker: &Picker, label: &str, session_id: &str, region: Region) -> Reply<()> {
    let mut pending = lock(&picker.pending)?;
    let p = pending.as_mut().ok_or("Seleção encerrada")?;
    let target = p
        .targets
        .iter()
        .find(|t| t.label == label && t.preview.session_id == session_id)
        .ok_or("Seleção inválida")?;
    if !region.valid(target.preview.source_width, target.preview.source_height) {
        return Err("Área fora do monitor selecionado".into());
    }
    let selection = Selection {
        editor_window: Some(target.label.clone()),
        origin: Some(target.origin),
        display: target.display.clone(),
        source_width: target.preview.source_width,
        source_height: target.preview.source_height,
        region,
    };
    p.sender
        .take()
        .ok_or("Seleção já concluída")?
        .send(Some(selection))
        .map_err(|_| "Seleção encerrada".into())
}
#[tauri::command]
pub fn finish_area_pick(
    window: tauri::WebviewWindow,
    picker: State<'_, Picker>,
    session_id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Reply<()> {
    confirm(
        &picker,
        window.label(),
        &session_id,
        Region {
            x,
            y,
            width,
            height,
        },
    )
}
pub fn cancel(app: &tauri::AppHandle, label: &str) {
    let picker = app.state::<Picker>();
    if let Ok(mut pending) = picker.pending.lock()
        && let Some(p) = pending.as_mut()
        && p.targets.iter().any(|t| t.label == label)
        && let Some(sender) = p.sender.take()
    {
        let _ = sender.send(None);
    }
}
#[tauri::command]
pub fn cancel_area_pick(app: tauri::AppHandle, window: tauri::WebviewWindow) {
    cancel(&app, window.label());
}

#[tauri::command]
pub fn self_test_area_pick(
    app: tauri::AppHandle,
    index: Option<usize>,
) -> Reply<serde_json::Value> {
    if !smoke::self_test_mode() {
        return Err("Comando exclusivo do autoteste".into());
    }
    let picker = app.state::<Picker>();
    let pending = lock(&picker.pending)?;
    let Some(p) = pending.as_ref().filter(|p| p.ready) else {
        return Ok(serde_json::Value::Null);
    };
    if let Some(index) = index {
        let t = p.targets.get(index).ok_or("Monitor de teste ausente")?;
        let (label, id) = (t.label.clone(), t.preview.session_id.clone());
        let region = Region {
            x: 10,
            y: 10,
            width: 320.min(t.preview.source_width.saturating_sub(10)),
            height: 240.min(t.preview.source_height.saturating_sub(10)),
        };
        drop(pending);
        confirm(&picker, &label, &id, region)?;
        return Ok(serde_json::json!(true));
    }
    Ok(
        serde_json::json!({"count":p.targets.len(), "labels":p.targets.iter().map(|t| &t.label).collect::<Vec<_>>()}),
    )
}

async fn ensure_windows(app: &tauri::AppHandle, placements: Placements) -> Reply<()> {
    let picker = app.state::<Picker>();
    let _windows = picker.windows.lock().await;
    for (label, monitor) in placements {
        let window = if let Some(window) = app.get_webview_window(&label) {
            window
        } else {
            tauri::WebviewWindowBuilder::new(
                app,
                label.clone(),
                tauri::WebviewUrl::App("index.html?window=monitor-area".into()),
            )
            .title("Tinrec · selecione uma área")
            .visible(false)
            .focused(false)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .content_protected(true)
            .transparent(true)
            .shadow(false)
            .build()
            .map_err(|e| e.to_string())?
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
    }

    Ok(())
}
pub async fn warm(app: &tauri::AppHandle) -> Reply<()> {
    let (_, placements) = targets(app, "warm")?;
    ensure_windows(app, placements).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(label: &str, width: u32, height: u32) -> Target {
        Target {
            origin: (0, 0),
            label: label.into(),
            display: format!("display:{label}"),
            preview: Preview {
                session_id: format!("session:{label}"),
                sequence: 1,
                image: String::new(),
                width,
                height,
                source_width: width,
                source_height: height,
                initial_selection: None,
            },
        }
    }
    #[test]
    fn selection_keeps_monitor_identity_and_rejects_stale_or_out_of_bounds_requests() {
        let (sender, receiver) = mpsc::channel();
        let picker = Picker {
            windows: Default::default(),
            pending: Mutex::new(Some(Pending {
                id: "pick".into(),
                targets: vec![target("left", 1920, 1080), target("right", 3840, 2160)],
                sender: Some(sender),
                ready: true,
            })),
        };
        let region = Region {
            x: 2500,
            y: 1200,
            width: 1000,
            height: 500,
        };
        assert!(confirm(&picker, "left", "session:left", region).is_err());
        assert!(confirm(&picker, "right", "session:left", region).is_err());
        assert!(
            confirm(
                &picker,
                "right",
                "session:right",
                Region { x: -1, ..region }
            )
            .is_err()
        );
        assert!(
            confirm(
                &picker,
                "right",
                "session:right",
                Region {
                    width: u32::MAX,
                    ..region
                }
            )
            .is_err()
        );
        confirm(&picker, "right", "session:right", region).unwrap();
        let selected = receiver.recv().unwrap().unwrap();
        assert_eq!(selected.display, "display:right");
        assert_eq!(
            (selected.source_width, selected.source_height),
            (3840, 2160)
        );
        assert_eq!(selected.region, region);
        assert!(
            confirm(
                &picker,
                "left",
                "session:left",
                Region {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100
                }
            )
            .is_err()
        );
    }
}
