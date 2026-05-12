use crate::paths::memory_home;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
}

pub fn pid_path() -> PathBuf {
    memory_home().join("daemon.pid")
}

pub fn read_info() -> Option<DaemonInfo> {
    let raw = fs::read_to_string(pid_path()).ok()?;
    let info: DaemonInfo = serde_json::from_str(raw.trim()).ok()?;
    if process_alive(info.pid) {
        Some(info)
    } else {
        let _ = fs::remove_file(pid_path());
        None
    }
}

pub fn write_info(info: &DaemonInfo) -> Result<()> {
    let path = pid_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string(info)?)?;
    Ok(())
}

pub fn clear_info() {
    let _ = fs::remove_file(pid_path());
}

pub fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc_kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    unsafe { kill(pid, sig) }
}

pub fn stop(timeout_ms: u64) -> Result<DaemonInfo> {
    let info = read_info().ok_or_else(|| anyhow!("daemon not running"))?;
    send_signal(info.pid, 15)?;
    let step = 50u64;
    let mut waited = 0u64;
    while waited < timeout_ms {
        if !process_alive(info.pid) {
            clear_info();
            return Ok(info);
        }
        std::thread::sleep(std::time::Duration::from_millis(step));
        waited += step;
    }
    send_signal(info.pid, 9)?;
    clear_info();
    Ok(info)
}

fn send_signal(pid: u32, sig: i32) -> Result<()> {
    #[cfg(unix)]
    {
        let result = unsafe { libc_kill(pid as i32, sig) };
        if result != 0 {
            return Err(anyhow!("kill({pid}, {sig}) failed"));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, sig);
        Err(anyhow!("daemon stop not supported on this platform"))
    }
}
