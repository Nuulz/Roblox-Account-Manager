use std::path::PathBuf;

use md5::{Digest, Md5};
use once_cell::sync::Lazy;

/// Carpeta del ejecutable. La app es portable: todos los datos viven junto al .exe.
pub static APP_DIR: Lazy<PathBuf> = Lazy::new(|| {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
});

pub fn data_path(name: &str) -> PathBuf {
    APP_DIR.join(name)
}

pub fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| APP_DIR.clone())
}

pub fn md5_hex(input: &str) -> String {
    hex::encode_upper(Md5::digest(input.as_bytes()))
}

pub fn set_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

pub fn get_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Escribe un archivo de forma atómica (tmp + rename) para no corromper datos si la app se cierra a mitad.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

pub fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(windows)]
pub fn shell_open(target: &str) {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(target),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(windows)]
pub fn message_box(title: &str, text: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), MB_OK | MB_ICONWARNING);
    }
}

#[cfg(windows)]
pub fn is_connected_to_internet() -> bool {
    use windows::Win32::Networking::WinInet::{InternetGetConnectedState, INTERNET_CONNECTION};
    let mut flags = INTERNET_CONNECTION(0);
    unsafe { InternetGetConnectedState(&mut flags, None).is_ok() }
}

/// Filtra el nombre de un juego igual que antes: sin etiquetas [..] ni símbolos raros.
pub fn filter_game_name(name: &str) -> String {
    static TAGS: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\s?\[[^\]]*\]\s?").unwrap());
    static BAD: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"[^a-zA-Z0-9% ._]").unwrap());
    let s = TAGS.replace_all(name, "");
    BAD.replace_all(&s, "").trim().to_string()
}
