export type Tier = "safe" | "ask" | "manual";

export interface Item {
  path: string;
  bytes: number;
  note: string | null;
  open: string | null;
}

export interface Finding {
  id: string;
  name: string;
  category: string;
  tier: Tier;
  what: string;
  after: string | null;
  how: string | null;
  open: string | null;
  bytes: number;
  /** Individual items can be ticked instead of all-or-nothing. */
  selectable: boolean;
  items: Item[];
  recycles: boolean;
  action: string;
}

/** What the UI asks the backend to clean: a finding, optionally only some of its items. */
export interface Selection {
  id: string;
  paths?: string[];
}

export interface DiskInfo {
  mount: string;
  total: number;
  free: number;
}

export interface ScanResult {
  disk: DiskInfo;
  findings: Finding[];
  activeProjects: number;
  staleProjects: number;
  tookMs: number;
}

export interface CleanOutcome {
  id: string;
  ok: boolean;
  bytes: number;
  recycled: boolean;
  message: string | null;
}

export interface CleanReport {
  before: DiskInfo;
  after: DiskInfo;
  outcomes: CleanOutcome[];
}
