use tauri::Manager;

fn dispatch(app: &tauri::AppHandle, action: &str) {
    let result = match action {
        "open" => crate::show_recorder(app.clone()),
        "folder" => crate::open_folder(app.clone(), app.state::<crate::Shared>()),
        "quit" => crate::request_exit(app),
        _ => Ok(()),
    };
    if let Err(error) = result {
        let _ = crate::show_recorder(app.clone());
        eprintln!("Tinrec tray: {error}");
    }
}

#[cfg(target_os = "linux")]
struct LinuxTray {
    app: tauri::AppHandle,
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}
#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "com.tinelli.tinrec".into()
    }
    fn title(&self) -> String {
        "Tinrec".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![ksni::Icon {
            width: self.width,
            height: self.height,
            data: self.pixels.clone(),
        }]
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Tinrec".into(),
            description: "Gravador ativo em segundo plano".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        dispatch(&self.app, "open");
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, StandardItem};
        vec![
            StandardItem {
                label: "Abrir Tinrec".into(),
                activate: Box::new(|tray: &mut Self| dispatch(&tray.app, "open")),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Abrir pasta de vídeos".into(),
                activate: Box::new(|tray: &mut Self| dispatch(&tray.app, "folder")),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Sair do Tinrec".into(),
                activate: Box::new(|tray: &mut Self| dispatch(&tray.app, "quit")),
                ..Default::default()
            }
            .into(),
        ]
    }
}
#[cfg(target_os = "linux")]
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use ksni::blocking::TrayMethods;
    let icon = app
        .default_window_icon()
        .ok_or("Ícone do Tinrec indisponível")?;
    // SNI uses network-order ARGB, whereas the bundled icon is RGBA.
    let pixels = icon
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    let tray = LinuxTray {
        app: app.handle().clone(),
        pixels,
        width: icon.width() as i32,
        height: icon.height() as i32,
    };
    // Keep waiting if the desktop shell's watcher starts after the app.
    app.manage(tray.assume_sni_available(true).spawn()?);
    Ok(())
}
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let open = MenuItem::with_id(app, "open", "Abrir Tinrec", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Abrir pasta de vídeos", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair do Tinrec", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &folder, &separator, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("Ícone do Tinrec indisponível")?;
    TrayIconBuilder::with_id("tinrec")
        .icon(icon)
        .tooltip("Tinrec")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| dispatch(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                dispatch(tray.app_handle(), "open");
            }
        })
        .build(app)?;
    Ok(())
}
