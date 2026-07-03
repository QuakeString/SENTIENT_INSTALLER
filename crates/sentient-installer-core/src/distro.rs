//! Phase 2: the dedicated `sentient` WSL distro + Docker Engine.
//! Downloads an Ubuntu rootfs, `wsl --import`s it, enables systemd, installs
//! Docker Engine inside it, and verifies. Idempotent — re-running repairs.

use std::path::Path;

use crate::progress::{Progress, ProgressFn};
#[cfg(windows)]
use crate::sys;

pub const DISTRO: &str = "sentient";
#[cfg_attr(not(windows), allow(dead_code))]
const ROOTFS_URL: &str =
    "https://cloud-images.ubuntu.com/wsl/releases/24.04/current/ubuntu-noble-wsl-amd64-24.04lts.rootfs.tar.gz";

/// Is the distro present AND Docker responding?
pub fn is_ready() -> bool {
    #[cfg(windows)]
    {
        distro_present()
            && sys::output("wsl.exe", &["-d", DISTRO, "-u", "root", "--", "docker", "version"])
                .map(|(ok, _, _)| ok)
                .unwrap_or(false)
    }
    #[cfg(not(windows))]
    false
}

/// Full Phase-2 setup. `install_dir` is where the distro's disk lives.
pub fn setup(sink: ProgressFn, install_dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::fs::create_dir_all(install_dir).map_err(|e| e.to_string())?;

        if !distro_present() {
            let rootfs = install_dir.join("ubuntu.rootfs.tar.gz");
            sink(Progress::Step { name: "Downloading Ubuntu base image (~350 MB)".into() });
            download(ROOTFS_URL, &rootfs, &sink)?;

            sink(Progress::Step { name: "Creating the SENTIENT WSL distro".into() });
            let distro_dir = install_dir.join("distro");
            std::fs::create_dir_all(&distro_dir).ok();
            wsl_native(&sink, &[
                "--import", DISTRO,
                &distro_dir.to_string_lossy(),
                &rootfs.to_string_lossy(),
                "--version", "2",
            ])?;
            let _ = std::fs::remove_file(&rootfs);
        }

        sink(Progress::Step { name: "Enabling systemd".into() });
        indistro(&sink, r"printf '[boot]\nsystemd=true\n' > /etc/wsl.conf")?;
        // restart the distro so systemd becomes PID 1
        let _ = sys::output("wsl.exe", &["--terminate", DISTRO]);

        sink(Progress::Step { name: "Installing Docker Engine (a few minutes)".into() });
        indistro_stream(&sink, "command -v docker >/dev/null 2>&1 || (curl -fsSL https://get.docker.com | sh)")?;

        sink(Progress::Step { name: "Starting Docker".into() });
        indistro(&sink, "systemctl enable --now docker")?;

        if is_ready() {
            sink(Progress::Done { message: "Docker Engine is installed and running.".into() });
            Ok(())
        } else {
            Err("Docker was installed but isn't responding yet — try the step again.".into())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = install_dir;
        sink(Progress::Error { message: "Docker setup is Windows-only.".into() });
        Err("Windows only".into())
    }
}

// ---- helpers (Windows) -------------------------------------------------------

#[cfg(windows)]
fn distro_present() -> bool {
    sys::output("wsl.exe", &["-l", "-q"])
        .map(|(_, out, _)| sys::decode(&out).lines().any(|l| l.trim().eq_ignore_ascii_case(DISTRO)))
        .unwrap_or(false)
}

/// Run a native `wsl.exe` command (UTF-16 output), stream it, error on failure.
#[cfg(windows)]
fn wsl_native(sink: &ProgressFn, args: &[&str]) -> Result<(), String> {
    match sys::output("wsl.exe", args) {
        Some((ok, out, err)) => {
            emit(sink, &out);
            emit(sink, &err);
            if ok { Ok(()) } else { Err(format!("wsl {} failed", args.join(" "))) }
        }
        None => Err("could not run wsl.exe".into()),
    }
}

/// Run a bash command inside the distro as root (output is UTF-8), error on fail.
#[cfg(windows)]
fn indistro(sink: &ProgressFn, bash: &str) -> Result<(), String> {
    match sys::output("wsl.exe", &["-d", DISTRO, "-u", "root", "--", "bash", "-lc", bash]) {
        Some((ok, out, err)) => {
            emit(sink, &out);
            emit(sink, &err);
            if ok { Ok(()) } else { Err(format!("in-distro command failed: {bash}")) }
        }
        None => Err("could not run wsl.exe".into()),
    }
}

/// Like `indistro`, but streams stdout/stderr line-by-line live (for long steps
/// like the Docker install).
#[cfg(windows)]
fn indistro_stream(sink: &ProgressFn, bash: &str) -> Result<(), String> {
    use std::io::BufRead;
    use std::process::Stdio;
    let mut child = sys::command("wsl.exe")
        .args(["-d", DISTRO, "-u", "root", "--", "bash", "-lc", bash])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (s1, s2) = (sink.clone(), sink.clone());
    let t1 = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            s1(Progress::Log { line });
        }
    });
    let t2 = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
            s2(Progress::Log { line });
        }
    });
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = t1.join();
    let _ = t2.join();
    if status.success() { Ok(()) } else { Err(format!("in-distro command failed: {bash}")) }
}

#[cfg(windows)]
fn emit(sink: &ProgressFn, bytes: &[u8]) {
    for line in sys::decode(bytes).lines() {
        let l = line.trim();
        if !l.is_empty() {
            sink(Progress::Log { line: l.into() });
        }
    }
}

/// Download a URL to a file, reporting percent progress.
#[cfg(windows)]
fn download(url: &str, dest: &Path, sink: &ProgressFn) -> Result<(), String> {
    use std::io::{Read, Write};
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let total: u64 = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    let mut done: u64 = 0;
    let mut last = -1i64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if total > 0 {
            let pct = (done * 100 / total) as i64;
            if pct != last {
                last = pct;
                sink(Progress::Percent { value: pct as f32 / 100.0 });
            }
        }
    }
    Ok(())
}
