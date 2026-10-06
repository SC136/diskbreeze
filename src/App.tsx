import { useEffect, useMemo, useRef, useState } from "react";
import {
  Badge, Body1, Button, Caption1, Card, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
  DialogTitle, FluentProvider, LargeTitle, MessageBar, MessageBarBody, ProgressBar, Spinner, Subtitle1, Subtitle2,
  Text, Title2, Title3,
} from "@fluentui/react-components";
import { ArrowSync20Regular, Open16Regular, FolderOpen16Regular } from "@fluentui/react-icons";
import { toPng } from "html-to-image";
import { APP_NAME, Poster } from "./Poster";
import { clean, fmt, inTauri, openTarget, reveal, savePoster, scan } from "./api";
import { darkTheme, lightTheme } from "./theme";
import type { CleanReport, Finding, ScanResult, Tier } from "./types";

type Phase = "idle" | "scanning" | "results" | "confirm" | "cleaning" | "done";

const TIERS: { tier: Tier; title: string; blurb: string }[] = [
  { tier: "safe", title: "Safe to clean", blurb: "Rebuilds itself. Nothing you made is touched." },
  { tier: "ask", title: "Your call", blurb: "Your own files or big downloads. Nothing here is ticked for you." },
  { tier: "manual", title: "Do it yourself", blurb: "These need another app or admin rights, so we only explain." },
];

function useDarkMode() {
  const q = window.matchMedia("(prefers-color-scheme: dark)");
  // ?scheme=light|dark is a browser-only preview override
  const forced = new URLSearchParams(location.search).get("scheme");
  const [dark, setDark] = useState(forced ? forced === "dark" : q.matches);
  useEffect(() => {
    const on = (e: MediaQueryListEvent) => !forced && setDark(e.matches);
    q.addEventListener("change", on);
    return () => q.removeEventListener("change", on);
  }, [q]);
  return dark;
}

export default function App() {
  const dark = useDarkMode();
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState("");
  const [result, setResult] = useState<ScanResult | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [report, setReport] = useState<CleanReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function startScan() {
    setPhase("scanning");
    setError(null);
    try {
      const r = await scan(setProgress);
      setResult(r);
      setPicked(new Set(r.findings.filter((x) => x.tier === "safe").map((x) => x.id)));
      setPhase("results");
    } catch (e) {
      setError(String(e));
      setPhase("idle");
    }
  }

  async function runClean() {
    setPhase("cleaning");
    try {
      setReport(await clean([...picked]));
      setPhase("done");
    } catch (e) {
      setError(String(e));
      setPhase("results");
    }
  }

  // Browser-only preview helper (never runs inside the real app): ?demo=results|done|confirm
  useEffect(() => {
    const demo = new URLSearchParams(location.search).get("demo");
    if (inTauri || !demo) return;
    (async () => {
      const r = await scan(() => {});
      setResult(r);
      const safe = r.findings.filter((x) => x.tier === "safe").map((x) => x.id);
      setPicked(new Set(safe));
      if (demo === "done") {
        setReport(await clean([...safe, "downloads-disk-images"]));
        setPhase("done");
      } else {
        setPhase(demo === "confirm" ? "confirm" : "results");
      }
    })();
  }, []);

  const chosen = useMemo(() => result?.findings.filter((f) => picked.has(f.id)) ?? [], [result, picked]);
  const chosenBytes = chosen.reduce((s, f) => s + f.bytes, 0);
  const showResults = (phase === "results" || phase === "confirm") && result;

  return (
    <FluentProvider theme={dark ? darkTheme : lightTheme} className="shell">
      <div className="app">
        <header className="bar">
          <div className="brand"><span className="mark" aria-hidden />{APP_NAME}</div>
          {phase === "results" && (
            <Button appearance="subtle" icon={<ArrowSync20Regular />} onClick={startScan}>Scan again</Button>
          )}
        </header>
        {error && <MessageBar intent="error" className="gutter"><MessageBarBody>{error}</MessageBarBody></MessageBar>}

        {phase === "idle" && <Landing onScan={startScan} />}
        {phase === "scanning" && <Busy label={progress || "Starting…"} />}
        {phase === "cleaning" && <Busy label="Cleaning… this can take a minute for big folders." />}
        {showResults && <Results result={result} picked={picked} setPicked={setPicked} />}
        {showResults && (
          <footer className="dock">
            <Text size={400}>
              <b>{fmt(chosenBytes)}</b> selected
              <span className="muted"> · {chosen.length} {chosen.length === 1 ? "item" : "items"}</span>
            </Text>
            <Button appearance="primary" size="large" disabled={chosen.length === 0} onClick={() => setPhase("confirm")}>
              Clean selected
            </Button>
          </footer>
        )}
        {phase === "confirm" && (
          <Confirm chosen={chosen} bytes={chosenBytes} onCancel={() => setPhase("results")} onGo={runClean} />
        )}
        {phase === "done" && report && result && <Done report={report} result={result} onAgain={startScan} />}
      </div>
    </FluentProvider>
  );
}

function Landing({ onScan }: { onScan: () => void }) {
  return (
    <main className="center">
      <LargeTitle as="h1">Find out what's filling your disk</LargeTitle>
      <Body1 className="lead">
        {APP_NAME} looks for caches, old build folders, forgotten downloads and big games, explains each one in plain
        English, and only cleans what you tick. Scanning changes nothing.
      </Body1>
      <Button appearance="primary" size="large" onClick={onScan}>Scan my computer</Button>
      <Caption1 className="muted">Takes about a minute. Personal files go to the Recycle Bin first.</Caption1>
    </main>
  );
}

function Busy({ label }: { label: string }) {
  return (
    <main className="center">
      <Spinner size="large" label={label} />
    </main>
  );
}

function Results({ result, picked, setPicked }: { result: ScanResult; picked: Set<string>; setPicked: (s: Set<string>) => void }) {
  const { disk } = result;
  const usedPct = (disk.total - disk.free) / disk.total;
  const reclaimable = result.findings.filter((f) => f.tier !== "manual").reduce((s, f) => s + f.bytes, 0);
  const toggle = (id: string) => {
    const next = new Set(picked);
    next.has(id) ? next.delete(id) : next.add(id);
    setPicked(next);
  };
  return (
    <main className="results">
      <div className="summary">
        <div>
          <Caption1 className="muted">Drive {disk.mount.replace("\\", "")}</Caption1>
          <div className="free"><Title2 as="span">{fmt(disk.free)}</Title2><Body1 className="muted"> free of {fmt(disk.total)}</Body1></div>
          <ProgressBar value={usedPct} thickness="large" color={usedPct > 0.9 ? "error" : usedPct > 0.8 ? "warning" : "brand"} aria-label={`${Math.round(usedPct * 100)}% full`} />
        </div>
        <Body1 className="sum-note">
          Up to <b>{fmt(reclaimable)}</b> can be cleaned.
          {result.staleProjects > 0 && <> Found build files in <b>{result.staleProjects}</b> old projects; <b>{result.activeProjects}</b> active ones were left alone.</>}
        </Body1>
      </div>

      {TIERS.map(({ tier, title, blurb }) => {
        const list = result.findings.filter((f) => f.tier === tier);
        if (!list.length) return null;
        return (
          <section key={tier} className="group">
            <div className="group-head">
              <Subtitle1 as="h2">{title}</Subtitle1>
              <Caption1 className="muted">{blurb}</Caption1>
              <Subtitle2 className="group-total">{fmt(list.reduce((s, f) => s + f.bytes, 0))}</Subtitle2>
            </div>
            {list.map((f) => <FindingCard key={f.id} f={f} checked={picked.has(f.id)} onToggle={() => toggle(f.id)} />)}
          </section>
        );
      })}
    </main>
  );
}

function FindingCard({ f, checked, onToggle }: { f: Finding; checked: boolean; onToggle: () => void }) {
  const [open, setOpen] = useState(false);
  const manual = f.tier === "manual";
  const steam = f.id === "steam-games";
  return (
    <Card className={`finding ${checked ? "on" : ""}`} appearance="filled">
      <div className="finding-row">
        {manual ? <span className="chk-gap" /> : (
          <Checkbox checked={checked} onChange={onToggle} aria-label={`Select ${f.name}`} />
        )}
        <div className="finding-main">
          <div className="finding-title">
            <Text weight="semibold" size={400}>{f.name}</Text>
            <Badge appearance="tint" color="informative" size="small">{f.category}</Badge>
          </div>
          <Body1 className="what">{f.what}</Body1>
          {f.after && !manual && <Caption1 className="muted">After: {f.after}</Caption1>}
          {manual && f.how && <Caption1 className="muted">{f.how}</Caption1>}
        </div>
        <Title3 as="span" className="size">{fmt(f.bytes)}</Title3>
      </div>
      <div className="finding-actions">
        <Caption1 className="muted grow">{f.action}</Caption1>
        {f.items.length > 0 && (
          <Button appearance="transparent" size="small" onClick={() => setOpen(!open)}>
            {open ? "Hide" : "Show"} {f.items.length > 1 ? `${f.items.length} items` : "item"}
          </Button>
        )}
        {f.open && <Button appearance="transparent" size="small" icon={<Open16Regular />} onClick={() => openTarget(f.open!)}>Open</Button>}
      </div>
      {open && (
        <ul className="items">
          {f.items.slice(0, 40).map((i) => (
            <li key={i.path}>
              <div className="item-main">
                <div className="item-path" title={i.path}>{steam ? i.note?.split(" · ")[0] : i.path}</div>
                {i.note && <Caption1 className="muted">{steam ? i.note.split(" · ")[1] : i.note}</Caption1>}
              </div>
              <Text weight="semibold" size={200}>{fmt(i.bytes)}</Text>
              {i.open
                ? <Button appearance="transparent" size="small" icon={<Open16Regular />} onClick={() => openTarget(i.open!)}>Uninstall…</Button>
                : <Button appearance="transparent" size="small" icon={<FolderOpen16Regular />} onClick={() => reveal(i.path)}>Show</Button>}
            </li>
          ))}
          {f.items.length > 40 && <li><Caption1 className="muted">…and {f.items.length - 40} more</Caption1></li>}
        </ul>
      )}
    </Card>
  );
}

function Confirm({ chosen, bytes, onCancel, onGo }: { chosen: Finding[]; bytes: number; onCancel: () => void; onGo: () => void }) {
  const permanent = chosen.some((f) => !f.recycles);
  const binSelected = chosen.some((f) => f.id === "recycle-bin");
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onCancel()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>Clean {fmt(bytes)}?</DialogTitle>
          <DialogContent>
            <ul className="plan">
              {chosen.map((f) => (
                <li key={f.id}><b>{f.name}</b><span>{fmt(f.bytes)}</span><em>{f.action}</em></li>
              ))}
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
  const freed = Math.max(0, report.after.free - report.before.free);
  const recycled = report.outcomes.filter((o) => o.ok && o.recycled).reduce((s, o) => s + o.bytes, 0);
  const failed = report.outcomes.filter((o) => !o.ok);
  const name = (id: string) => result.findings.find((f) => f.id === id)?.name ?? id;

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
        <Caption1 className="muted">All done</Caption1>
        <div className="free"><LargeTitle as="span">{fmt(freed)}</LargeTitle><Body1 className="muted"> freed</Body1></div>
        <Body1>{fmt(report.before.free)} free before. <b>{fmt(report.after.free)}</b> free now.</Body1>
        {recycled > 0 && (
          <MessageBar intent="info"><MessageBarBody>
            {fmt(recycled)} went to the Recycle Bin and still uses space. Empty the bin to get it back.
          </MessageBarBody></MessageBar>
        )}
        {failed.length > 0 && (
          <MessageBar intent="error"><MessageBarBody>
            Couldn't finish: {failed.map((o) => `${name(o.id)} (${o.message})`).join("; ")}
          </MessageBarBody></MessageBar>
        )}
        <div className="done-actions">
          <Button appearance="primary" size="large" onClick={save}>Save share card</Button>
          <Button appearance="secondary" size="large" onClick={onAgain}>Scan again</Button>
        </div>
        {saved && <Caption1 className="muted">Saved to {saved}</Caption1>}
        {saveError && <Caption1 className="err">{saveError}</Caption1>}
      </div>
      <div className="poster-frame"><div className="poster-scale"><Poster ref={ref} report={report} /></div></div>
    </main>
  );
}
