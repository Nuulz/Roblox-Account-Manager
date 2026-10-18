//! Procesos y ventanas de Roblox (Win32).

use std::sync::Mutex;

use once_cell::sync::Lazy;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows::core::{BOOL, HSTRING};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, MoveWindow,
    PostMessageW, GW_OWNER, SC_MINIMIZE, WM_CLOSE, WM_KEYDOWN, WM_KEYUP, WM_SYSCOMMAND,
};

pub const PLAYER_EXE: &str = "RobloxPlayerBeta.exe";

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RbxProcess {
    pub pid: u32,
    pub cmd: String,
    pub start_time: u64,
    pub memory_mb: u64,
    pub hwnd: Option<isize>,
    pub title: String,
    pub tracker_id: String,
}

static SYS: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(System::new()));

pub fn tracker_id_from_cmd(cmd: &str) -> String {
    static RE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"(?:-b |browsertrackerid[:=])(\d+)").unwrap());
    RE.captures(cmd).map(|c| c[1].to_string()).unwrap_or_default()
}

pub fn roblox_processes() -> Vec<RbxProcess> {
    let mut sys = SYS.lock().unwrap();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_memory(),
    );
    let mut out = Vec::new();
    for (pid, p) in sys.processes() {
        if !p.name().to_string_lossy().eq_ignore_ascii_case(PLAYER_EXE) {
            continue;
        }
        let cmd = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ");
        let pid = pid.as_u32();
        let hwnd = main_window(pid);
        out.push(RbxProcess {
            pid,
            tracker_id: tracker_id_from_cmd(&cmd),
            cmd,
            start_time: p.start_time(),
            memory_mb: p.memory() / 1024 / 1024,
            title: hwnd.map(window_title).unwrap_or_default(),
            hwnd,
        });
    }
    out
}

pub fn kill(pid: u32) -> bool {
    let sys = SYS.lock().unwrap();
    if let Some(p) = sys.process(Pid::from_u32(pid)) {
        return p.kill();
    }
    drop(sys);
    let mut sys = SYS.lock().unwrap();
    sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    sys.process(Pid::from_u32(pid)).map(|p| p.kill()).unwrap_or(false)
}

struct FindCtx {
    pid: u32,
    found: Option<isize>,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut FindCtx);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == ctx.pid && IsWindowVisible(hwnd).as_bool() {
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        if owner.0.is_null() {
            ctx.found = Some(hwnd.0 as isize);
            return BOOL(0);
        }
    }
    BOOL(1)
}

pub fn main_window(pid: u32) -> Option<isize> {
    let mut ctx = FindCtx { pid, found: None };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.found
}

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut _)
}

pub fn window_title(h: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd(h), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn window_rect(h: isize) -> Option<(i32, i32, i32, i32)> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd(h), &mut r).ok()? };
    Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}

pub fn move_window(h: isize, x: i32, y: i32, w: i32, hgt: i32) {
    unsafe {
        let _ = MoveWindow(hwnd(h), x, y, w, hgt, true);
    }
}

pub fn post_key(h: isize, vk: u32) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(h)), WM_KEYDOWN, WPARAM(vk as usize), LPARAM(0));
        let _ = PostMessageW(Some(hwnd(h)), WM_KEYUP, WPARAM(vk as usize), LPARAM(0));
    }
}

pub fn minimize(h: isize) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(h)), WM_SYSCOMMAND, WPARAM(SC_MINIMIZE as usize), LPARAM(0));
    }
}

pub fn close_window(h: isize) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(h)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Procesos lanzados por esta cuenta (por su BrowserTrackerId).
pub fn processes_for_tracker(tracker: &str) -> Vec<RbxProcess> {
    if tracker.is_empty() {
        return Vec::new();
    }
    roblox_processes().into_iter().filter(|p| p.tracker_id == tracker).collect()
}

// ---------------------------------------------------------------- multi roblox

struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}

static MULTI_MUTEX: Lazy<Mutex<Option<SendHandle>>> = Lazy::new(|| Mutex::new(None));

/// Mantiene (o suelta) el mutex único de Roblox para permitir varias instancias.
/// Devuelve false si Roblox ya estaba abierto y el mutex no se pudo tomar.
pub fn set_multi_roblox(enabled: bool) -> bool {
    let mut guard = MULTI_MUTEX.lock().unwrap();
    if enabled && guard.is_none() {
        unsafe {
            let Ok(h) = CreateMutexW(None, true, &HSTRING::from("ROBLOX_singletonMutex")) else { return false };
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // 0 = WAIT_OBJECT_0, 0x80 = WAIT_ABANDONED (dueño anterior murió): en ambos casos ya es nuestro
                let r = WaitForSingleObject(h, 0).0;
                if r != 0 && r != 0x80 {
                    let _ = CloseHandle(h);
                    return false;
                }
            }
            *guard = Some(SendHandle(h));
        }
    } else if !enabled {
        if let Some(h) = guard.take() {
            unsafe {
                let _ = ReleaseMutex(h.0);
                let _ = CloseHandle(h.0);
            }
        }
    }
    true
}

pub fn multi_active() -> bool {
    MULTI_MUTEX.lock().unwrap().is_some()
}

// ------------------------------------------------------- arreglo del error 773

static COOKIE_LOCK: Lazy<Mutex<Option<std::fs::File>>> = Lazy::new(|| Mutex::new(None));

/// Bloquea RobloxCookies.dat mientras la app está abierta: evita que Roblox mezcle sesiones
/// entre instancias (error 773 al teletransportar).
pub fn set_cookie_lock(enabled: bool) {
    use std::os::windows::fs::OpenOptionsExt;
    let mut guard = COOKIE_LOCK.lock().unwrap();
    if !enabled {
        *guard = None;
        return;
    }
    if guard.is_some() {
        return;
    }
    let path = crate::util::local_app_data().join("Roblox").join("LocalStorage").join("RobloxCookies.dat");
    if !path.exists() {
        return;
    }
    match std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&path) {
        Ok(f) => *guard = Some(f),
        Err(e) => log::warn!("No se pudo bloquear RobloxCookies.dat: {e}"),
    }
}

// ----------------------------------------------------------------- lanzador

/// Abre una URL de protocolo (roblox-player:) y espera a que el lanzador termine (máx. 90 s).
pub fn shell_launch_and_wait(url: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = HSTRING::from("open");
    let file = HSTRING::from(url);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: windows::core::PCWSTR(verb.as_ptr()),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| format!("No se pudo abrir Roblox: {e}. ¿Está instalado?"))?;
        if !info.hProcess.is_invalid() && !info.hProcess.0.is_null() {
            WaitForSingleObject(info.hProcess, 90_000);
            let _ = CloseHandle(info.hProcess);
        }
    }
    Ok(())
}
