import type { CleanReport, DriveInfo, Finding, ScanResult, Selection } from "./types";

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
    outcomes: rows.map((r) => ({ id: r.x.id, ok: true, bytes: r.bytes, recycled: r.x.recycles, message: null })),
  };
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
