// Mooncrest Launcher Setup – window logic.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;
const { open } = window.__TAURI__.dialog;

const de = (navigator.language || "de").toLowerCase().startsWith("de");
const T = de ? {
  folder: "Installationsordner", change: "Ändern", desktop: "Verknüpfung auf dem Desktop",
  startmenu: "Eintrag im Startmenü", launchAfter: "Danach starten", cancel: "Abbrechen", close: "Schließen", start: "Mooncrest Launcher starten",
  dontClose: "Das dauert nur einen Moment.",
  installTitle: "Mooncrest Launcher installieren",
  installLead: "Deine Plattform für alle Produkte der Mooncrest Group. Installiert wird nur für dein Windows-Konto, ohne Administratorrechte.",
  installButton: "Installieren",
  updateTitle: "Mooncrest Launcher aktualisieren",
  updateLead: (from, to) => `Version ${from} ist installiert. Das Update auf ${to} behält deine Einstellungen, Downloads und installierten Produkte.`,
  updateButton: "Aktualisieren",
  reinstallTitle: "Mooncrest Launcher reparieren",
  reinstallLead: (v) => `Version ${v} ist schon installiert. Die Launcher-Dateien werden neu geschrieben, deine Daten bleiben.`,
  reinstallButton: "Neu installieren",
  free: (s) => `${s} frei, ${"{need}"} benötigt`,
  stages: { prepare: "Wird vorbereitet", waiting: "Warte auf den Launcher", copy: "Dateien werden kopiert", register: "Wird bei Windows eingetragen", shortcuts: "Verknüpfungen werden angelegt", done: "Fertig" },
  doneTitle: "Alles bereit.", doneLead: "Der Mooncrest Launcher ist installiert. Beyond the Horizon.",
  running: "Der Mooncrest Launcher läuft gerade. Schließe ihn, dann geht es weiter.",
  closeIt: "Launcher beenden",
  failed: "Die Installation hat nicht geklappt:",
  unTitle: "Mooncrest Launcher deinstallieren",
  unLead: "Installierte Produkte wie MLBClient und deine Downloads bleiben erhalten.",
  unData: "Auch Einstellungen, Profil und Statistik des Launchers löschen",
  unButton: "Deinstallieren",
  unDoneTitle: "Deinstalliert.", unDoneLead: "Der Mooncrest Launcher wurde entfernt. Bis bald.",
  pick: "Installationsordner wählen",
} : {
  folder: "Install folder", change: "Change", desktop: "Desktop shortcut",
  startmenu: "Start menu entry", launchAfter: "Start when done", cancel: "Cancel", close: "Close", start: "Start Mooncrest Launcher",
  dontClose: "This only takes a moment.",
  installTitle: "Install Mooncrest Launcher",
  installLead: "Your home for every Mooncrest Group product. Installs for your Windows account only, no administrator rights needed.",
  installButton: "Install",
  updateTitle: "Update Mooncrest Launcher",
  updateLead: (from, to) => `Version ${from} is installed. Updating to ${to} keeps your settings, downloads and installed products.`,
  updateButton: "Update",
  reinstallTitle: "Repair Mooncrest Launcher",
  reinstallLead: (v) => `Version ${v} is already installed. The launcher files are written again, your data stays.`,
  reinstallButton: "Reinstall",
  free: (s) => `${s} free, ${"{need}"} needed`,
  stages: { prepare: "Preparing", waiting: "Waiting for the launcher", copy: "Copying files", register: "Registering with Windows", shortcuts: "Creating shortcuts", done: "Done" },
  doneTitle: "All set.", doneLead: "Mooncrest Launcher is installed. Beyond the Horizon.",
  running: "Mooncrest Launcher is running. Close it to continue.",
  closeIt: "Close launcher",
  failed: "The installation did not work:",
  unTitle: "Uninstall Mooncrest Launcher",
  unLead: "Installed products such as MLBClient and your downloads are kept.",
  unData: "Also delete the launcher's settings, profile and statistics",
  unButton: "Uninstall",
  unDoneTitle: "Uninstalled.", unDoneLead: "Mooncrest Launcher was removed. See you soon.",
  pick: "Choose install folder",
};

const $ = (id) => document.getElementById(id);
const win = getCurrentWindow();
document.documentElement.lang = de ? "de" : "en";
document.querySelectorAll("[data-t]").forEach((el) => { el.textContent = T[el.dataset.t]; });
$("min").onclick = () => win.minimize();
$("close").onclick = () => { if (!busy) win.close(); };

const fmt = (n) => {
  const u = ["B", "KB", "MB", "GB", "TB"]; let i = 0; let v = n;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v.toLocaleString(de ? "de-DE" : "en-US", { maximumFractionDigits: i > 1 ? 1 : 0 })} ${u[i]}`;
};

// The moon's limb is the progress bar.
const limb = $("limb-fill");
const len = limb.getTotalLength();
limb.style.strokeDasharray = `${len}`;
const setProgress = (p) => { limb.style.strokeDashoffset = `${len * (1 - p)}`; };
setProgress(0);

let info = null;
let dir = "";
let busy = false;

const show = (id) => { document.querySelectorAll(".view").forEach((v) => { v.hidden = v.id !== id; }); };

async function updateSpace() {
  $("dir").textContent = dir;
  $("dir").title = dir;
  const free = await invoke("disk_free", { dir }).catch(() => null);
  const need = info.payload_size + 32 * 1024 * 1024;
  $("space").textContent = free == null ? "" : T.free(fmt(free)).replace("{need}", fmt(need));
}

function showError(el, text, withClose) {
  el.hidden = false;
  el.textContent = text;
  if (withClose) {
    const b = document.createElement("button");
    b.className = "btn";
    b.textContent = T.closeIt;
    b.onclick = async () => { await invoke("close_launcher").catch(() => {}); el.hidden = true; };
    el.appendChild(b);
  }
}

async function runInstall() {
  $("setup-error").hidden = true;
  busy = true;
  show("v-progress");
  $("stage").textContent = T.stages.prepare;
  try {
    await invoke("run_install", { dir, desktop: $("opt-desktop").checked, startMenu: $("opt-start").checked });
    busy = false;
    if ($("opt-launch").checked) {
      await invoke("launch_and_exit", { dir });
      return;
    }
    show("v-done");
  } catch (e) {
    busy = false;
    setProgress(0);
    show("v-setup");
    const msg = String(e);
    if (msg === "running") showError($("setup-error"), T.running, true);
    else showError($("setup-error"), `${T.failed} ${msg}`);
  }
}

listen("progress", (ev) => {
  const { stage, pct } = ev.payload;
  setProgress(pct);
  $("pct").textContent = `${Math.round(pct * 100)} %`;
  $("stage").textContent = T.stages[stage] ?? "";
});

(async () => {
  info = await invoke("info");
  $("ver").textContent = `Version ${info.version}`;
  dir = info.default_dir;

  if (info.mode === "uninstall") {
    show("v-uninstall");
    $("un-cancel").onclick = () => win.close();
    $("un-go").onclick = async () => {
      busy = true;
      $("un-go").disabled = true;
      $("un-error").hidden = true;
      try {
        await invoke("run_uninstall", { removeData: $("opt-data").checked });
        busy = false;
        setProgress(1);
        $("done-title").textContent = T.unDoneTitle;
        $("done-lead").textContent = T.unDoneLead;
        $("start").hidden = true;
        $("finish").onclick = () => win.close();
        show("v-done");
      } catch (e) {
        busy = false;
        $("un-go").disabled = false;
        const msg = String(e);
        if (msg === "running") showError($("un-error"), T.running, true);
        else showError($("un-error"), `${T.failed} ${msg}`);
      }
    };
    return;
  }

  const ex = info.existing;
  if (info.mode === "update") {
    $("setup-title").textContent = T.updateTitle;
    $("setup-lead").textContent = T.updateLead(ex.version ?? "?", info.version);
    $("go").textContent = T.updateButton;
  } else if (info.mode === "reinstall") {
    $("setup-title").textContent = T.reinstallTitle;
    $("setup-lead").textContent = T.reinstallLead(info.version);
    $("go").textContent = T.reinstallButton;
  } else {
    $("setup-title").textContent = T.installTitle;
    $("setup-lead").textContent = T.installLead;
    $("go").textContent = T.installButton;
  }
  // An existing installation keeps its folder.
  $("pick").hidden = !!ex;
  $("opt-desktop").checked = !ex;
  show("v-setup");
  updateSpace();

  $("pick").onclick = async () => {
    const chosen = await open({ directory: true, title: T.pick, defaultPath: dir });
    if (typeof chosen === "string") {
      const clean = chosen.replace(/[\\/]+$/, "");
      dir = /mooncrest launcher$/i.test(clean) ? clean : `${clean}\\Mooncrest Launcher`;
      updateSpace();
    }
  };
  $("cancel").onclick = () => win.close();
  $("go").onclick = runInstall;
  $("finish").onclick = () => win.close();
  $("start").onclick = () => invoke("launch_and_exit", { dir });
  $("done-title").textContent = T.doneTitle;
  $("done-lead").textContent = T.doneLead;
  $("go").focus();
})();
