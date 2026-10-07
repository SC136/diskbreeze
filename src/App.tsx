import { useEffect, useMemo, useRef, useState } from "react";
import {
  Badge, Body1, Button, Caption1, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
  DialogTitle, Dropdown, FluentProvider, LargeTitle, MessageBar, MessageBarBody, Option, Spinner, Subtitle2, Tab,
  TabList, Text,
} from "@fluentui/react-components";
import {
  ArrowSync20Regular, CheckmarkCircle20Filled, ChevronDown20Regular, DismissCircle20Filled, ChevronUp20Regular, FolderOpen16Regular, Open16Regular,
} from "@fluentui/react-icons";
import { toPng } from "html-to-image";
import { APP_NAME, freedBytes, Poster, type CleanedRow } from "./Poster";
import { clean, fmt, fmtGB, fmtParts, inTauri, listDrives, openTarget, reveal, savePoster, scan } from "./api";
import { darkTheme, lightTheme } from "./theme";
import type { CleanReport, DriveInfo, Finding, ScanResult, Selection, Tier } from "./types";

type Phase = "idle" | "scanning" | "results" | "confirm" | "cleaning" | "done";
/** Items the user un-ticked inside a ticked finding, by finding id. */
type Excluded = Record<string, Set<string>>;

const TIERS: { tier: Tier; title: string; blurb: string }[] = [
  { tier: "safe", title: "Safe to clean", blurb: "Rebuilds itself. Nothing you made is touched." },
  { tier: "ask", title: "Your call", blurb: "Your own files or big downloads. Nothing here is ticked for you." },
  { tier: "manual", title: "Do it yourself", blurb: "These need another app or admin rights, so we only explain." },
];

const selBytes = (f: Finding, ex: Excluded) => {
  const e = ex[f.id];
  if (!f.selectable || !e || e.size === 0) return f.bytes;
  return f.items.filter((i) => !e.has(i.path)).reduce((s, i) => s + i.bytes, 0);
};
const selCount = (f: Finding, ex: Excluded) => (f.selectable ? f.items.length - (ex[f.id]?.size ?? 0) : f.items.length);

function useDarkMode() {
  const q = window.matchMedia("(prefers-color-scheme: dark)");
  // ?scheme=light|dark is a browser-only preview override
  const forced = new URLSearchParams(location.search).get("scheme");
  const [dark, setDark] = useState(forced ? forced === "dark" : q.matches);
  useEffect(() => {
    const on = (e: MediaQueryListEvent) => !forced && setDark(e.matches);
    q.addEventListener("change", on);
    return () => q.removeEventListener("change", on);
  }, [q, forced]);
  return dark;
}

export default function App() {
  const dark = useDarkMode();
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState("");
  const [result, setResult] = useState<ScanResult | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [excluded, setExcluded] = useState<Excluded>({});
  const [report, setReport] = useState<CleanReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [drive, setDrive] = useState<string>("C:");

  // Offer every local drive; default to the one holding the user's profile.
  useEffect(() => {
    listDrives()
      .then((list) => {
        setDrives(list);
        const wanted = !inTauri ? new URLSearchParams(location.search).get("drive") : null;
        setDrive(list.find((d) => d.letter === wanted)?.letter ?? list.find((d) => d.hasProfile)?.letter ?? list[0]?.letter ?? "C:");
      })
      .catch((e) => setError(String(e)));
  }, []);

  const safeIds = (r: ScanResult) => new Set(r.findings.filter((x) => x.tier === "safe").map((x) => x.id));

  async function startScan(d: string = drive) {
    setDrive(d);
    setPhase("scanning");
    setError(null);
    try {
      const r = await scan(d, setProgress);
      setResult(r);
      setChecked(safeIds(r));
      setExcluded({});
      setPhase("results");
    } catch (e) {
      setError(String(e));
      setPhase("idle");
    }
  }

  const chosen = useMemo(() => result?.findings.filter((f) => checked.has(f.id)) ?? [], [result, checked]);
  const chosenBytes = chosen.reduce((s, f) => s + selBytes(f, excluded), 0);

  function selections(): Selection[] {
    return chosen.map((f) => {
      const ex = excluded[f.id];
      return f.selectable && ex && ex.size > 0
        ? { id: f.id, paths: f.items.filter((i) => !ex.has(i.path)).map((i) => i.path) }
        : { id: f.id };
    });
  }

  async function runClean() {
    setPhase("cleaning");
    try {
      setReport(await clean(selections()));
      setPhase("done");
    } catch (e) {
      setError(String(e));
      setPhase("results");
    }
  }

  function toggleFinding(f: Finding) {
    const next = new Set(checked);
    next.has(f.id) ? next.delete(f.id) : next.add(f.id);
    setChecked(next);
    setExcluded({ ...excluded, [f.id]: new Set() });
  }

  function toggleItem(f: Finding, path: string) {
    const ex = new Set(excluded[f.id] ?? []);
    const next = new Set(checked);
    if (!next.has(f.id)) {
      // Ticking one item of an unticked finding selects just that item.
      f.items.forEach((i) => i.path !== path && ex.add(i.path));
      next.add(f.id);
    } else {
      ex.has(path) ? ex.delete(path) : ex.add(path);
      if (ex.size === f.items.length) {
        next.delete(f.id);
        ex.clear();
      }
    }
    setChecked(next);
    setExcluded({ ...excluded, [f.id]: ex });
  }

  function setTier(tier: Tier, on: boolean) {
    if (!result) return;
    const next = new Set(checked);
    result.findings.filter((f) => f.tier === tier).forEach((f) => (on ? next.add(f.id) : next.delete(f.id)));
    setChecked(next);
    setExcluded((ex) => {
      const copy = { ...ex };
      result.findings.filter((f) => f.tier === tier).forEach((f) => delete copy[f.id]);
      return copy;
    });
  }

  // Browser-only preview helper (never runs inside the real app): ?demo=results|done|confirm
  useEffect(() => {
    const demo = new URLSearchParams(location.search).get("demo");
    if (inTauri || !demo) return;
    (async () => {
      const wanted = new URLSearchParams(location.search).get("drive") ?? "C:";
      setDrive(wanted);
      const r = await scan(wanted, () => {});
      setResult(r);
      setChecked(safeIds(r));
      if (demo === "done" || demo === "small") {
        const ids = demo === "small" ? ["temp-files"] : [...safeIds(r)];
        setReport(await clean(ids.map((id) => ({ id }))));
        setPhase("done");
      } else {
        setPhase(demo === "confirm" ? "confirm" : "results");
      }
    })();
  }, []);

  const showResults = (phase === "results" || phase === "confirm") && result;

  return (
    <FluentProvider theme={dark ? darkTheme : lightTheme} className="shell">
      <div className="app">
        <header className="bar">
          <div className="brand"><img className="mark" src="/logo.svg" alt="" width={24} height={24} />{APP_NAME}</div>
          {phase === "results" && (
            <div className="bar-actions">
              {drives.length > 1 && <DriveSwitcher drives={drives} value={drive} onPick={(d) => startScan(d)} />}
              <Button appearance="subtle" icon={<ArrowSync20Regular />} onClick={() => startScan()}>Scan again</Button>
            </div>
          )}
        </header>
        {error && <MessageBar intent="error" className="gutter"><MessageBarBody>{error}</MessageBarBody></MessageBar>}

        {phase === "idle" && <Landing drives={drives} drive={drive} onPick={setDrive} onScan={() => startScan()} />}
        {phase === "scanning" && <Busy label={progress || "Starting…"} />}
        {phase === "cleaning" && <Busy label="Cleaning… this can take a minute for big folders." />}
        {showResults && (
          <Results
            result={result} checked={checked} excluded={excluded}
            onFinding={toggleFinding} onItem={toggleItem} onTier={setTier}
          />
        )}
        {showResults && (
          <footer className="dock">
            <div className="dock-sum">
              <span className="dock-num">{fmt(chosenBytes)}</span>
              <Caption1 className="muted">selected · {chosen.length} {chosen.length === 1 ? "category" : "categories"}</Caption1>
            </div>
            <div className="dock-actions">
              <Button appearance="subtle" disabled={chosen.length === 0} onClick={() => { setChecked(new Set()); setExcluded({}); }}>Clear</Button>
              <Button appearance="primary" size="large" disabled={chosen.length === 0} onClick={() => setPhase("confirm")}>
                Clean selected
              </Button>
            </div>
          </footer>
        )}
        {phase === "confirm" && (
          <Confirm chosen={chosen} excluded={excluded} bytes={chosenBytes} onCancel={() => setPhase("results")} onGo={runClean} />
        )}
        {phase === "done" && report && result && <Done report={report} result={result} onAgain={() => startScan()} />}
      </div>
    </FluentProvider>
  );
}

interface LandingProps { drives: DriveInfo[]; drive: string; onPick: (d: string) => void; onScan: () => void }

function Landing({ drives, drive, onPick, onScan }: LandingProps) {
  const picked = drives.find((d) => d.letter === drive);
  return (
    <main className="center">
      <img src="/logo.svg" alt="" width={84} height={84} />
      <LargeTitle as="h1">Find out what's filling your disk</LargeTitle>
      <Body1 className="lead">
        {APP_NAME} looks for caches, old build folders, forgotten downloads and big games, explains each one in plain
        English, and only cleans what you tick. Scanning changes nothing.
      </Body1>

      {drives.length > 1 && (
        <div className="drives" role="radiogroup" aria-label="Drive to scan">
          {drives.map((d) => {
            const used = (d.total - d.free) / d.total;
            const on = d.letter === drive;
            return (
              <button key={d.letter} type="button" role="radio" aria-checked={on} className={`drive ${on ? "on" : ""}`} onClick={() => onPick(d.letter)}>
                <div className="drive-top">
                  <span className="drive-letter">{d.letter}</span>
                  <span className="drive-label">{d.label || "Local disk"}</span>
                  {d.hasProfile && <Badge appearance="tint" color="brand" size="small">Windows</Badge>}
                  {d.removable && <Badge appearance="tint" color="informative" size="small">Removable</Badge>}
                </div>
                <div className="bar-track"><div className="bar-fill" style={{ width: `${Math.round(used * 100)}%` }} /></div>
                <Caption1 className="muted">{fmt(d.free)} free of {fmt(d.total)}</Caption1>
              </button>
            );
          })}
        </div>
      )}

      <Button appearance="primary" size="large" onClick={onScan}>
        {drives.length > 1 ? `Scan drive ${drive}` : "Scan my computer"}
      </Button>
      <Caption1 className="muted lead">
        {picked && !picked.hasProfile
          ? "Looks for old code projects, big old files, Steam games and this drive's Recycle Bin. App caches and Downloads live on your Windows drive."
          : "Takes about a minute. Personal files go to the Recycle Bin first."}
      </Caption1>
    </main>
  );
}

function DriveSwitcher({ drives, value, onPick }: { drives: DriveInfo[]; value: string; onPick: (d: string) => void }) {
  const label = (d: DriveInfo) => `${d.letter} ${d.label || "Local disk"} · ${fmt(d.free)} free`;
  return (
    <Dropdown
      className="drive-switch" aria-label="Drive" size="small"
      value={label(drives.find((d) => d.letter === value) ?? drives[0])}
      selectedOptions={[value]}
      onOptionSelect={(_, d) => d.optionValue && d.optionValue !== value && onPick(d.optionValue)}
    >
      {drives.map((d) => <Option key={d.letter} value={d.letter} text={label(d)}>{label(d)}</Option>)}
    </Dropdown>
  );
}

function Busy({ label }: { label: string }) {
  return (
    <main className="center">
      <Spinner size="large" label={label} />
    </main>
  );
}

function Ring({ pct }: { pct: number }) {
  const r = 42, c = 2 * Math.PI * r;
  const color = pct > 0.9 ? "var(--colorPaletteRedForeground1)" : pct > 0.8 ? "var(--colorPaletteDarkOrangeForeground1)" : "var(--colorBrandForeground1)";
  return (
    <div className="ring" role="img" aria-label={`${Math.round(pct * 100)}% full`}>
      <svg viewBox="0 0 100 100">
        <circle cx="50" cy="50" r={r} fill="none" stroke="var(--colorNeutralStroke2)" strokeWidth="11" />
        <circle cx="50" cy="50" r={r} fill="none" stroke={color} strokeWidth="11" strokeLinecap="round"
          strokeDasharray={`${c * pct} ${c}`} transform="rotate(-90 50 50)" />
      </svg>
      <div className="ring-c"><b>{Math.round(pct * 100)}%</b><span>full</span></div>
    </div>
  );
}

interface ResultsProps {
  result: ScanResult; checked: Set<string>; excluded: Excluded;
  onFinding: (f: Finding) => void; onItem: (f: Finding, path: string) => void; onTier: (t: Tier, on: boolean) => void;
}

function Results({ result, checked, excluded, onFinding, onItem, onTier }: ResultsProps) {
  const { disk } = result;
  const used = (disk.total - disk.free) / disk.total;
  const reclaimable = result.findings.filter((f) => f.tier !== "manual").reduce((s, f) => s + f.bytes, 0);
  const tiers = TIERS.filter((t) => result.findings.some((f) => f.tier === t.tier));
  // ?tab= and ?open= are browser-only preview helpers
  const preview = new URLSearchParams(location.search);
  const [tab, setTab] = useState<Tier>((!inTauri && (preview.get("tab") as Tier)) || tiers[0]?.tier || "safe");
  const [open, setOpen] = useState<string | null>(!inTauri ? preview.get("open") : null);
  const list = result.findings.filter((f) => f.tier === tab);
  const max = Math.max(...list.map((f) => f.bytes), 1);
  const current = TIERS.find((t) => t.tier === tab)!;
  const allOn = list.length > 0 && list.every((f) => checked.has(f.id));

  return (
    <main className="results">
      <section className="hero">
        <Ring pct={used} />
        <div className="hero-text">
          <Caption1 className="muted">Drive {disk.mount.replace("\\", "")}</Caption1>
          <div><span className="hero-free">{fmt(disk.free)}</span><Body1 className="muted"> free of {fmt(disk.total)}</Body1></div>
          <Body1>
            Up to <b>{fmt(reclaimable)}</b> can be cleaned.
            {result.staleProjects > 0 && <> Build files in <b>{result.staleProjects}</b> old projects were found; <b>{result.activeProjects}</b> active ones were left alone.</>}
          </Body1>
        </div>
      </section>

      <TabList selectedValue={tab} onTabSelect={(_, d) => { setTab(d.value as Tier); setOpen(null); }} size="large" className="tabs">
        {tiers.map((t) => {
          const items = result.findings.filter((f) => f.tier === t.tier);
          return (
            <Tab key={t.tier} value={t.tier}>
              {t.title}
              <span className="tab-meta">{fmt(items.reduce((s, f) => s + f.bytes, 0))}</span>
            </Tab>
          );
        })}
      </TabList>

      <div className="toolbar">
        <Caption1 className="muted">{current.blurb}</Caption1>
        {tab !== "manual" && (
          <Button appearance="transparent" size="small" onClick={() => onTier(tab, !allOn)}>
            {allOn ? "Select none" : "Select all"}
          </Button>
        )}
      </div>

      <div className="rows">
        {list.map((f) => (
          <Row
            key={f.id} f={f} max={max} ex={excluded} on={checked.has(f.id)} open={open === f.id}
            onOpen={() => setOpen(open === f.id ? null : f.id)}
            onToggle={() => onFinding(f)} onItem={(p) => onItem(f, p)}
          />
        ))}
      </div>
    </main>
  );
}

interface RowProps {
  f: Finding; max: number; ex: Excluded; on: boolean; open: boolean;
  onOpen: () => void; onToggle: () => void; onItem: (path: string) => void;
}

function Row({ f, max, ex, on, open, onOpen, onToggle, onItem }: RowProps) {
  const manual = f.tier === "manual";
  const steam = f.id === "steam-games";
  const excl = ex[f.id];
  const partial = on && f.selectable && !!excl && excl.size > 0;
  const shown = partial ? selBytes(f, ex) : f.bytes;
  return (
    <article className={`row ${on ? "on" : ""} ${open ? "open" : ""}`}>
      <div className="row-head">
        {manual ? <span className="chk-gap" /> : (
          <Checkbox checked={partial ? "mixed" : on} onChange={onToggle} aria-label={`Select ${f.name}`} />
        )}
        <div className="row-click" onClick={onOpen}>
          <div className="row-main">
            <div className="row-title">
              <Text weight="semibold" size={400}>{f.name}</Text>
              <Badge appearance="tint" color="informative" size="small">{f.category}</Badge>
              {partial && <Badge appearance="tint" color="brand" size="small">{selCount(f, ex)} of {f.items.length}</Badge>}
            </div>
            {!open && <Body1 className="row-what">{f.what}</Body1>}
          </div>
          <div className="row-size">
            <Text weight="semibold" size={500}>{fmt(shown)}</Text>
            <div className="bar-track"><div className="bar-fill" style={{ width: `${Math.max(2, (shown / max) * 100)}%` }} /></div>
          </div>
        </div>
        <Button appearance="subtle" size="small" aria-expanded={open} aria-label={open ? "Hide details" : "Show details"}
          icon={open ? <ChevronUp20Regular /> : <ChevronDown20Regular />} onClick={onOpen} />
      </div>

      {open && (
        <div className="row-body">
          <Body1>{f.what}</Body1>
          {f.after && !manual && <Caption1 className="muted">After: {f.after}</Caption1>}
          {manual && f.how && <Caption1 className="muted">{f.how}</Caption1>}
          <div className="row-meta">
            <Caption1 className="muted">{f.action}</Caption1>
            {f.open && <Button appearance="transparent" size="small" icon={<Open16Regular />} onClick={() => openTarget(f.open!)}>Open</Button>}
          </div>
          {f.items.length > 0 && (
            <ul className="items">
              {f.items.slice(0, 200).map((i) => {
                const name = steam ? i.note?.split(" · ")[0] ?? i.path : i.path.split("\\").pop() ?? i.path;
                const sub = steam ? i.note?.split(" · ")[1] : [i.path.slice(0, i.path.length - name.length - 1), i.note].filter(Boolean).join(" · ");
                return (
                  <li key={i.path}>
                    {f.selectable
                      ? <Checkbox checked={on && !excl?.has(i.path)} onChange={() => onItem(i.path)} aria-label={`Select ${name}`} />
                      : <span className="chk-gap" />}
                    <div className="item-main" title={i.path}>
                      <Text size={300} weight="semibold" className="item-name">{name}</Text>
                      {sub && <Caption1 className="muted item-sub">{sub}</Caption1>}
                    </div>
                    <Text size={200} weight="semibold" className="item-size">{fmt(i.bytes)}</Text>
                    {i.open
                      ? <Button appearance="transparent" size="small" icon={<Open16Regular />} onClick={() => openTarget(i.open!)}>Uninstall…</Button>
                      : <Button appearance="transparent" size="small" icon={<FolderOpen16Regular />} onClick={() => reveal(i.path)}>Show</Button>}
                  </li>
                );
              })}
              {f.items.length > 200 && <li className="more"><Caption1 className="muted">…and {f.items.length - 200} more, included with the selection above</Caption1></li>}
            </ul>
          )}
        </div>
      )}
    </article>
  );
}

function Confirm({ chosen, excluded, bytes, onCancel, onGo }: { chosen: Finding[]; excluded: Excluded; bytes: number; onCancel: () => void; onGo: () => void }) {
  const permanent = chosen.some((f) => !f.recycles);
  const binSelected = chosen.some((f) => f.id === "recycle-bin");
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onCancel()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>Clean {fmt(bytes)}?</DialogTitle>
          <DialogContent>
            <ul className="plan">
              {chosen.map((f) => {
                const partial = f.selectable && (excluded[f.id]?.size ?? 0) > 0;
                return (
                  <li key={f.id}>
                    <b>{f.name}{partial && <span className="muted"> · {selCount(f, excluded)} of {f.items.length} items</span>}</b>
                    <span>{fmt(selBytes(f, excluded))}</span>
                    <em>{f.action}</em>
                  </li>
                );
              })}
            </ul>
            {permanent && (
              <MessageBar intent="warning" className="gap-top"><MessageBarBody>
                Caches and build folders are deleted for good. They rebuild themselves, but they won't be in the Recycle Bin.
              </MessageBarBody></MessageBar>
            )}
            {binSelected && (
              <MessageBar intent="warning" className="gap-top"><MessageBarBody>
                Emptying the Recycle Bin is permanent. Open the bin first if you might want something back.
              </MessageBarBody></MessageBar>
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" onClick={onCancel}>Cancel</Button>
            <Button appearance="primary" onClick={onGo}>Yes, clean it</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function Done({ report, result, onAgain }: { report: CleanReport; result: ScanResult; onAgain: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const freed = freedBytes(report);
  const { n, unit } = fmtParts(freed);
  const name = (id: string) => result.findings.find((f) => f.id === id)?.name ?? id;
  const done = report.outcomes.filter((o) => o.ok);
  const failed = report.outcomes.filter((o) => !o.ok);
  const recycled = done.filter((o) => o.recycled).reduce((s, o) => s + o.bytes, 0);
  const rows: CleanedRow[] = done.filter((o) => o.bytes > 0).map((o) => ({ name: name(o.id), bytes: o.bytes, recycled: o.recycled }));

  async function save() {
    if (!ref.current) return;
    setSaveError(null);
    try {
      const url = await toPng(ref.current, { pixelRatio: 2, cacheBust: true });
      setSaved(await savePoster(url.split(",")[1]));
    } catch (e) {
      setSaveError(`Couldn't save the card: ${e}`);
    }
  }

  return (
    <main className="done">
      <div className="done-left">
        <Caption1 className="muted">{failed.length > 0 ? "Finished with some problems" : "All done"}</Caption1>
        <div className="free"><LargeTitle as="span">{n} {unit}</LargeTitle><Body1 className="muted"> freed</Body1></div>
        <div className="stat-row">
          <div className="stat"><Caption1 className="muted">Free before</Caption1><b>{fmtGB(report.before.free)}</b></div>
          <div className="stat"><Caption1 className="muted">Free now</Caption1><b>{fmtGB(report.after.free)}</b></div>
          <div className="stat"><Caption1 className="muted">Cleaned</Caption1><b>{done.length} of {report.outcomes.length}</b></div>
        </div>
        {recycled > 0 && (
          <MessageBar intent="info"><MessageBarBody>
            {fmt(recycled)} went to the Recycle Bin and still uses space. Empty the bin to get it back.
          </MessageBarBody></MessageBar>
        )}
        {failed.length > 0 && (
          <MessageBar intent="warning"><MessageBarBody>
            {failed.length} {failed.length === 1 ? "item" : "items"} couldn't be cleaned. Files in use are skipped, so close the app and scan again.
          </MessageBarBody></MessageBar>
        )}

        <Subtitle2 as="h2">What happened</Subtitle2>
        <ul className="cleaned">
          {report.outcomes.map((o) => (
            <li key={o.id}>
              <span className={o.ok ? "ok" : "bad"}>{o.ok ? <CheckmarkCircle20Filled /> : <DismissCircle20Filled />}</span>
              <div className="c-name">
                <Text weight="semibold">{name(o.id)}</Text>
                {!o.ok && <Caption1 className="err">{o.message}</Caption1>}
                {o.ok && o.recycled && <Caption1 className="muted">Moved to the Recycle Bin</Caption1>}
              </div>
              {o.ok && <Text weight="semibold">{fmt(o.bytes)}</Text>}
            </li>
          ))}
        </ul>

        <div className="done-actions">
          <Button appearance="primary" size="large" onClick={save}>Save share card</Button>
          <Button appearance="secondary" size="large" onClick={onAgain}>Scan again</Button>
        </div>
        {saved && <Caption1 className="muted">Saved to {saved}</Caption1>}
        {saveError && <Caption1 className="err">{saveError}</Caption1>}
      </div>
      <div className="poster-frame"><div className="poster-scale"><Poster ref={ref} report={report} rows={rows} /></div></div>
    </main>
  );
}
