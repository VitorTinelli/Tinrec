//! Policies that differ between the active Windows product and archived ports.
use anyhow::{Context, Result};
use std::path::Path;

pub fn capture_backend() -> pinray::BackendPreference {
    if cfg!(target_os = "windows") {
        // WGC includes the pointer; the pinned DXGI implementation does not.
        pinray::BackendPreference::WindowsWgc
    } else {
        pinray::BackendPreference::Auto
    }
}

pub fn publish_file(partial: &Path, output: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
        let from: Vec<u16> = partial.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = output.as_os_str().encode_wide().chain(Some(0)).collect();
        // No REPLACE_EXISTING: atomic publication also works on exFAT/FAT32.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("Não foi possível publicar o arquivo sem sobrescrever o destino");
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::hard_link(partial, output)
            .context("Não foi possível publicar o arquivo sem sobrescrever o destino")?;
        std::fs::remove_file(partial)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_never_overwrites_and_preserves_failed_source() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("partial");
        let target = dir.path().join("target");
        std::fs::write(&partial, b"new").unwrap();
        std::fs::write(&target, b"old").unwrap();
        assert!(publish_file(&partial, &target).is_err());
        assert_eq!(std::fs::read(&partial).unwrap(), b"new");
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        std::fs::remove_file(&target).unwrap();
        publish_file(&partial, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!partial.exists());
    }
}
