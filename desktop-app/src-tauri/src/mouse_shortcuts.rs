//! Windows mouse shortcuts. The hook only queues actions; clicks always pass through.
use super::*;
use std::{cell::RefCell, sync::mpsc, thread};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::*,
    },
};
struct Context {
    app: tauri::AppHandle,
    bindings: Vec<(Modifiers, String, String)>,
}
thread_local! { static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) }; }
pub struct Hook {
    id: u32,
    thread: Option<thread::JoinHandle<()>>,
}
impl Hook {
    pub fn start(
        app: tauri::AppHandle,
        bindings: &BTreeMap<String, String>,
    ) -> Reply<Option<Self>> {
        let bindings = bindings
            .iter()
            .filter_map(|(action, value)| match mouse_binding(value) {
                Ok(Some((mods, button))) => Some((mods, button, action.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        if bindings.is_empty() {
            return Ok(None);
        }
        let (ready, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("tinrec-mouse-shortcuts".into())
            .spawn(move || {
                let mut msg = MSG::default();
                unsafe {
                    PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                }
                CONTEXT.with(|context| *context.borrow_mut() = Some(Context { app, bindings }));
                let hook = unsafe {
                    SetWindowsHookExW(
                        WH_MOUSE_LL,
                        Some(callback),
                        GetModuleHandleW(std::ptr::null()),
                        0,
                    )
                };
                if hook.is_null() {
                    let _ = ready.send(Err(format!(
                        "Não foi possível ativar os atalhos do mouse: {}",
                        std::io::Error::last_os_error()
                    )));
                    return;
                }
                let id = unsafe { GetCurrentThreadId() };
                if ready.send(Ok(id)).is_ok() {
                    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
                        unsafe {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }
                unsafe {
                    UnhookWindowsHookEx(hook);
                }
                CONTEXT.with(|context| context.borrow_mut().take());
            })
            .map_err(|e| e.to_string())?;
        match receiver.recv() {
            Ok(Ok(id)) => Ok(Some(Self {
                id,
                thread: Some(thread),
            })),
            result => {
                let _ = thread.join();
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => "Não foi possível iniciar os atalhos do mouse".into(),
                })
            }
        }
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.id, WM_QUIT, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
unsafe extern "system" fn callback(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0 {
        let button = match message as u32 {
            WM_LBUTTONDOWN => Some("MouseLeft"),
            WM_RBUTTONDOWN => Some("MouseRight"),
            WM_MBUTTONDOWN => Some("MouseMiddle"),
            WM_XBUTTONDOWN => {
                let event = unsafe { &*(data as *const MSLLHOOKSTRUCT) };
                match event.mouseData >> 16 {
                    1 => Some("MouseX1"),
                    2 => Some("MouseX2"),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(button) = button {
            let mut mods = Modifiers::empty();
            for (key, modifier) in [
                (VK_CONTROL, Modifiers::CONTROL),
                (VK_MENU, Modifiers::ALT),
                (VK_SHIFT, Modifiers::SHIFT),
                (VK_LWIN, Modifiers::SUPER),
                (VK_RWIN, Modifiers::SUPER),
            ] {
                if unsafe { GetAsyncKeyState(i32::from(key)) } < 0 {
                    mods |= modifier;
                }
            }
            CONTEXT.with(|context| {
                if let Some(context) = context.borrow().as_ref()
                    && let Some((_, _, action)) = context
                        .bindings
                        .iter()
                        .find(|(m, b, _)| *m == mods && b == button)
                {
                    let app = context.app.clone();
                    let action = action.clone();
                    let _ = context
                        .app
                        .run_on_main_thread(move || dispatch(&app, &action));
                }
            });
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) }
}
