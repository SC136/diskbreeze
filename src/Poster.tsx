import { forwardRef, type ReactNode } from "react";
import { fmt, fmtGB, fmtParts } from "./api";
import type { CleanReport } from "./types";

export const APP_NAME = "DiskBreeze";
const GB = 1024 ** 3;

export interface CleanedRow {
  name: string;
  bytes: number;
  recycled: boolean;
}

/** Bytes that were really freed (recycled items still use space until the bin is emptied). */
export function freedBytes(report: CleanReport): number {
  return report.outcomes.filter((o) => o.ok && !o.recycled).reduce((s, o) => s + o.bytes, 0);
}

/** The headline should match the achievement: don't gloat over 26 MB. */
function headline(freed: number): ReactNode {
  const gb = freed / GB;
  if (gb < 1) return <>A tidier drive. <span>Every bit counts.</span></>;
  if (gb < 20) return <>My drive just got <em>lighter.</em></>;
  return <>My drive can <em>breathe</em> again. <span>Finally.</span></>;
}

/** 1080×1350 share card. Rendered inside the Done screen and exported to PNG with html-to-image. */
export const Poster = forwardRef<HTMLDivElement, { report: CleanReport; rows: CleanedRow[] }>(({ report, rows }, ref) => {
  const { before, after } = report;
  const freed = freedBytes(report);
  const { n, unit } = fmtParts(freed);
  const usedPct = (d: typeof before) => Math.round(((d.total - d.free) / d.total) * 100);
  const circ = 2 * Math.PI * 84;
  const ring = (pct: number, color: string) => (
    <svg viewBox="0 0 200 200">
      <circle cx="100" cy="100" r="84" fill="none" stroke="rgba(255,255,255,.12)" strokeWidth="18" />
      <circle cx="100" cy="100" r="84" fill="none" stroke={color} strokeWidth="18" strokeLinecap="round"
        strokeDasharray={`${(circ * pct) / 100} ${circ}`} />
    </svg>
  );
  const gauges = [
    { label: "Before", disk: before, color: "#ff8a7a" },
    { label: "After", disk: after, color: "#4fd1a5" },
  ];
  const top = [...rows].sort((a, b) => b.bytes - a.bytes).slice(0, 4);
  const more = rows.length - top.length;
  return (
    <div className="poster" ref={ref}>
      <div className="p-glow p-g1" />
      <div className="p-glow p-g2" />
      <div className="p-grid" />
      <div className="p-wrap">
        <div className="p-top">
          <span className="p-pill">Disk cleanup</span>
          <span className="p-date">{new Date().toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}</span>
        </div>
        <h1 className="p-h1">{headline(freed)}</h1>
        <div className="p-hero">
          <div className="p-big">{n}</div>
          <div className="p-unit"><b>{unit}</b><span>of space freed up</span></div>
        </div>
        {top.length > 0 && (
          <ul className="p-list">
            {top.map((r) => (
              <li key={r.name}>
                <span>{r.name}{r.recycled && <i> · moved to Recycle Bin</i>}</span>
                <b>{fmt(r.bytes)}</b>
              </li>
            ))}
            {more > 0 && <li className="p-more"><span>+ {more} more</span></li>}
          </ul>
        )}
        <div className="p-gauges">
          {gauges.map(({ label, disk, color }) => (
            <div className="p-gauge" key={label}>
              <div className="p-ring">
                {ring(usedPct(disk), color)}
                <div className="p-ring-c"><b>{usedPct(disk)}%</b><span>full</span></div>
              </div>
              <div>
                <div className="p-k">{label}</div>
                <div className="p-v">{fmtGB(disk.free)}</div>
                <div className="p-s">free of {Math.round(disk.total / GB)} GB</div>
              </div>
            </div>
          ))}
        </div>
        <div className="p-foot">
          <span>Cleaned up with</span>
          <b><img src="/logo.svg" alt="" width={52} height={52} />{APP_NAME}</b>
        </div>
      </div>
    </div>
  );
});
