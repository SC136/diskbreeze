# Contributing to DiskBreeze

Thank you for your interest in contributing to DiskBreeze!

DiskBreeze helps people understand what is filling up their disk and clean only what they choose. We prioritize safety, clear explanations in plain English, and a native Windows 11 Fluent experience.

---

## Ways to Contribute

1. [Add a new cleanup target to the catalog](#1-contributing-a-catalog-entry) (Easiest — no Rust required)
2. [Improve the frontend UI & UX](#2-frontend-development) (React 19, TypeScript, Fluent UI)
3. [Add or improve native detectors](#3-backend-development) (Rust, Tauri v2)
4. [Report bugs or suggest features](#4-reporting-issues)

---

## 1. Contributing a Catalog Entry

Most space hogs are discovered via rules in [`catalog/windows.toml`](catalog/windows.toml). Adding an entry is declarative and simple.

### Schema Reference

```toml
[[item]]
id = "my-tool-cache"             # Unique kebab-case identifier
name = "My Tool Cache"           # Clear, user-facing display name
category = "Developer caches"    # Category (e.g. Developer caches, AI models, Browsers & apps, Games, Windows)
tier = "safe"                    # safe | ask | manual
paths = ['%LOCALAPPDATA%\MyTool\Cache'] # Paths to scan (supports %ENV%, {home}, wildcards)
what = "One plain sentence explaining what this folder is."
after = "What the user will notice after cleaning."
action = "contents"              # delete | contents | recycle | command | manual | admin
min_mb = 100                     # Optional: minimum MB required to display (default: 50)
```

### Fields and Rules

| Field | Type | Description |
| :--- | :--- | :--- |
| `id` | string | Unique lowercase identifier (e.g., `unity-cache`, `vcpkg-archives`). |
| `name` | string | Human-readable name. |
| `category` | string | Section heading: `Developer caches`, `AI models`, `Browsers & apps`, `Games`, or `Windows`. |
| `tier` | string | `safe` (rebuilds itself, pre-selected), `ask` (user files or big re-download, never pre-selected), or `manual` (requires another app or external steps). |
| `paths` | array | Path templates to check. Supports `%ENV%` variables (e.g., `%LOCALAPPDATA%`, `%APPDATA%`, `%TEMP%`), `{home}`, `{downloads}`, `{videos}`, `{pictures}`, `{steam}`, and `*` / `?` wildcards inside folder names. |
| `what` | string | Concise explanation in plain English. Avoid developer jargon where possible. |
| `after` | string | Honest description of consequences (e.g., "The next build downloads packages again."). |
| `action` | string | One of `delete`, `contents` (empty folder but keep directory), `recycle`, `command`, `manual`, `admin`. |
| `command` | string | For `action = "command"`, the CLI command to invoke (e.g., `npm cache clean --force`). |
| `fallback` | string | For `action = "command"`, fallback behavior if command fails (`contents` or `delete`). |
| `how` | string | Required for `tier = "manual"`, explaining how the user can clean it themselves. |

### Safety Golden Rules
- **Rule of thumb:** If you aren't 100% sure an item regenerates safely, use `tier = "ask"`, never `safe`.
- **Recycle personal files:** Anything created by the user or personal data must use `action = "recycle"`. Recycled items cannot be marked `tier = "safe"`.
- **Protected paths:** Never point to shallow system directories, drive roots, `C:\Windows`, or user root profile directories. All paths are validated against `paths::is_protected`.
- **Admin scripts:** Scripts requiring administrator privileges must be approved and reviewed.

---

## 2. Frontend Development

The frontend is built with:
- **React 19**
- **TypeScript**
- **Microsoft Fluent UI (Fluent 2)** via `@fluentui/react-components`
- **Vite**

### Setup & Local Preview
```bash
npm install
npm run dev
```
Open [http://localhost:1420/?demo=results](http://localhost:1420/?demo=results) or `?demo=done` to view mock data without needing the native Rust backend running.

### Build & Typecheck
```bash
npm run build
```

---

## 3. Backend Development

The desktop backend is built with Tauri v2 and Rust.

### Prerequisites
- Node.js (v18+)
- Rust toolchain (`rustup`)
- Windows WebView2 (included by default on Windows 11)

### Running & Testing
```bash
# Run backend unit tests and catalog validations:
cd src-tauri && cargo test

# Run the complete desktop app:
npm run tauri dev

# Run read-only scan outputting JSON (useful for debugging):
cd src-tauri && cargo run -- --scan-json
```

---

## 4. Pull Request Checklist

Before submitting a pull request:
- [ ] Catalog entries parse and adhere to all safety rules (`cargo test`).
- [ ] Descriptions in `what` and `after` are written in plain, friendly English.
- [ ] Frontend changes build cleanly without TypeScript errors (`npm run build`).
- [ ] No extraneous dependencies or tracking code added.
