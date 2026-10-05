// Formatting for the D screens: sentence case, tabular numbers, no em dashes.

import type { AppError } from "./bindings";

/** "48.2 MB". `null` (an f64 the backend couldn't send) shows as a dash. */
export function bytes(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${i === 0 ? v : v.toFixed(v < 10 ? 2 : 1)} ${units[i]}`;
}

/** "Today, 21:15", "Yesterday, 21:15" or "1 Oct, 21:15". */
export function when(iso: string, now = new Date()): string {
  const d = new Date(iso);
  const midnight = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((midnight(now) - midnight(d)) / 86400000);
  const day =
    days === 0
      ? "Today"
      : days === 1
        ? "Yesterday"
        : d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  return `${day}, ${time}`;
}

/** "3 hours ago". */
export function ago(iso: string, now = Date.now()): string {
  const s = Math.round((now - new Date(iso).getTime()) / 1000);
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  if (s < 60) return rtf.format(-s, "second");
  if (s < 3600) return rtf.format(-Math.round(s / 60), "minute");
  if (s < 86400) return rtf.format(-Math.round(s / 3600), "hour");
  return rtf.format(-Math.round(s / 86400), "day");
}

/** "1h 42m" since an RFC 3339 time. */
export function duration(sinceIso: string, now = Date.now()): string {
  const m = Math.max(0, Math.round((now - new Date(sinceIso).getTime()) / 60000));
  return m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** "1h 42m", "2h 05m" or "22m" for a length of time. */
export function span(ms: number): string {
  const m = Math.max(0, Math.round(ms / 60000));
  if (m < 60) return `${m}m`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

function clock(d: Date): string {
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** "Today", "Yesterday" or "Fri 2 Oct". */
function dayLabel(d: Date, now: Date): string {
  const midnight = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((midnight(now) - midnight(d)) / 86400000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

/** A session's time: "Today, since 19:12" while running, else
 *  "Yesterday, 19:40 – 22:52". */
export function sessionWhen(startIso: string, endIso: string | null, now = new Date()): string {
  const start = new Date(startIso);
  if (!endIso) return `${dayLabel(start, now)}, since ${clock(start)}`;
  return `${dayLabel(start, now)}, ${clock(start)} – ${clock(new Date(endIso))}`;
}

/** "Sunday, 4 October". */
export function longDate(d = new Date()): string {
  return d.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
}

export function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** Commands throw `AppError` (`{ kind, detail? }`); anything else is shown as is. */
export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "kind" in e;
}

/** A short, human message for an error a command threw. */
export function errorText(e: unknown): string {
  if (!isAppError(e)) return String(e);
  switch (e.kind) {
    case "GameRunning":
      return "WoW is running. Close the game first.";
    case "NoInstall":
      return "The game folder isn't set yet.";
    case "RestorePending":
      return "Your last restore didn't finish. Roll it back or finish it first.";
    case "Busy":
      return "Another backup or restore is running.";
    case "ReadOnly":
      return `Some files are marked read-only: ${e.detail.paths.join(", ")}`;
    case "DeletionsChanged":
      return "Restore stopped before changing anything. More files would be removed than you confirmed.";
    case "BackupCorrupt":
      return `This snapshot is damaged: ${e.detail.files.join(", ")}`;
    case "Parse":
      return `${e.detail.file} line ${e.detail.line}: ${e.detail.msg}`;
    default: {
      const other: { kind: string; detail?: unknown } = e;
      return other.detail !== undefined ? String(other.detail) : other.kind;
    }
  }
}
