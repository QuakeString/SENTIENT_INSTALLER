// SENTIENT Installer frontend (Phase 0: preflight checks). Uses Tauri's global
// invoke (withGlobalTauri = true), no bundler.

const invoke = window.__TAURI__?.core?.invoke;
const $ = (id) => document.getElementById(id);

// follow OS theme (a manual toggle can come later)
if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)")) {
  // CSS media query handles it; nothing to do.
}

const ICON = { pass: "i-pass", setup: "i-setup", fail: "i-fail", unknown: "i-unknown" };

function renderChecks(list) {
  const el = $("checks");
  el.innerHTML = list
    .map(
      (c) => `
      <div class="check">
        <svg class="icon s-${c.status}"><use href="#${ICON[c.status] || "i-unknown"}"/></svg>
        <div class="body">
          <div class="label">${c.label}</div>
          <div class="detail">${c.detail}</div>
        </div>
      </div>`
    )
    .join("");

  const fails = list.filter((c) => c.status === "fail");
  const setups = list.filter((c) => c.status === "setup");
  const summary = $("summary");
  if (fails.length) {
    summary.className = "summary bad";
    summary.textContent = `${fails.length} blocker${fails.length > 1 ? "s" : ""} must be resolved before installing.`;
    $("next").disabled = true;
  } else {
    summary.className = "summary";
    summary.textContent = setups.length
      ? `Ready — the installer will set up ${setups.length} item${setups.length > 1 ? "s" : ""}.`
      : "Everything's ready.";
    $("next").disabled = false;
  }
  // Phase 0: the install steps aren't built yet.
  $("nextNote").textContent =
    "Provisioning (WSL2 → Docker → deploy SENTIENT) arrives in the next build.";
}

async function recheck() {
  if (!invoke) {
    $("summary").textContent = "Not running inside the app.";
    return;
  }
  $("recheck").disabled = true;
  $("checks").innerHTML =
    '<div class="check"><svg class="icon spin s-unknown"><use href="#i-spin"/></svg><div class="body"><div class="label">Checking…</div></div></div>';
  try {
    const list = await invoke("preflight");
    renderChecks(list);
  } catch (e) {
    $("summary").className = "summary bad";
    $("summary").textContent = "Check failed: " + e;
  } finally {
    $("recheck").disabled = false;
  }
}

$("recheck").addEventListener("click", recheck);
$("next").addEventListener("click", () => {
  $("nextNote").textContent =
    "The install steps (WSL2, Docker Engine, SENTIENT deploy) land in Phase 1+.";
});
recheck();
