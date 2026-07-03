//! Tauri command layer — thin shell over `sentient-installer-core`.
//! Phase 0: preflight checks. Phase 1: WSL2 provisioning + reboot-and-resume.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::Manager;

use sentient_installer_core::checks::{self, Check};
use sentient_installer_core::progress::{Progress, ProgressFn};
use sentient_installer_core::wsl;

/// Run every preflight check and return the results for the UI.
#[tauri::command]
fn preflight() -> Vec<Check> {
    checks::run_all()
}

#[derive(Serialize)]
pub struct WslResult {
    ready: bool,
    reboot_required: bool,
}

/// Install / enable WSL2, streaming progress to the webview. Returns whether WSL
/// is ready and whether a reboot is required to finish. Runs the (blocking, can
/// take minutes) work on a background thread so the UI stays responsive and the
/// progress channel delivers live — NOT on the main thread.
#[tauri::command]
async fn install_wsl(on_progress: Channel<Progress>) -> WslResult {
    let ch = on_progress;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let sink: ProgressFn = Arc::new(move |p| {
            let _ = ch.send(p);
        });
        wsl::install(sink)
    })
    .await
    .expect("wsl install task panicked");
    WslResult {
        ready: outcome.ready,
        reboot_required: outcome.reboot_required,
    }
}

/// Is WSL functional right now? (used after a reboot to verify).
#[tauri::command]
fn wsl_ready() -> bool {
    wsl::is_ready()
}

// ---- install-state persistence (survives reboots) ----------------------------

fn state_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("state.txt"))
}

/// The current step in the wizard, persisted so we resume after a reboot.
/// Defaults to "checks".
#[tauri::command]
fn get_state(app: tauri::AppHandle) -> String {
    state_file(&app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "checks".into())
}

#[tauri::command]
fn set_state(app: tauri::AppHandle, step: String) -> Result<(), String> {
    let p = state_file(&app).ok_or("no data dir")?;
    std::fs::write(p, step).map_err(|e| e.to_string())
}

// ---- reboot & resume ---------------------------------------------------------

/// Register a one-shot entry so the installer relaunches after the next login,
/// to resume where it left off. No-op off Windows.
#[tauri::command]
fn arm_resume() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        let out = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce",
                "/v",
                "SentientInstaller",
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }
    #[cfg(not(windows))]
    Ok(())
}

/// Restart the machine (short delay). No-op off Windows.
#[tauri::command]
fn reboot_now() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("shutdown")
            .args(["/r", "/t", "5", "/c", "Restarting to finish WSL2 setup for SENTIENT"])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    for var in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        if std::env::var_os(var).is_none() {
            std::env::set_var(var, "1");
        }
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            preflight,
            install_wsl,
            wsl_ready,
            get_state,
            set_state,
            arm_resume,
            reboot_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running SENTIENT Installer");
}
