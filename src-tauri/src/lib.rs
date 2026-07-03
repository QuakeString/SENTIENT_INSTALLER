//! Tauri command layer — thin shell over `sentient-installer-core`. Phase 0
//! exposes the read-only preflight checks; later phases add provisioning
//! commands (WSL2, Docker, deploy) that stream progress.

use sentient_installer_core::checks::{self, Check};

/// Run every preflight check and return the results for the UI.
#[tauri::command]
fn preflight() -> Vec<Check> {
    checks::run_all()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // WebKitGTK crashes on some Linux GPU setups without these (dev runs only);
    // harmless on Windows/macOS.
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
        .invoke_handler(tauri::generate_handler![preflight])
        .run(tauri::generate_context!())
        .expect("error while running SENTIENT Installer");
}
