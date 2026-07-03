# SENTIENT Installer — Research & Plan

A single graphical wizard that gets a fresh Windows PC from "nothing installed"
to "SENTIENT running in the browser", by provisioning **Docker Engine inside
WSL2** (no Docker Desktop), deploying the SENTIENT stack from the public
registry, and installing the backup app — checking and remediating every
prerequisite along the way.

## v1 scope (resolved decisions)

| Decision | Choice |
| --- | --- |
| Platform (v1) | **Windows first** (WSL2 + Docker Engine). Linux/macOS later. |
| Docker on Windows | **Docker Engine inside WSL2** — no Docker Desktop (no license, lighter). |
| Images | **Pull from the public registry** (`quakestring/sentient` is public). Offline bundle later. |
| Tech stack | **Rust + Tauri** (same as the backup app; reuse the CI/installer patterns). |

## What "installed" means — the target

The SENTIENT stack is `docker compose` (from `/home/st/SENTIENT/docker-compose.yml`):

- **postgres** — `timescale/timescaledb:2.26.1-pg18`, container `sentient-postgres`,
  port `5432`, volume `postgres_data`, `POSTGRES_DB/USER=sentient`.
- **sentient** — `quakestring/sentient:latest` (public), container `sentient`,
  port `8080`, volume `sentient_data`, `DATABASE_URL=postgresql://sentient:sentient@postgres:5432/sentient`.

Done = both containers healthy, and **http://localhost:8080** reachable from the
Windows host (WSL2 forwards `localhost` automatically). First login (sys-admin):
`admin@sentient.local` / `admin123`.

## Architecture

- **Rust + Tauri** GUI (wizard). A thin frontend (like the backup app) over Rust
  commands that run the provisioning steps and stream progress/log lines.
- **Elevation**: the installer manifest requests admin (UAC). Privileged steps
  (`wsl --install`, feature enable) run elevated; in-distro steps run via
  `wsl -d sentient -u root -- <cmd>`.
- **State machine that survives reboots**: enabling WSL features needs a restart,
  so the installer persists its progress (a small state file / registry key) and
  registers a **RunOnce** entry to relaunch and resume at the right step.
- Reuse the backup app's Tauri progress/Channel patterns and GitHub Actions
  build → signed `.exe`/`.msi`.

## The install flow (Windows / Docker-in-WSL2)

1. **Detect & preflight** — Windows build/version, **CPU virtualization enabled**
   (detect only; can't toggle BIOS — instruct if off), admin rights, free disk,
   internet reachability, is WSL present, is the `sentient` distro already there.
   Each shown as ✓ / ✗ / fixable.
2. **WSL2** — if missing/outdated: `wsl --install --no-distribution`, `wsl --update`,
   `wsl --set-default-version 2`. If the WSL / VirtualMachinePlatform features were
   just enabled → **reboot + resume**.
3. **Dedicated distro** — download a known Ubuntu rootfs and `wsl --import sentient
   <dir> <rootfs>` (avoids the interactive Ubuntu first-run; isolated from the
   user's other distros; deterministic to tear down/reset).
4. **Docker Engine in the distro** — `curl -fsSL https://get.docker.com | sh` via
   `wsl -d sentient -u root`; enable **systemd** in `/etc/wsl.conf`
   (`[boot]\nsystemd=true`), `wsl --shutdown`, then `systemctl enable --now docker`.
   Verify `docker version` + `docker compose version`.
5. **Deploy SENTIENT** — write `docker-compose.yml` + `.env` into the distro
   (e.g. `/opt/sentient/`), `docker compose pull`, `docker compose up -d`, poll
   until `sentient-postgres` is healthy and `sentient` answers on `:8080`.
6. **Backup app** — download/run the SENTIENT Backup & Restore Windows installer
   (already built in the SENTIENT_BACKUP repo).
7. **Autostart** — a per-user Task Scheduler entry at logon:
   `wsl -d sentient -- docker compose -f /opt/sentient/docker-compose.yml up -d`
   (WSL2 distros don't auto-start).
8. **Finish** — open http://localhost:8080, show the URL + first-login credentials,
   offer "open now".

## The genuinely hard parts (and how we handle them)

- **Reboot & resume** — persist step state; register RunOnce to relaunch elevated
  and continue. The wizard shows "resuming after restart…".
- **systemd-in-WSL** — set `[boot] systemd=true` in `/etc/wsl.conf`, `wsl --shutdown`
  to apply, then start `docker` via systemd. (Fallback: launch `dockerd` + a
  keep-alive if systemd is unavailable on old WSL.)
- **Elevation** — request admin up front; keep privileged operations minimal and
  explicit.
- **BIOS virtualization** — detect (`systeminfo` / CPUID); if off, show clear
  per-vendor instructions and block Docker steps until fixed.
- **Autostart** — Task Scheduler logon task (documented above).
- **Idempotency & re-runs** — every step checks "already done?" so re-running the
  wizard repairs rather than duplicates. A "Reset" tears down the `sentient` distro.
- **Failure surfaces** — capture stderr from each `wsl`/`docker` call, show it in a
  collapsible log (reuse the backup-app progress widget), never freeze.

## Phases (one per session, like the backup app)

- **P0** — scaffold (Tauri + CI), preflight-checks screen (detect + ✓/✗, no changes).
- **P1** — WSL2 enable/update + the reboot-and-resume state machine.
- **P2** — distro import + Docker Engine + systemd; verify docker works.
- **P3** — deploy SENTIENT (compose pull/up), health-wait, open in browser.
- **P4** — install the backup app + autostart task + finish/verify + reset flow.

## Deferred / future

- Linux installer (native Docker Engine per distro — apt/dnf/pacman; much simpler).
- macOS (Docker via Colima/Podman, or Docker Desktop).
- Offline/air-gapped bundle (ship the images as a tar, `docker load`).
- Optional Docker Desktop path for users who prefer it.
- Uninstaller (tear down distro + app + tasks).
- Code signing (avoid SmartScreen), auto-update.

## Open questions

- Ubuntu rootfs source/pinning for `wsl --import` (official cloud image vs a
  fetched `wsl.rootfs`), and how much to cache/bundle vs download.
- Where to place the compose/.env inside the distro and how to template `.env`
  (ports, passwords) if we let the user customize.
- Whether to pin `quakestring/sentient:latest` to a specific tag for reproducible
  installs.
