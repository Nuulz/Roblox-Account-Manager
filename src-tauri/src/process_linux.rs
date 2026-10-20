//! Procesos de Roblox en Linux: las instancias de Sober lanzadas desde la app.
//! No hay ventanas Win32, así que "hwnd" es el pid de la instancia.

use std::sync::Mutex;

use once_cell::sync::Lazy;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::sober;

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

/// La memoria de Sober está en sus procesos hijos (flatpak → bwrap → sober).
fn tree_memory(sys: &System, root: u32) -> u64 {
    let mut total = 0;
    let mut stack = vec![Pid::from_u32(root)];
    while let Some(pid) = stack.pop() {
        if let Some(p) = sys.process(pid) {
            total += p.memory();
        }
        stack.extend(sys.processes().iter().filter(|(_, p)| p.parent() == Some(pid)).map(|(c, _)| *c));
    }
    total
}

pub fn roblox_processes() -> Vec<RbxProcess> {
    let running = sober::running();
    if running.is_empty() {
        return Vec::new();
    }
    let mut sys = SYS.lock().unwrap();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet).with_memory(),
    );
    running
        .into_iter()
        .map(|(tracker, pid)| {
            let p = sys.process(Pid::from_u32(pid));
            RbxProcess {
                pid,
                cmd: p.map(|p| p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ")).unwrap_or_default(),
                start_time: p.map(|p| p.start_time()).unwrap_or(0),
                memory_mb: tree_memory(&sys, pid) / 1024 / 1024,
                hwnd: Some(pid as isize),
                title: "Sober".into(),
                tracker_id: tracker,
            }
        })
        .collect()
}

pub fn kill(pid: u32) -> bool {
    match sober::running().into_iter().find(|(_, p)| *p == pid) {
        Some((tracker, _)) => {
            sober::stop(&tracker);
            true
        }
        None => false,
    }
}

pub fn window_rect(_h: isize) -> Option<(i32, i32, i32, i32)> {
    None
}

pub fn move_window(_h: isize, _x: i32, _y: i32, _w: i32, _hgt: i32) {}

pub fn post_key(_h: isize, _vk: u32) {}

pub fn minimize(_h: isize) {}

pub fn close_window(h: isize) {
    kill(h as u32);
}

pub fn processes_for_tracker(tracker: &str) -> Vec<RbxProcess> {
    if tracker.is_empty() {
        return Vec::new();
    }
    roblox_processes().into_iter().filter(|p| p.tracker_id == tracker).collect()
}

/// En Linux cada cuenta ya corre en su propia instancia de Sober.
pub fn set_multi_roblox(_enabled: bool) -> bool {
    true
}

pub fn multi_active() -> bool {
    sober::pasta().is_some()
}

pub fn set_cookie_lock(_enabled: bool) {}
