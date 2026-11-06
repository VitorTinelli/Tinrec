use crate::{Reply, Shared, control_recording, lock, show_controls, show_recorder, snapshot_value};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const ACTIONS: &[(&str, &str)] = &[
    ("screenshot", "Print de área"),
    ("screenshot-screen", "Print de tela"),
    ("screenshot-window", "Print de janela"),
    ("screen", "Gravar tela"),
    ("window", "Gravar janela"),
    ("area", "Gravar área"),
    ("show", "Abrir gravador"),
    ("start", "Iniciar gravação"),
    ("stop", "Parar e salvar"),
    ("pause", "Pausar ou retomar"),
    ("microphone", "Silenciar ou ativar microfone"),
    ("system", "Silenciar ou ativar áudio do computador"),
    ("controls", "Mostrar mini menu"),
    ("library", "Abrir biblioteca"),
];
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub active: BTreeMap<String, String>,
    pub error: Option<String>,
    #[serde(default)]
    pub wayland: bool,
}
#[cfg(target_os = "windows")]
#[path = "mouse_shortcuts.rs"]
mod mouse;
pub struct Shortcuts {
    #[cfg(target_os = "windows")]
    mouse: Mutex<Option<mouse::Hook>>,
    view: Mutex<View>,
    operation: tokio::sync::Mutex<()>,
    suspended: AtomicBool,
    file: PathBuf,
    #[cfg(target_os = "linux")]
    close_portal: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}
impl Shortcuts {
    pub fn new(file: PathBuf) -> Self {
        let mut view = View::default();
        match std::fs::read(&file) {
            Ok(bytes) => match serde_json::from_slice::<BTreeMap<String, String>>(&bytes) {
                Ok(bindings) => view.bindings = bindings,
                Err(_) => {
                    view.error = Some(
                        "Não foi possível ler os atalhos salvos. Configure-os novamente.".into(),
                    )
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => view.error = Some("Não foi possível ler os atalhos salvos.".into()),
        }
        view.wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
        Self {
            #[cfg(target_os = "windows")]
            mouse: Mutex::new(None),
            view: Mutex::new(view),
            operation: tokio::sync::Mutex::new(()),
            suspended: AtomicBool::new(false),
            file,
            #[cfg(target_os = "linux")]
            close_portal: Mutex::new(None),
        }
    }
    pub fn view(&self) -> Reply<View> {
        Ok(lock(&self.view)?.clone())
    }
    pub async fn apply(
        &self,
        app: &tauri::AppHandle,
        bindings: BTreeMap<String, String>,
    ) -> Reply<View> {
        let _operation = self.operation.lock().await;
        let bindings = validate(bindings).inspect_err(|error| {
            if let Ok(mut view) = self.view.lock() {
                view.error = Some(error.clone());
            }
        })?;
        let old = self.view()?;
        #[cfg(target_os = "linux")]
        let result = if old.wayland {
            self.portal(app, &bindings).await
        } else {
            register(app, &bindings).map(|()| bindings.clone())
        };
        #[cfg(not(target_os = "linux"))]
        let result = register(app, &bindings).map(|()| bindings.clone());
        let active = match result {
            Ok(active) => active,
            Err(mut error) => {
                if !old.wayland && register(app, &old.bindings).is_err() {
                    lock(&self.view)?.active.clear();
                    error.push_str(" Os atalhos anteriores também não puderam ser reativados. Escolha novas combinações.");
                }
                lock(&self.view)?.error = Some(error.clone());
                let _ = app.emit_to("main", "shortcuts-state", self.view()?);
                return Err(error);
            }
        };
        let temp = self.file.with_extension("tmp");
        let saved = std::fs::write(
            &temp,
            serde_json::to_vec(&bindings).map_err(|e| e.to_string())?,
        )
        .and_then(|()| std::fs::rename(&temp, &self.file));
        let error = saved.err().map(|_| "Atalhos ativos nesta sessão, mas não foi possível salvá-los para a próxima abertura.".to_string());
        let next = View {
            bindings,
            active,
            error,
            wayland: old.wayland,
        };
        *lock(&self.view)? = next.clone();
        let _ = app.emit_to("main", "shortcuts-state", &next);
        Ok(next)
    }
    #[cfg(target_os = "linux")]
    async fn portal(
        &self,
        app: &tauri::AppHandle,
        bindings: &BTreeMap<String, String>,
    ) -> Reply<BTreeMap<String, String>> {
        use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
        use futures_util::StreamExt;
        if bindings.is_empty() {
            if let Some(close) = lock(&self.close_portal)?.take() {
                let _ = close.send(());
            }
            return Ok(BTreeMap::new());
        }
        let portal = GlobalShortcuts::new().await.map_err(|_| "O sistema não disponibilizou o portal de atalhos globais. Use os controles do Tinrec ou habilite o portal GlobalShortcuts do seu ambiente Linux.".to_string())?;
        let session = portal
            .create_session(Default::default())
            .await
            .map_err(|e| format!("Não foi possível criar a sessão de atalhos: {e}"))?;
        let entries: Vec<_> = bindings
            .iter()
            .map(|(id, value)| {
                let description = ACTIONS.iter().find(|(action, _)| action == id).unwrap().1;
                NewShortcut::new(id, format!("Tinrec · {description}"))
                    .preferred_trigger(portal_trigger(value).as_deref())
            })
            .collect();
        let request = portal
            .bind_shortcuts(&session, &entries, None, Default::default())
            .await;
        let active = match request.and_then(|request| request.response()) {
            Ok(response) => response
                .shortcuts()
                .iter()
                .map(|shortcut| {
                    (
                        shortcut.id().to_string(),
                        shortcut.trigger_description().to_string(),
                    )
                })
                .collect(),
            Err(error) => {
                let _ = session.close().await;
                return Err(format!(
                    "Os atalhos não foram autorizados pelo sistema: {error}"
                ));
            }
        };
        let mut activated = portal
            .receive_activated()
            .await
            .map_err(|e| e.to_string())?;
        let mut changed = portal
            .receive_shortcuts_changed()
            .await
            .map_err(|e| e.to_string())?;
        let session_path = serde_json::to_value(&session)
            .map_err(|e| e.to_string())?
            .as_str()
            .unwrap_or_default()
            .to_string();
        let (close, mut cancelled) = tokio::sync::oneshot::channel();
        if let Some(previous) = lock(&self.close_portal)?.replace(close) {
            let _ = previous.send(());
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut cancelled => break,
                    event = activated.next() => {
                        let Some(event) = event else { break };
                        if event.session_handle().as_str() == session_path { dispatch(&app, event.shortcut_id()); }
                    },
                    event = changed.next() => {
                        let Some(event) = event else { break };
                        if event.session_handle().as_str() == session_path {
                            let manager = app.state::<Shortcuts>();
                            if let Ok(mut view) = manager.view.lock() {
                                view.active = event.shortcuts().iter().map(|shortcut| (shortcut.id().to_string(), shortcut.trigger_description().to_string())).collect();
                                let _ = app.emit_to("main", "shortcuts-state", &*view);
                            }
                        }
                    }
                }
            }
            let _ = session.close().await;
        });
        Ok(active)
    }
}
fn validate(bindings: BTreeMap<String, String>) -> Reply<BTreeMap<String, String>> {
    let mut unique = HashSet::new();
    let mut normalized = BTreeMap::new();
    for (action, value) in bindings {
        if !ACTIONS.iter().any(|(id, _)| *id == action) {
            return Err("Ação de atalho inválida.".into());
        }
        if value.trim().is_empty() {
            continue;
        }
        if let Some((mods, button)) = mouse_binding(&value)? {
            if !cfg!(target_os = "windows") {
                return Err("Atalhos de mouse disponíveis no Windows.".into());
            }
            let value = format_mouse(mods, &button);
            if !unique.insert(value.clone()) {
                return Err("Cada ação precisa de um atalho diferente.".into());
            }
            normalized.insert(action, value);
            continue;
        }
        let shortcut: Shortcut = value
            .parse()
            .map_err(|_| format!("Atalho inválido: {value}. Use, por exemplo, Ctrl+Shift+R."))?;
        if !unique.insert(shortcut.to_string()) {
            return Err("Cada ação precisa de uma combinação diferente.".into());
        }
        normalized.insert(action, shortcut.to_string());
    }
    Ok(normalized)
}
fn register(app: &tauri::AppHandle, bindings: &BTreeMap<String, String>) -> Reply<()> {
    #[cfg(target_os = "windows")]
    {
        let old = lock(&app.state::<Shortcuts>().mouse)?.take();
        drop(old);
    }
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    for (action, binding) in bindings {
        if mouse_binding(binding)?.is_some() {
            continue;
        }
        let action = action.clone();
        app.global_shortcut().on_shortcut(binding.as_str(), move |app, _, event| {
            if event.state == ShortcutState::Pressed { dispatch(app, &action); }
        }).map_err(|_| format!("Não foi possível registrar {binding}. A combinação pode estar em uso por outro aplicativo."))?;
    }
    #[cfg(target_os = "windows")]
    {
        let hook = mouse::Hook::start(app.clone(), bindings)?;
        *lock(&app.state::<Shortcuts>().mouse)? = hook;
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn portal_trigger(binding: &str) -> Option<String> {
    let shortcut: Shortcut = binding.parse().ok()?;
    let mut keys = Vec::new();
    if shortcut.mods.contains(Modifiers::CONTROL) {
        keys.push("CTRL".to_string());
    }
    if shortcut.mods.contains(Modifiers::ALT) {
        keys.push("ALT".to_string());
    }
    if shortcut.mods.contains(Modifiers::SHIFT) {
        keys.push("SHIFT".to_string());
    }
    if shortcut.mods.contains(Modifiers::SUPER) {
        keys.push("LOGO".to_string());
    }
    let code = shortcut.key.to_string();
    let key = code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
        .unwrap_or(match code.as_str() {
            "Space" => "space",
            "Comma" => "comma",
            "Period" => "period",
            "Enter" => "Return",
            _ => &code,
        });
    let key = if code.starts_with("Key") {
        key.to_ascii_lowercase()
    } else {
        match key {
            "ArrowUp" => "Up".into(),
            "ArrowDown" => "Down".into(),
            "ArrowLeft" => "Left".into(),
            "ArrowRight" => "Right".into(),
            _ => key.to_string(),
        }
    };
    keys.push(key);
    Some(keys.join("+"))
}
fn dispatch(app: &tauri::AppHandle, action: &str) {
    if app.state::<Shortcuts>().suspended.load(Ordering::Acquire) {
        return;
    }
    #[cfg(target_os = "windows")]
    if let Ok(mut probe) = TEST_EVENTS.lock()
        && let Some(events) = probe.as_mut()
    {
        events.push(action.into());
        return;
    }
    let state = app.state::<Shared>();
    if state.exiting.load(Ordering::Acquire) {
        return;
    }
    let Ok(snapshot) = snapshot_value(&state) else {
        return;
    };
    match action {
        "show" | "library" => {
            let _ = show_recorder(app.clone());
            let _ = app.emit_to("main", "shortcut-action", action);
        }
        "screenshot" | "screenshot-screen" | "screenshot-window" if !snapshot.busy() => {
            let _ = app.emit_to("main", "shortcut-action", action);
        }
        "area" if !snapshot.busy() => {
            let _ = app.emit_to("main", "shortcut-action", action);
        }
        "start" | "screen" | "window" if !snapshot.busy() => {
            let _ = show_recorder(app.clone());
            let _ = app.emit_to("main", "shortcut-action", action);
        }
        "controls" => {
            let _ = show_controls(app.clone(), state);
        }
        "stop" => {
            let _ = control_recording(app.clone(), state, "stop".into());
        }
        "pause" => {
            let command = if snapshot.phase == "paused" {
                "resume"
            } else {
                "pause"
            };
            let _ = control_recording(app.clone(), state, command.into());
        }
        "microphone" => {
            let _ = control_recording(app.clone(), state, "mute-microphone".into());
        }
        "system" => {
            let _ = control_recording(app.clone(), state, "mute-system".into());
        }
        _ => {}
    }
}
#[tauri::command]
pub fn get_shortcuts(state: tauri::State<'_, Shortcuts>) -> Reply<View> {
    state.view()
}
#[tauri::command]
pub async fn shortcut_capture(
    app: tauri::AppHandle,
    state: tauri::State<'_, Shortcuts>,
    enabled: bool,
) -> Reply<()> {
    let _operation = state.operation.lock().await;
    let was_suspended = state.suspended.swap(enabled, Ordering::AcqRel);
    if enabled {
        #[cfg(target_os = "windows")]
        {
            let old = lock(&state.mouse)?.take();
            drop(old);
        }
        if !state.view()?.wayland {
            app.global_shortcut()
                .unregister_all()
                .map_err(|e| e.to_string())?;
        }
    } else if was_suspended && !state.view()?.wayland {
        let view = state.view()?;
        if let Err(error) = register(&app, &view.active) {
            let mut view = lock(&state.view)?;
            view.active.clear();
            view.error = Some(error.clone());
            let _ = app.emit_to("main", "shortcuts-state", &*view);
            return Err(error);
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn save_shortcuts(
    app: tauri::AppHandle,
    state: tauri::State<'_, Shortcuts>,
    bindings: BTreeMap<String, String>,
) -> Reply<View> {
    state.apply(&app, bindings).await
}

#[tauri::command]
pub fn local_shortcut(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    action: String,
) -> Reply<()> {
    match action.as_str() {
        "close" => window.close().map_err(|e| e.to_string()),
        "settings" => {
            show_recorder(app.clone())?;
            app.emit_to("main", "shortcut-action", "settings")
                .map_err(|e| e.to_string())
        }
        _ => Err("Atalho local inválido.".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_single_keys_mouse_buttons_actions_and_duplicates() {
        let bindings = BTreeMap::from([
            ("start".into(), "Ctrl+Shift+R".into()),
            ("stop".into(), "F5".into()),
            ("pause".into(), "Q".into()),
            ("controls".into(), "Shift+R".into()),
        ]);
        assert_eq!(validate(bindings).unwrap().len(), 4);
        #[cfg(target_os = "windows")]
        assert_eq!(
            validate(BTreeMap::from([
                ("show".into(), "control+mousex1".into()),
                ("stop".into(), "MouseLeft".into())
            ]))
            .unwrap()["show"],
            "Ctrl+MouseX1"
        );
        for bindings in [
            BTreeMap::from([("start".into(), "MouseX3".into())]),
            BTreeMap::from([("start".into(), "Q+MouseLeft".into())]),
            BTreeMap::from([("start".into(), "Ctrl+Control+MouseLeft".into())]),
            BTreeMap::from([("start".into(), "Q".into()), ("stop".into(), "KeyQ".into())]),
            BTreeMap::from([
                ("start".into(), "Ctrl+MouseX1".into()),
                ("stop".into(), "Control+mousex1".into()),
            ]),
            BTreeMap::from([("unknown".into(), "Ctrl+R".into())]),
            BTreeMap::from([
                ("start".into(), "Ctrl+R".into()),
                ("stop".into(), "Control+KeyR".into()),
            ]),
        ] {
            assert!(validate(bindings).is_err());
        }
        assert!(
            validate(BTreeMap::from([("start".into(), "".into())]))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn loads_saved_bindings_without_assigning_defaults() {
        let file =
            std::env::temp_dir().join(format!("tinrec-shortcuts-{}.json", uuid::Uuid::new_v4()));
        assert!(
            Shortcuts::new(file.clone())
                .view()
                .unwrap()
                .bindings
                .is_empty()
        );
        std::fs::write(&file, br#"{"start":"Ctrl+Shift+R"}"#).unwrap();
        let manager = Shortcuts::new(file.clone());
        assert_eq!(manager.view().unwrap().bindings["start"], "Ctrl+Shift+R");
        assert!(manager.view().unwrap().active.is_empty());
        std::fs::remove_file(file).unwrap();
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn converts_physical_keys_to_portal_shortcut_syntax() {
        assert_eq!(
            portal_trigger("Ctrl+Shift+KeyR").as_deref(),
            Some("CTRL+SHIFT+r")
        );
        assert_eq!(
            portal_trigger("Super+Alt+Digit1").as_deref(),
            Some("ALT+LOGO+1")
        );
        assert_eq!(portal_trigger("Ctrl+ArrowUp").as_deref(), Some("CTRL+Up"));
    }
}

fn mouse_binding(value: &str) -> Reply<Option<(Modifiers, String)>> {
    if !value.to_ascii_lowercase().contains("mouse") {
        return Ok(None);
    }
    let mut parts = value.split('+').map(str::trim).collect::<Vec<_>>();
    let button = match parts
        .pop()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mouseleft" => "MouseLeft",
        "mouseright" => "MouseRight",
        "mousemiddle" => "MouseMiddle",
        "mousex1" => "MouseX1",
        "mousex2" => "MouseX2",
        _ => return Err("Botão do mouse inválido.".into()),
    };
    let mut mods = Modifiers::empty();
    for part in parts {
        let modifier = match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Modifiers::CONTROL,
            "alt" | "option" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            "super" | "win" | "windows" | "meta" | "cmd" | "command" => Modifiers::SUPER,
            _ => return Err("Modificador de mouse inválido.".into()),
        };
        if mods.contains(modifier) {
            return Err("Modificador repetido no atalho.".into());
        }
        mods |= modifier;
    }
    Ok(Some((mods, button.into())))
}
fn format_mouse(mods: Modifiers, button: &str) -> String {
    let mut parts = Vec::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "Ctrl"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Super"),
    ] {
        if mods.contains(modifier) {
            parts.push(name);
        }
    }
    parts.push(button);
    parts.join("+")
}

#[cfg(target_os = "windows")]
static TEST_EVENTS: Mutex<Option<Vec<String>>> = Mutex::new(None);

#[tauri::command]
pub async fn self_test_shortcuts(
    app: tauri::AppHandle,
    state: tauri::State<'_, Shortcuts>,
) -> Reply<()> {
    if !crate::smoke::self_test_mode() {
        return Err("Comando exclusivo do autoteste".into());
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::{
            Foundation::POINT,
            UI::{
                Input::KeyboardAndMouse::*,
                WindowsAndMessaging::{GetCursorPos, SetCursorPos},
            },
        };
        fn key(code: u16, up: bool) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: code,
                        wScan: 0,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }
        fn mouse(flags: u32, data: u32) -> INPUT {
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: data,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }
        fn send(inputs: &[INPUT]) -> Reply<()> {
            if unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                )
            } != inputs.len() as u32
            {
                return Err("Falha ao enviar entrada para o autoteste".into());
            }
            Ok(())
        }
        let fixture = app
            .get_webview_window("capture-fixture")
            .ok_or("Janela de teste ausente")?;
        fixture.set_focus().map_err(|e| e.to_string())?;
        let p = fixture.outer_position().map_err(|e| e.to_string())?;
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return Err("Posição do cursor indisponível".into());
        }
        if unsafe { SetCursorPos(p.x + 100, p.y + 100) } == 0 {
            return Err("Não foi possível posicionar o cursor de teste".into());
        }
        let previous = state.view()?.bindings;
        *lock(&TEST_EVENTS)? = Some(Vec::new());
        let result: Reply<()> = async {
            state
                .apply(
                    &app,
                    BTreeMap::from([
                        ("show".into(), "F5".into()),
                        ("library".into(), "Q".into()),
                        ("controls".into(), "MouseX1".into()),
                        ("pause".into(), "Ctrl+MouseMiddle".into()),
                    ]),
                )
                .await?;
            for inputs in [
                vec![key(VK_F5, false), key(VK_F5, true)],
                vec![key(0x51, false), key(0x51, false), key(0x51, true)],
                vec![mouse(MOUSEEVENTF_XDOWN, 1), mouse(MOUSEEVENTF_XUP, 1)],
                vec![
                    key(VK_CONTROL, false),
                    mouse(MOUSEEVENTF_MIDDLEDOWN, 0),
                    mouse(MOUSEEVENTF_MIDDLEUP, 0),
                    key(VK_CONTROL, true),
                ],
            ] {
                send(&inputs)?;
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            shortcut_capture(app.clone(), app.state::<Shortcuts>(), true).await?;
            send(&[
                key(0x51, false),
                key(0x51, true),
                mouse(MOUSEEVENTF_XDOWN, 1),
                mouse(MOUSEEVENTF_XUP, 1),
            ])?;
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            if lock(&TEST_EVENTS)?.as_ref().unwrap().len() != 4 {
                return Err("Atalho disparou durante a edição".into());
            }
            shortcut_capture(app.clone(), app.state::<Shortcuts>(), false).await?;
            send(&[
                key(0x51, false),
                key(0x51, true),
                mouse(MOUSEEVENTF_XDOWN, 1),
                mouse(MOUSEEVENTF_XUP, 1),
            ])?;
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let expected = vec![
                "show", "library", "controls", "pause", "library", "controls",
            ];
            let events = lock(&TEST_EVENTS)?.clone().unwrap();
            if events != expected {
                return Err(format!("Atalhos não dispararam corretamente: {:?}", events));
            }
            Ok(())
        }
        .await;
        state.suspended.store(false, Ordering::Release);
        let restored = state.apply(&app, previous).await;
        let _ = send(&[
            key(VK_CONTROL, true),
            key(VK_F5, true),
            key(0x51, true),
            mouse(MOUSEEVENTF_XUP, 1),
            mouse(MOUSEEVENTF_MIDDLEUP, 0),
        ]);
        unsafe {
            SetCursorPos(cursor.x, cursor.y);
        }
        *lock(&TEST_EVENTS)? = None;
        result?;
        restored?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, state);
    }
    Ok(())
}
