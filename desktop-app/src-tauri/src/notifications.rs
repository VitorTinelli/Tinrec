//! Upload completion notifications never open a browser without a click.
use crate::Reply;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub fn print_uploaded(app: &tauri::AppHandle, url: &str) -> Reply<()> {
    if !url.starts_with("https://tinrec.vercel.app/v/") {
        return Err("Link do print inválido.".into());
    }
    let copied = app.clipboard().write_text(url).is_ok();
    let body = if copied {
        "Link copiado para a área de transferência. Clique para abrir no site."
    } else {
        "Não foi possível copiar o link. Clique para abrir o print no site."
    };
    show(url, body)
}
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn toast_xml(url: &str, body: &str) -> String {
    format!(
        "<toast activationType=\"protocol\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>Print enviado</text><text>{}</text></binding></visual></toast>",
        escape_xml(url),
        escape_xml(body)
    )
}
#[cfg(target_os = "windows")]
fn show(url: &str, body: &str) -> Reply<()> {
    use windows::{
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };
    let notify = || -> Result<(), Box<dyn std::error::Error>> {
        let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(r"Software\Classes\AppUserModelId\com.tinelli.tinrec")?;
        key.set_value("DisplayName", &"Tinrec")?;
        key.set_value("ShowInSettings", &1u32)?;
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    RoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(toast_xml(url, body)))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("com.tinelli.tinrec"))?
            .Show(&toast)?;
        Ok(())
    };
    notify().map_err(|e| format!("Print enviado, mas não foi possível mostrar a notificação: {e}"))
}
#[cfg(not(target_os = "windows"))]
fn show(_url: &str, _body: &str) -> Reply<()> {
    Err("Notificação de envio disponível no Windows.".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn toast_opens_only_on_protocol_activation_and_escapes_xml() {
        let xml = super::toast_xml("https://tinrec.vercel.app/v/one?a=1&b=2", "Copiado <ok>");
        assert!(xml.contains("activationType=\"protocol\""));
        assert!(xml.contains("a=1&amp;b=2"));
        assert!(xml.contains("Copiado &lt;ok&gt;"));
    }
}
