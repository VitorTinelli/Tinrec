//! Page opening survives overlay closure, is bounded, and deduplicates waiters.
use crate::*;
use std::{
    collections::HashSet,
    sync::OnceLock,
    time::{Duration, Instant},
};
static OPENING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub enum Page {
    Video(String),
}

pub fn open(app: tauri::AppHandle, state: Shared, page: Page) {
    let key = match &page {
        Page::Video(id) => format!("video:{id}"),
    };
    let registry = OPENING.get_or_init(Default::default);
    if !registry.lock().is_ok_and(|mut set| set.insert(key.clone())) {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        while !state.exiting.load(Ordering::Acquire) {
            let id = match &page {
                Page::Video(id) => id,
            };
            let url = state.cloud.page_url(id);
            match url {
                Ok(Some(url)) => {
                    if app.opener().open_url(url, None::<&str>).is_err() {
                        crate::diagnostics::notify(
                            &app,
                            "Arquivo salvo, mas não foi possível abrir o navegador. Use Abrir no site na biblioteca.",
                        );
                    }
                    break;
                }
                Err(error) => {
                    crate::diagnostics::notify(
                        &app,
                        &format!(
                            "A abertura automática foi cancelada: {error}. Consulte a biblioteca para tentar novamente."
                        ),
                    );
                    break;
                }
                Ok(None) if Instant::now() >= deadline => {
                    crate::diagnostics::notify(
                        &app,
                        "O envio continua na fila. Quando o link estiver disponível, use Abrir no site na biblioteca.",
                    );
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(500)),
            }
        }
        if let Ok(mut set) = registry.lock() {
            set.remove(&key);
        }
    });
}
