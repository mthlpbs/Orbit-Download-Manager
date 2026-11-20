// Orbit's Figma popup: site controls and media first, with secondary preferences in Settings.
const ext = globalThis.browser ?? globalThis.chrome;
const HELP = "https://github.com/mthlpbs/Orbit-Download-Manager/blob/main/docs/browser-help.md";
const PROJECT = "https://github.com/mthlpbs/Orbit-Download-Manager";
const t = (key, ...subs) => ext.i18n.getMessage(key, subs) || key;
const $ = (id) => document.getElementById(id);
const ask = (message) => ext.runtime.sendMessage(message).catch(() => ({ ok: false }));
const lang = "en";
let connected = false;
let current = null;
let listedMedia = [];
let batchBusy = false;
const pending = new Set();
const rowButtons = new Map();
let toastTimer;
$("help").href = HELP;
$("support").href = PROJECT;

function notify(text, error = false) {
  clearTimeout(toastTimer);
  $("toast-text").textContent = text;
  $("toast").classList.toggle("error", error);
  $("toast").hidden = !text;
  toastTimer = setTimeout(() => { $("toast").hidden = true; }, error ? 5000 : 2200);
}

async function currentTab() {
  const page = new URLSearchParams(location.search).get("page");
  if (page) return (await ext.tabs.query({}).catch(() => [])).find((tab) => tab.url === page) || null;
  const [tab] = await ext.tabs.query({ active: true, currentWindow: true }).catch(() => []);
  return tab || null;
}

async function show() {
  const state = await ask({ type: "state" });
  connected = Boolean(state.connected);
  const updating = connected && state.why === "update-app";
  const status = $("status");
  status.classList.remove("ok", "bad", "warn");
  status.classList.add(!connected ? "bad" : state.paused || updating ? "warn" : "ok");
  $("status-text").textContent = !connected ? "App not connected" : state.paused ? "Capture paused" : "Connected · Capture on";
  $("status-text").title = state.app ? "Orbit " + state.app : "";
  $("take").checked = !state.paused;
  $("take").disabled = !connected;
  $("open").disabled = $("options").disabled = !connected;
  $("version").textContent = state.version || ext.runtime.getManifest().version || "";
  $("connection-alert").hidden = connected && !updating;
  $("popup").classList.toggle("has-alert", !connected || updating);
  $("alert-title").textContent = updating ? "Update Orbit for all features" : "Orbit can’t receive downloads";
  $("alert-description").textContent = updating ? "Some features need a newer Orbit app. Downloads continue to work." : "Install the Orbit app or finish browser setup. Until then, downloads stay in the browser.";
  $("fix-label").textContent = updating ? "Update Orbit" : "Set up Orbit";
  $("fix").href = updating ? PROJECT + "/releases/latest" : HELP + "#" + (state.why || "not-installed");
  showDeclined(connected && state.declined);
  current = await currentTab();
  let host = "";
  try { const url = new URL(current && current.url); if (/^https?:$/.test(url.protocol)) host = url.hostname; } catch { /* Browser pages have no site preferences. */ }
  await showSite(host, connected);
  await showMedia(current, connected);
}

async function showSite(host, available) {
  $("site").hidden = !host;
  $("site-host").textContent = host;
  $("site-host").title = host;
  const prefs = host && available ? await ask({ type: "site-settings", host }) : {};
  const rows = [
    [$("site-show-buttons"), "hidden", "hideUndo", "set-site-panels"],
    [$("site-capture"), "excluded", "captureUndo", "set-site-capture"],
  ];
  const managed = rows.some(([, key, undo]) => prefs[key] && !prefs[undo]);
  $("site-note").textContent = managed ? t("popup_site_managed") : "";
  $("site-note").hidden = !managed;
  $("popup").classList.toggle("has-site-note", managed);
  for (const [box, key, undo, type] of rows) {
    box.checked = !prefs[key];
    box.disabled = !available || !prefs.ok || Boolean(prefs[key] && !prefs[undo]);
    box.onchange = async () => {
      const checked = box.checked;
      box.disabled = true;
      const saved = await ask({ type, host, [key]: !checked });
      if (!saved.ok) { box.checked = !checked; notify(t("popup_site_failed"), true); }
      box.disabled = !connected;
    };
  }
}

function showDeclined(value) {
  const box = $("declined");
  box.hidden = !(value && value.why);
  $("popup").classList.toggle("has-declined", !box.hidden);
  if (box.hidden) return;
  const key = "declined_" + String(value.why).replace(/-/g, "_");
  const why = t(key);
  $("declined-what").textContent = value.name ? t("popup_declined", value.name) : t("popup_declined_unnamed");
  $("declined-why").textContent = why === key ? "" : why;
}

const TYPES = {
  "video/mp4": "MP4",
  "audio/mp4": "M4A",
  "audio/mpeg": "MP3",
  "video/webm": "WEBM",
  "audio/webm": "WEBM",
  "audio/ogg": "OGG",
  "video/ogg": "OGV",
  "video/quicktime": "MOV",
  "video/x-matroska": "MKV",
  "audio/flac": "FLAC",
  "audio/wav": "WAV",
};

// Its type as a short tag: HLS, DASH, else its file's extension as it will be saved (a script's
// answer: its type's, #800), else its type's.
function tagOf(m) {
  if (m.kind === "hls") return "HLS";
  if (m.kind === "dash") return "DASH";
  const ext = typeof m.name === "string" ? m.ext : (/\.([a-z0-9]{2,4})$/i.exec(m.url.split(/[?#]/)[0]) || [])[1];
  return (ext || TYPES[m.mime] || (m.kind === "audio" ? "AUDIO" : "VIDEO")).toUpperCase();
}

// Its name: a file's as it will be saved (#800), else the address's last part, readable.
function nameOf(m) {
  if (typeof m.name === "string" && m.name) return m.name;
  const url = m.url;
  const last = url.split(/[?#]/)[0].split("/").filter(Boolean).pop() || url;
  try {
    return decodeURIComponent(last);
  } catch {
    return last;
  }
}

function clock(seconds) {
  const s = Math.round(seconds);
  const two = (n) => String(n).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}` : `${Math.floor(s / 60)}:${two(s % 60)}`;
}

// English size labels, matching the rest of Orbit.
function sizeText(bytes) {
  const units = [["gigabyte", 1 << 30], ["megabyte", 1 << 20], ["kilobyte", 1 << 10]];
  const [unit, n] = units.find(([, n]) => bytes >= n) || ["byte", 1];
  try {
    return new Intl.NumberFormat(lang, { style: "unit", unit, unitDisplay: "short", maximumFractionDigits: 1 }).format(bytes / n);
  } catch {
    return `${(bytes / n).toFixed(1)} ${unit}`;
  }
}

// What is known of it: its qualities (or picture size), length, size; "Live" for a live stream.
function metaOf(m) {
  if (m.drm) return [(m.duration > 0 ? Math.round(m.duration / 60) + " min" : ""), "Can’t be saved"].filter(Boolean).join(" · ");
  const parts = [];
  if (m.kind === "audio") parts.push("Audio");
  else if (m.heights && m.heights.length) parts.push(Math.max(...m.heights) + "p");
  parts.push(tagOf(m));
  if (m.live) parts.push("Live");
  if (m.size > 0) parts.push(sizeText(m.size));
  return parts.join(" · ");
}


function icon(name) {
  const node = document.createElement("span");
  node.className = "icon icon-" + name;
  node.setAttribute("aria-hidden", "true");
  return node;
}

function iconButton(name, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "icon-button";
  button.setAttribute("aria-label", label);
  button.title = label;
  button.append(icon(name));
  return button;
}

function fallbackFor(media) { return "assets/fallback-" + (media.kind === "audio" ? "audio" : "video") + ".png"; }

function thumbnailOf(media) {
  const box = document.createElement("div");
  box.className = "thumbnail";
  const image = document.createElement("img");
  image.width = 64; image.height = 40; image.alt = ""; image.referrerPolicy = "no-referrer";
  image.src = typeof media.poster === "string" && /^https?:\/\//i.test(media.poster) ? media.poster : fallbackFor(media);
  image.addEventListener("error", () => { image.src = fallbackFor(media); }, { once: true });
  box.append(image);
  if (media.duration > 0) {
    const duration = document.createElement("span");
    duration.className = "duration"; duration.textContent = clock(media.duration); box.append(duration);
  }
  return box;
}

function updateDownloadControls() {
  for (const [url, button] of rowButtons) button.disabled = !connected || batchBusy || pending.has(url);
  const saveable = listedMedia.filter((media) => !media.drm);
  $("download-all-label").textContent = "Download all (" + saveable.length + ")";
  $("download-all").hidden = !listedMedia.length;
  $("download-all").disabled = !connected || !saveable.length || batchBusy || pending.size > 0;
}

async function downloadMedia(media) {
  if (!connected || media.drm || pending.has(media.url) || !current) return false;
  pending.add(media.url); updateDownloadControls();
  try {
    const result = await ask({ type: "media-download", tab: current.id, url: media.url });
    return Boolean(result.ok);
  } finally { pending.delete(media.url); updateDownloadControls(); }
}

async function showMedia(tab, available) {
  const reply = tab && Number.isInteger(tab.id) ? await ask({ type: "media", tab: tab.id }) : {};
  listedMedia = Array.isArray(reply.media) && !reply.keptOff ? reply.media : [];
  const list = $("media-list");
  const previousScroll = list.scrollTop;
  list.replaceChildren(); rowButtons.clear();
  $("media-count").textContent = listedMedia.length;
  $("media-count").hidden = !listedMedia.length;
  list.hidden = !listedMedia.length;
  $("media-none").hidden = listedMedia.length > 0;
  $("media-empty-description").textContent = reply.keptOff ? t("popup_media_store") : reply.streamed ? t("popup_media_streamed") : "Play the video or audio on this page, then reopen Orbit.";
  for (const media of listedMedia) {
    const row = document.createElement("li");
    row.dataset.kind = media.kind; row.dataset.format = tagOf(media);
    row.classList.toggle("protected", Boolean(media.drm));
    row.append(thumbnailOf(media));
    const text = document.createElement("div"); text.className = "what";
    const name = document.createElement("span"); name.className = "name"; name.textContent = nameOf(media); name.title = nameOf(media);
    const meta = document.createElement("div"); meta.className = "meta";
    if (media.drm) {
      const badge = document.createElement("span"); badge.className = "drm"; badge.append(icon("lock"), "DRM"); meta.append(badge);
    }
    const detail = document.createElement("span"); detail.className = "meta-text"; detail.textContent = metaOf(media); meta.append(detail);
    text.append(name, meta); row.append(text);
    const actions = document.createElement("div"); actions.className = "media-actions";
    const copy = iconButton("link", "Copy link to " + nameOf(media));
    copy.classList.add("copy-link");
    copy.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(media.url); notify("Link copied"); }
      catch { notify("Could not copy the link. Please try again.", true); }
    });
    actions.append(copy);
    if (media.drm) {
      const locked = iconButton("lock", "Protected media — can’t be saved"); locked.disabled = true; actions.append(locked);
    } else {
      const download = iconButton("download", "Download " + nameOf(media));
      download.classList.add("download"); download.disabled = !available;
      rowButtons.set(media.url, download);
      download.addEventListener("click", async () => {
        if (batchBusy) return;
        const added = await downloadMedia(media);
        notify(added ? "Added to Orbit" : "Could not add the download. Please try again.", !added);
      });
      actions.append(download);
    }
    row.append(actions); list.append(row);
  }
  list.scrollTop = previousScroll;
  updateDownloadControls();
}

$("download-all").addEventListener("click", async () => {
  if (batchBusy || !connected || pending.size) return;
  const files = listedMedia.filter((media) => !media.drm);
  if (!files.length) return;
  batchBusy = true; updateDownloadControls();
  let added = 0;
  try { for (const media of files) if (await downloadMedia(media)) added++; }
  finally { batchBusy = false; updateDownloadControls(); }
  notify(added === files.length ? "Added " + added + (added === 1 ? " download" : " downloads") + " to Orbit" : "Added " + added + " of " + files.length + " downloads. Try the remaining files again.", added !== files.length);
});

function settingsView(open) {
  $("main-view").hidden = open; $("settings-view").hidden = !open;
  document.title = open ? "Settings · Orbit" : "Orbit Download Manager";
  if (open) $("back").focus(); else $("settings").focus();
}
$("settings").addEventListener("click", () => settingsView(true));
$("back").addEventListener("click", () => settingsView(false));
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !$("settings-view").hidden) { event.preventDefault(); settingsView(false); }
});

async function storedSwitch(id, key, after, defaultValue = false) {
  const box = $(id);
  const prefs = await ext.storage.local.get({ [key]: defaultValue }).catch(() => ({ [key]: defaultValue }));
  box.checked = Boolean(prefs[key]); box.disabled = false;
  box.addEventListener("change", async () => {
    const checked = box.checked; box.disabled = true;
    try { await ext.storage.local.set({ [key]: checked }); if (after) await after(); }
    catch { box.checked = !checked; notify("Could not save this setting. Please try again.", true); }
    finally { box.disabled = false; }
  });
}
storedSwitch("media-title-name", "mediaTitleName", () => showMedia(current, connected));
storedSwitch("site-cookies", "useSiteCookies", null, true);
storedSwitch("show-download-button", "showDownloadButton", async () => {
  let host = "";
  try { const url = new URL(current && current.url); if (/^https?:$/.test(url.protocol)) host = url.hostname; } catch {}
  await showSite(host, connected);
}, true);
storedSwitch("video-hide-fullscreen", "hideVideoFullscreen");

const PROXY = { permissions: ["proxy"] };
const manifest = ext.runtime.getManifest();
const proxy = $("proxy");
if ((manifest.optional_permissions || []).includes("proxy")) {
  const syncProxy = () => ext.permissions.contains(PROXY).then((on) => { proxy.checked = Boolean(on); }).catch(() => { proxy.checked = false; });
  syncProxy();
  proxy.addEventListener("change", () => {
    // The permission request must stay in the user's click, before any await.
    const asked = proxy.checked ? ext.permissions.request(PROXY) : ext.permissions.remove(PROXY);
    asked.catch(() => { notify("Proxy permission was not granted.", true); }).then(syncProxy);
  });
} else if ((manifest.permissions || []).includes("proxy")) {
  storedSwitch("proxy", "proxyOn");
} else {
  proxy.disabled = true;
  $("proxy-row").title = "Proxy routing is unavailable in this browser.";
}

const SKIP_KEYS = ["Alt", "Ctrl", "Shift"];
function showSkipKey(value) {
  const key = SKIP_KEYS.includes(value) ? value : "Alt";
  $("skip-key").value = key; $("skip-key").style.width = key === "Alt" ? "52px" : "64px"; $("skip-hint").textContent = key;
}
ext.storage.local.get({ skipKey: "Alt" }).then((prefs) => showSkipKey(prefs.skipKey)).catch(() => showSkipKey("Alt"));
$("skip-key").addEventListener("change", async () => {
  const key = $("skip-key").value;
  try { await ext.storage.local.set({ skipKey: key }); showSkipKey(key); }
  catch { notify("Could not save the skip key. Please try again.", true); ext.storage.local.get({ skipKey: "Alt" }).then((prefs) => showSkipKey(prefs.skipKey)); }
});

$("take").addEventListener("change", async () => {
  const checked = $("take").checked; $("take").disabled = true;
  const result = await ask({ type: "pause", paused: !checked });
  if (!result.ok) { $("take").checked = !checked; notify("Could not change capture. Please try again.", true); }
  await show();
});
$("retry").addEventListener("click", async () => {
  $("retry").disabled = true;
  try { await show(); } finally { $("retry").disabled = false; }
});
$("open").addEventListener("click", async () => {
  if ((await ask({ type: "open-app" })).ok) window.close(); else notify("Could not open Orbit. Please try again.", true);
});
$("options").addEventListener("click", async () => {
  if ((await ask({ type: "open-options" })).ok) window.close(); else notify("Could not open Orbit’s Options. Please try again.", true);
});

// A removed remote destination must never remain active behind a local-only popup.
ext.storage.local.remove("remote").then(show, show);
