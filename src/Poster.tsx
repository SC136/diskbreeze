import { forwardRef } from "react";
import type { CleanReport } from "./types";

export const APP_NAME = "Disk Doctor";
const GB = 1024 ** 3;

/** 1080×1350 share card. Rendered inside the Done screen and exported to PNG with html-to-image. */
export const Poster = forwardRef<HTMLDivElement, { report: CleanReport }>(({ report }, ref) => {
  const { before, after } = report;
  const freed = Math.max(0, after.free - before.free);
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
        <h1 className="p-h1">My drive can <em>breathe</em> again. <span>Finally.</span></h1>
        <div className="p-hero">
          <div className="p-big">{(freed / GB).toFixed(freed >= 100 * GB ? 0 : 1)}</div>
          <div className="p-unit"><b>GB</b><span>of space freed up</span></div>
        </div>
        <div className="p-gauges">
          {gauges.map(({ label, disk, color }) => (
            <div className="p-gauge" key={label}>
              <div className="p-ring">
                {ring(usedPct(disk), color)}
                <div className="p-ring-c"><b>{usedPct(disk)}%</b><span>full</span></div>
              </div>
              <div>
                <div className="p-k">{label}</div>
                <div className="p-v">{(disk.free / GB).toFixed(1)} GB</div>
                <div className="p-s">free of {Math.round(disk.total / GB)} GB</div>
              </div>
            </div>
          ))}
        </div>
        <div className="p-foot">
          <span>Cleaned up with</span>
          <b>{APP_NAME}</b>
        </div>
      </div>
    </div>
  );
});
