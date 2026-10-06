import { useEffect, useMemo, useRef, useState } from "react";
import { toPng } from "html-to-image";
import { APP_NAME, Poster } from "./Poster";
import { clean, fmt, inTauri, openTarget, reveal, savePoster, scan } from "./api";
import type { CleanReport, Finding, ScanResult, Tier } from "./types";

type Phase = "idle" | "scanning" | "results" | "confirm" | "cleaning" | "done";

const TIERS: { tier: Tier; title: string; blurb: string }[] = [
  { tier: "safe", title: "Safe to clean", blurb: "Rebuilds itself. Nothing you made is touched." },
  { tier: "ask", title: "Your call", blurb: "Your own files or big downloads. Nothing here is ticked for you." },
  { tier: "manual", title: "Do it yourself", blurb: "These need another app or admin rights, so we only explain." },
];

export default function App() {
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

  // Browser-only preview helper (never runs inside the real app): ?demo=results|done
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

  return (
    <div className="app">
      <header className="bar">
        <div className="brand"><span className="dot" />{APP_NAME}</div>
        {phase === "results" && <button className="ghost" onClick={startScan}>Scan again</button>}
      </header>
      {error && <div className="error" role="alert">{error}</div>}

      {phase === "idle" && <Landing onScan={startScan} />}
      {phase === "scanning" && <Scanning detail={progress} />}
      {(phase === "results" || phase === "confirm") && result && (
        <Results result={result} picked={picked} setPicked={setPicked} />
      )}
      {(phase === "results" || phase === "confirm") && result && (
        <footer className="dock">
          <div>
            <b>{fmt(chosenBytes)}</b> selected
            <span className="muted"> · {chosen.length} {chosen.length === 1 ? "item" : "items"}</span>
          </div>
          <button className="primary" disabled={chosen.length === 0} onClick={() => setPhase("confirm")}>Clean selected</button>
        </footer>
      )}
      {phase === "confirm" && (
        <Confirm chosen={chosen} bytes={chosenBytes} onCancel={() => setPhase("results")} onGo={runClean} />
      )}
      {phase === "cleaning" && <Scanning detail="Cleaning… this can take a minute for big folders." />}
      {phase === "done" && report && <Done report={report} result={result!} onAgain={startScan} />}
    </div>
  );
}

function Landing({ onScan }: { onScan: () => void }) {
  return (
    <main className="center">
      <h1 className="hero-title">Find out what's <em>filling your disk.</em></h1>
      <p className="lead">
        {APP_NAME} looks for caches, old build folders, forgotten downloads and big games, explains each one in plain
        English, and only cleans what you tick. Scanning changes nothing.
      </p>
      <button className="primary big" onClick={onScan}>Scan my computer</button>
      <p className="muted small">Takes about a minute. Personal files go to the Recycle Bin, never straight to oblivion.</p>
    </main>
  );
}

function Scanning({ detail }: { detail: string }) {
  return (
    <main className="center">
      <div className="spinner" aria-hidden />
      <p className="lead">{detail || "Starting…"}</p>
    </main>
  );
}

function Results({ result, picked, setPicked }: { result: ScanResult; picked: Set<string>; setPicked: (s: Set<string>) => void }) {
  const { disk } = result;
  const usedPct = Math.round(((disk.total - disk.free) / disk.total) * 100);
  const reclaimable = result.findings.filter((f) => f.tier !== "manual").reduce((s, f) => s + f.bytes, 0);
  const toggle = (id: string) => {
    const next = new Set(picked);
    next.has(id) ? next.delete(id) : next.add(id);
    setPicked(next);
  };
  return (
    <main className="results">
      <section className="summary">
        <div>
          <div className="eyebrow">Drive {disk.mount.replace("\\", "")}</div>
          <div className="big-num">{fmt(disk.free)} <span>free of {fmt(disk.total)}</span></div>
          <div className="meter" role="img" aria-label={`${usedPct}% full`}>
            <i style={{ width: `${usedPct}%` }} />
          </div>
        </div>
        <div className="sum-note">
          Up to <b>{fmt(reclaimable)}</b> can be cleaned.
          {result.staleProjects > 0 && <> Found build files in <b>{result.staleProjects}</b> old projects; <b>{result.activeProjects}</b> active ones were left alone.</>}
        </div>
      </section>

      {TIERS.map(({ tier, title, blurb }) => {
        const list = result.findings.filter((f) => f.tier === tier);
        if (!list.length) return null;
        return (
          <section key={tier} className="group">
            <div className="group-head">
              <h2>{title}</h2>
              <span className="muted">{blurb}</span>
              <span className="group-total">{fmt(list.reduce((s, f) => s + f.bytes, 0))}</span>
            </div>
            {list.map((f) => <Card key={f.id} f={f} checked={picked.has(f.id)} onToggle={() => toggle(f.id)} />)}
          </section>
        );
      })}
    </main>
  );
}

function Card({ f, checked, onToggle }: { f: Finding; checked: boolean; onToggle: () => void }) {
  const [open, setOpen] = useState(false);
  const manual = f.tier === "manual";
  const hasItems = f.items.length > 0;
  return (
    <article className={`card ${checked ? "on" : ""}`}>
      <div className="card-row">
        {manual ? <span className="chk spacer" /> : (
          <input className="chk" type="checkbox" checked={checked} onChange={onToggle} aria-label={`Select ${f.name}`} />
        )}
        <div className="card-main">
          <div className="card-title">{f.name}<span className="cat">{f.category}</span></div>
          <p className="what">{f.what}</p>
          {f.after && !manual && <p className="after">After: {f.after}</p>}
          {manual && f.how && <p className="after">{f.how}</p>}
        </div>
        <div className="size">{fmt(f.bytes)}</div>
      </div>
      <div className="card-actions">
        <span className="muted small">{f.action}</span>
        {hasItems && <button className="link" onClick={() => setOpen(!open)}>{open ? "Hide" : "Show"} {f.items.length > 1 ? `${f.items.length} items` : "item"}</button>}
        {f.open && <button className="link" onClick={() => openTarget(f.open!)}>Open</button>}
      </div>
      {open && (
        <ul className="items">
          {f.items.slice(0, 40).map((i) => (
            <li key={i.path}>
              <div className="item-main">
                <div className="item-path" title={i.path}>{i.note?.split(" · ")[0] && f.id === "steam-games" ? i.note.split(" · ")[0] : i.path}</div>
                {i.note && <div className="muted small">{f.id === "steam-games" ? i.note.split(" · ")[1] : i.note}</div>}
              </div>
              <span className="item-size">{fmt(i.bytes)}</span>
              {i.open ? <button className="link" onClick={() => openTarget(i.open!)}>Uninstall…</button>
                : <button className="link" onClick={() => reveal(i.path)}>Show</button>}
            </li>
          ))}
          {f.items.length > 40 && <li className="muted small">…and {f.items.length - 40} more</li>}
        </ul>
      )}
    </article>
  );
}

function Confirm({ chosen, bytes, onCancel, onGo }: { chosen: Finding[]; bytes: number; onCancel: () => void; onGo: () => void }) {
  const permanent = chosen.some((f) => !f.recycles);
  const binSelected = chosen.some((f) => f.id === "recycle-bin");
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);
  return (
    <div className="scrim" role="dialog" aria-modal="true" aria-label="Confirm cleaning">
      <div className="modal">
        <h2>Clean {fmt(bytes)}?</h2>
        <ul className="plan">
          {chosen.map((f) => (
            <li key={f.id}><b>{f.name}</b><span>{fmt(f.bytes)}</span><em>{f.action}</em></li>
          ))}
        </ul>
        {permanent && <p className="warn">Caches and build folders are deleted for good. They rebuild themselves, but they won't be in the Recycle Bin.</p>}
        {binSelected && <p className="warn">Emptying the Recycle Bin is permanent. Open the bin first if you might want something back.</p>}
        <div className="modal-actions">
          <button className="ghost" onClick={onCancel}>Cancel</button>
          <button className="primary" onClick={onGo}>Yes, clean it</button>
        </div>
      </div>
    </div>
  );
}

function Done({ report, result, onAgain }: { report: CleanReport; result: ScanResult; onAgain: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const freed = Math.max(0, report.after.free - report.before.free);
  const recycled = report.outcomes.filter((o) => o.ok && o.recycled).reduce((s, o) => s + o.bytes, 0);
  const failed = report.outcomes.filter((o) => !o.ok);
  const name = (id: string) => result.findings.find((f) => f.id === id)?.name ?? id;

  async function save() {
    if (!ref.current) return;
    const url = await toPng(ref.current, { pixelRatio: 2, cacheBust: true });
    setSaved(await savePoster(url.split(",")[1]));
  }

  return (
    <main className="done">
      <div className="done-left">
        <div className="eyebrow">All done</div>
        <div className="big-num">{fmt(freed)} <span>freed</span></div>
        <p className="lead">{fmt(report.before.free)} free before. <b>{fmt(report.after.free)}</b> free now.</p>
        {recycled > 0 && (
          <p className="note">{fmt(recycled)} went to the Recycle Bin and still uses space. Empty the bin to get it back.</p>
        )}
        {failed.length > 0 && (
          <div className="fails">
            <b>Couldn't finish:</b>
            <ul>{failed.map((o) => <li key={o.id}>{name(o.id)}: {o.message}</li>)}</ul>
          </div>
        )}
        <div className="done-actions">
          <button className="primary" onClick={save}>Save share card</button>
          <button className="ghost" onClick={onAgain}>Scan again</button>
        </div>
        {saved && <p className="muted small">Saved to {saved}</p>}
      </div>
      <div className="poster-frame"><div className="poster-scale"><Poster ref={ref} report={report} /></div></div>
    </main>
  );
}
