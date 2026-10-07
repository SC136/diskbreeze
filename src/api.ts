import type { CleanReport, DriveInfo, Finding, HistoryEntry, ScanResult, Selection, UpdateInfo } from "./types";

const GB = 1024 ** 3;
const MB = 1024 ** 2;

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Realistic sample data so the UI can be developed and screenshotted in a plain browser. */
const mockFindings: Finding[] = [
  f("old-project-builds", "Build files in old projects", "Old project builds", "safe", 66 * GB,
    "node_modules, build output and virtual environments in 192 projects you haven't edited in 60+ days. Your code isn't touched; these folders are rebuilt from it.",
    "When you go back to a project, run its install or build step again.", "Deletes 192 folders",
    [["C:\\code\\shop-app\\node_modules", 4.7 * GB, "node_modules · last edited 3 months ago"], ["C:\\code\\notes-app\\src-tauri\\target", 7.6 * GB, "target · last edited 4 months ago"],
     ["C:\\code\\video-tool\\build", 2.4 * GB, "build · last edited 5 months ago"], ["C:\\code\\music-site\\web\\.next", 1.5 * GB, ".next · last edited 2 months ago"]]),
  f("npm-cache", "npm cache", "Developer caches", "safe", 2.9 * GB, "A copy of every npm package you've ever installed.", "The next npm install downloads packages again.", "Runs `npm cache clean --force`", []),
  f("gradle-caches", "Gradle caches", "Developer caches", "safe", 16.3 * GB, "Dependencies and build caches for Gradle and Android projects, often for many old Gradle versions.", "The next Gradle or Android build downloads dependencies again. Close Android Studio first.", "Deletes 1 folder", []),
  f("browser-caches", "Browser caches", "Browsers & apps", "safe", 4.4 * GB, "Copies of websites your browsers keep to load pages faster. Logins, history and bookmarks are not touched.", "Sites load a little slower the first time. Close your browsers first so nothing is skipped.", "Empties 27 folders", []),
  f("temp-files", "Temporary files", "Windows", "safe", 26 * MB, "Leftovers from installers and apps. Anything changed in the last 2 days is kept.", "Nothing visible. Files in use are skipped.", "Empties 1 folder", []),
  f("huggingface-hub", "Hugging Face models", "AI models", "ask", 17.7 * GB, "AI models downloaded by Python scripts (transformers, diffusers…).", "Each model downloads again the next time a script loads it.", "Deletes 1 folder", []),
  f("windows-update-leftovers", "Windows Update leftovers", "Windows", "ask", 7.5 * GB, "Update files Windows already downloaded and installed.", "Windows downloads an update again if it ever needs it.", "Windows will ask for administrator permission to clear downloaded Windows update files", []),
  f("hibernation-file", "Hibernation file", "Windows", "ask", 6.1 * GB, "Where Windows saves memory when the PC hibernates. Fast Startup uses it too.", "You lose Hibernate and Fast Startup. To get them back, run: powercfg /h on", "Windows will ask for administrator permission to turn off hibernation", []),
  { ...f("component-store", "Old Windows component versions", "Windows", "ask", 0, "After updates, Windows keeps old versions of its own system components. This removes the ones nothing needs anymore.", "Takes a few minutes. The saving varies, usually 1 to 5 GB.", "Windows will ask for administrator permission to clean up old Windows component versions", []), estimate: true },
  { ...f("wsl-vhdx-compact", "Shrink WSL and Docker disk files", "Docker & WSL", "ask", 11.5 * GB, "WSL distributions and Docker Desktop keep their data in disk files that grow but never shrink by themselves. Nothing inside them is deleted.", "WSL and Docker Desktop's engine are stopped while this runs. How much comes back depends on how much free space is inside each file; it can be small.", "Windows will ask for administrator permission to shrink WSL and Docker disk files",
    [["C:\\Users\\you\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx", 8.6 * GB, ""], ["C:\\Users\\you\\AppData\\Local\\wsl\\{9504e1e2}\\ext4.vhdx", 2.8 * GB, ""]]), estimate: true, selectable: false },
  f("onedrive-free-up", "OneDrive files you can keep online only", "OneDrive", "ask", 0.43 * GB, "Big files stored both here and in OneDrive that haven't changed in over a month. Making them online-only frees the space here; the copy in OneDrive stays safe.", "They show a cloud icon in File Explorer and download again when you open them.", "Makes 3 files online-only; the copy in OneDrive stays",
    [["C:\\Users\\you\\OneDrive\\Music\\Album\\01 Intro.flac", 0.05 * GB, "last changed 8 months ago"], ["C:\\Users\\you\\OneDrive\\Documents\\Recording (17).m4a", 0.026 * GB, "last changed 1 months ago"], ["C:\\Users\\you\\OneDrive\\Videos\\clip.mp4", 0.35 * GB, "last changed 5 months ago"]]),
  f("installed-apps", "Installed programs", "Apps", "manual", 12.4 * GB, "Your biggest installed programs. Sizes are what each program reports about itself, so they can be rough.", null, "You do this one yourself",
    [["HKLM|SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Blender", 5.2 * GB, "Blender · Blender Foundation, Jan 2026"], ["HKLM|SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\VS", 3.4 * GB, "Visual Studio Build Tools 2022 · Microsoft Corporation, Jul 2026"]]),
  f("downloads-disk-images", "Disk images (ISO files)", "Downloads", "ask", 22.4 * GB, "Operating system and software images. Once installed or written to a USB stick you rarely need them again.", "They go to the Recycle Bin, so you can still get them back.", "Moves 10 items to the Recycle Bin",
    [["C:\\Users\\you\\Downloads\\Windows.iso", 6.6 * GB, "downloaded 3 weeks ago"], ["C:\\Users\\you\\Downloads\\ubuntu-26.04-desktop-amd64.iso", 6.1 * GB, "downloaded 4 months ago"],
     ["C:\\Users\\you\\Downloads\\pop-os_24.04_amd64_generic_27.iso", 3.3 * GB, "downloaded 2 months ago"], ["C:\\Users\\you\\Downloads\\debian-13.6.0-amd64-netinst.iso", 0.7 * GB, "downloaded 2 months ago"]], true),
  f("recycle-bin", "Recycle Bin", "Recycle Bin", "ask", 33 * GB, "207 files and folders you deleted earlier. They still use space until the bin is emptied.", "They're gone for good. Open the bin first if you might want something back.", "Empties the Recycle Bin permanently", []),
  f("steam-games", "Steam games", "Games", "manual", 227.5 * GB, "Installed games. Uninstalling frees the space, and you can reinstall any time.", null, "You do this one yourself",
    [["Grand Theft Auto V", 119 * GB, "Grand Theft Auto V · last played 2 months ago"], ["Counter-Strike 2", 69 * GB, "Counter-Strike 2 · last played yesterday"]]),
  f("chrome-ai-model", "Chrome's built-in AI model", "AI models", "manual", 8 * GB, "Gemini Nano, used by Chrome features like Help me write.", null, "You do this one yourself", []),
];

function f(id: string, name: string, category: string, tier: Finding["tier"], bytes: number, what: string, after: string | null,
  action: string, items: [string, number, string][], recycles = false): Finding {
  return {
    id, name, category, tier, what, after, bytes: Math.round(bytes), action, recycles, open: null,
    selectable: items.length > 1 && tier !== "manual",
    estimate: false,
    how: tier === "manual" ? "Open the app and remove what you don't use. DiskBreeze can't do this one for you safely." : null,
    items: items.map(([path, b, note]) => ({ path, bytes: Math.round(b), note, open: null })),
  };
}

const mockDisk = { mount: "C:\\", total: 932 * GB, free: 56.6 * GB };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const mockDrives: DriveInfo[] = [
  { letter: "C:", label: "OS", total: 932 * GB, free: 56.6 * GB, removable: false, hasProfile: true },
  { letter: "D:", label: "Data", total: 1863 * GB, free: 612 * GB, removable: false, hasProfile: false },
  { letter: "E:", label: "USB Drive", total: 59 * GB, free: 21 * GB, removable: true, hasProfile: false },
];

/** Sample findings for a drive that doesn't hold the user's profile. */
const mockDataFindings: Finding[] = [
  f("old-project-builds", "Build files in old projects", "Old project builds", "safe", 11.2 * GB,
    "node_modules, build output and virtual environments in 14 projects you haven't edited in 60+ days. Your code isn't touched; these folders are rebuilt from it.",
    "When you go back to a project, run its install or build step again.", "Deletes 14 folders",
    [["D:\\Work\\store-front\\node_modules", 3.1 * GB, "node_modules · last edited 4 months ago"], ["D:\\Work\\game-jam\\Library", 2.6 * GB, "Library · last edited 8 months ago"]]),
  f("bigfiles", "Big files you haven't opened in months", "Big files", "ask", 38.4 * GB,
    "Large disk images, archives, videos, VM disks and backups that haven't changed in over 3 months. Only loose files like these are listed, never parts of an installed game or app.",
    "They go to the Recycle Bin, so you can still get them back.", "Moves 3 items to the Recycle Bin",
    [["D:\\Backups\\laptop-2025.vhdx", 24 * GB, "last changed 14 months ago"], ["D:\\Videos\\raw\\trip-footage.mkv", 9.8 * GB, "last changed 7 months ago"], ["D:\\Installers\\win11.iso", 4.6 * GB, "last changed 5 months ago"]], true),
  f("steam-games", "Steam games", "Games", "manual", 84 * GB, "Installed games. Uninstalling frees the space, and you can reinstall any time.", null, "You do this one yourself",
    [["Baldur's Gate 3", 84 * GB, "Baldur's Gate 3 · last played 6 months ago"]]),
  f("recycle-bin", "Recycle Bin (D:)", "Recycle Bin", "ask", 6.1 * GB, "31 files and folders you deleted earlier from this drive. They still use space until the bin is emptied.", "They're gone for good. Open the bin first if you might want something back.", "Empties the Recycle Bin on D: permanently", []),
];

export async function listDrives(): Promise<DriveInfo[]> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<DriveInfo[]>("list_drives");
  }
  return mockDrives;
}

export async function scan(drive: string, onProgress: (detail: string) => void): Promise<ScanResult> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const off = await listen<{ detail: string }>("progress", (e) => onProgress(e.payload.detail));
    try {
      return await invoke<ScanResult>("scan", { drive });
    } finally {
      off();
    }
  }
  const data = drive !== "C:";
  const steps = data
    ? ["Looking for old code projects…", "Looking for big old files…", "Checking Steam games…", "Measuring the Recycle Bin…"]
    : ["Checking known space hogs…", "Looking for old code projects…", "Looking through Downloads…", "Measuring the Recycle Bin…"];
  for (const d of steps) {
    onProgress(d);
    await sleep(500);
  }
  const d = mockDrives.find((x) => x.letter === drive) ?? mockDrives[0];
  return {
    disk: { mount: `${d.letter}\\`, total: d.total, free: d.free },
    findings: data ? mockDataFindings : mockFindings,
    activeProjects: data ? 3 : 27,
    staleProjects: data ? 14 : 192,
    tookMs: 2000,
  };
}

export async function clean(selections: Selection[]): Promise<CleanReport> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<CleanReport>("clean", { selections });
  }
  await sleep(1500);
  const rows = selections.flatMap((s) => {
    const x = [...mockFindings, ...mockDataFindings].find((m) => m.id === s.id);
    if (!x) return [];
    const bytes = s.paths ? x.items.filter((i) => s.paths!.includes(i.path)).reduce((t, i) => t + i.bytes, 0) : x.bytes;
    return [{ x, bytes }];
  });
  const freed = rows.reduce((t, r) => t + (r.x.recycles ? 0 : r.bytes), 0);
  return {
    before: mockDisk,
    after: { ...mockDisk, free: mockDisk.free + freed },
    outcomes: rows.map((r) => ({ id: r.x.id, name: r.x.name, ok: true, bytes: r.bytes, recycled: r.x.recycles, message: null })),
  };
}

export async function appVersion(): Promise<string> {
  if (inTauri) {
    const { getVersion } = await import("@tauri-apps/api/app");
    return getVersion();
  }
  return "0.2.1";
}

export async function getHistory(): Promise<HistoryEntry[]> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<HistoryEntry[]>("get_history", { limit: 50 });
  }
  const outcomes = [
    { name: "npm cache", ok: true, bytes: 2.9 * GB, recycled: false, message: null, paths: ["C:\\Users\\you\\AppData\\Local\\npm-cache"] },
    { name: "Gradle caches", ok: true, bytes: 16.3 * GB, recycled: false, message: null, paths: ["C:\\Users\\you\\.gradle\\caches"] },
    { name: "Disk images (ISO files)", ok: true, bytes: 6.6 * GB, recycled: true, message: null, paths: ["C:\\Users\\you\\Downloads\\Windows.iso"] },
    { name: "Docker unused data", ok: false, bytes: 0, recycled: false, message: "docker isn't running", paths: [] },
  ].map((o) => ({ ...o, bytes: Math.round(o.bytes) }));
  return [
    { when: "2026-10-06T20:15:30", drive: "C:", appVersion: "0.2.1", freeBefore: 56.6 * GB, freeAfter: 75.8 * GB, outcomes },
    { when: "2026-10-02T11:02:10", drive: "C:", appVersion: "0.1.0", freeBefore: 40 * GB, freeAfter: 41 * GB, outcomes: outcomes.slice(0, 1) },
  ];
}

export async function openHistoryFolder() {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_history_folder");
  }
}

/** The first line of release notes, without markdown: "**New: x**" -> "New: x". */
export function shortNotes(notes: string | null): string {
  const first = (notes ?? "").split("\n").map((l) => l.trim()).find((l) => l.length > 0) ?? "";
  const clean = first.replace(/^[#>\-*\s]+/, "").replace(/[*_`]/g, "").trim();
  return clean.length > 140 ? `${clean.slice(0, 137)}…` : clean;
}

/** A plain-text report of one cleanup, ready to paste into a bug report. */
export function historyReport(e: HistoryEntry): string {
  const lines = [
    `DiskBreeze ${e.appVersion} cleanup on ${e.drive} at ${e.when.replace("T", " ")}`,
    `Free space: ${fmtGB(e.freeBefore)} -> ${fmtGB(e.freeAfter)}`,
    ...e.outcomes.map((o) => `- ${o.ok ? "OK  " : "FAIL"} ${o.name}: ${fmt(o.bytes)}${o.recycled ? " (to Recycle Bin)" : ""}${o.message ? ` [${o.message}]` : ""}`),
  ];
  return lines.join("\n");
}

/** A plain-text report of all scan findings, formatted for sharing or reporting. */
export function scanReport(result: ScanResult): string {
  const { disk, findings, activeProjects, staleProjects } = result;
  const reclaimable = findings.filter((f) => f.tier !== "manual" && !f.estimate).reduce((s, f) => s + f.bytes, 0);
  const lines = [
    `DiskBreeze Scan Report — Drive ${disk.mount}`,
    `Total capacity: ${fmt(disk.total)} | Free space: ${fmt(disk.free)}`,
    `Reclaimable: ~${fmt(reclaimable)}`,
    staleProjects > 0 ? `Code projects: ${staleProjects} old build folders found (${activeProjects} active projects untouched)` : "",
    `Generated: ${new Date().toLocaleString()}`,
    "",
    "Findings:",
    ...findings.map((f) => {
      const sizeStr = f.estimate ? (f.bytes > 0 ? `up to ${fmt(f.bytes)}` : "varies") : fmt(f.bytes);
      const tag = f.tier === "safe" ? "[SAFE]" : f.tier === "ask" ? "[ASK]" : "[DIY]";
      return `${tag} ${f.name} (${f.category}): ${sizeStr} - ${f.what}`;
    }),
  ];
  return lines.filter(Boolean).join("\n");
}

let pendingUpdate: { downloadAndInstall: (cb: (ev: { event: string; data?: { contentLength?: number; chunkLength?: number } }) => void) => Promise<void> } | null = null;

/** Ask GitHub whether a newer version exists. Returns null when up to date. Never throws. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    if (!inTauri) {
      return new URLSearchParams(location.search).get("update")
        ? { version: "0.3.1", notes: "**Fixes and polish**\n- A tidier update banner.\n- Your history log now lives in the app's data folder.\n\n**Install:** run the setup file." }
        : null;
    }
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) return null;
    pendingUpdate = update;
    return { version: update.version, notes: update.body ?? null };
  } catch {
    return null; // offline or no release yet: stay quiet
  }
}

/** Download and install the update found by checkForUpdate, then restart. */
export async function installUpdate(onProgress: (pct: number | null) => void): Promise<void> {
  if (!inTauri || !pendingUpdate) {
    for (let i = 0; i <= 10; i++) { onProgress(i * 10); await sleep(150); }
    return;
  }
  let total = 0, done = 0;
  await pendingUpdate.downloadAndInstall((ev) => {
    if (ev.event === "Started") total = ev.data?.contentLength ?? 0;
    if (ev.event === "Progress") { done += ev.data?.chunkLength ?? 0; onProgress(total ? Math.min(100, Math.round((done / total) * 100)) : null); }
  });
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

export async function openTarget(target: string) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_target", { target });
  }
}

export async function reveal(path: string) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("reveal", { path });
  }
}

export async function savePoster(pngBase64: string): Promise<string> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("save_poster", { pngBase64 });
  }
  const a = document.createElement("a");
  a.href = `data:image/png;base64,${pngBase64}`;
  a.download = "disk-cleanup.png";
  a.click();
  return "your Downloads folder";
}

/** Split a size into number + unit, for big hero numbers: 26 MB -> {n:"26", unit:"MB"}. */
export function fmtParts(bytes: number): { n: string; unit: string } {
  if (bytes >= GB) return { n: (bytes / GB).toFixed(bytes >= 100 * GB ? 0 : 1), unit: "GB" };
  if (bytes >= MB) return { n: String(Math.round(bytes / MB)), unit: "MB" };
  return { n: String(Math.max(0, Math.round(bytes / 1024))), unit: "KB" };
}

/** Free-space readouts always show one decimal so small changes stay visible. */
export const fmtGB = (bytes: number) => `${(bytes / GB).toFixed(1)} GB`;

export function fmt(bytes: number): string {
  if (bytes >= GB) return `${(bytes / GB).toFixed(bytes >= 100 * GB ? 0 : 1)} GB`;
  if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
