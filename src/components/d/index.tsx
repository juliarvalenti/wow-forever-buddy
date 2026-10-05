// The component vocabulary from design/mocks/round-3/IMPLEMENTING.md, named
// after the D materials. Screens use only these, so the styling pass
// restyles them (src/styles/d.css) without touching the screens.

import { Lock } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect } from "react";

type Children = { children?: ReactNode };

export function Page({ children }: Children) {
  return <div className="d-page">{children}</div>;
}

export function PageHeader({
  title,
  lede,
  actions,
}: {
  title: string;
  lede?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="d-page-head">
      <div>
        <h1>{title}</h1>
        {lede && <p className="d-lede">{lede}</p>}
      </div>
      {actions && <div className="d-actions">{actions}</div>}
    </header>
  );
}

export function Panel({ children, className }: Children & { className?: string }) {
  return <section className={`d-panel ${className ?? ""}`}>{children}</section>;
}

export function PanelHeader({ title, children }: Children & { title: ReactNode }) {
  return (
    <div className="d-panel-head">
      <h2>{title}</h2>
      {children}
    </div>
  );
}

export function PanelBody({ children }: Children) {
  return <div className="d-panel-body">{children}</div>;
}

/** Parchment: only for records (the ledger, session recaps). Mock classes:
 *  article.parchment(.tilt) > .ph (h2 + .meta) + .pb(.ruled). */
export function Parchment({
  title,
  meta,
  tilt,
  ruled,
  className,
  children,
}: Children & { title: ReactNode; meta?: ReactNode; tilt?: boolean; ruled?: boolean; className?: string }) {
  return (
    <article className={["parchment", tilt && "tilt", className].filter(Boolean).join(" ")}>
      <div className="ph">
        <h2>{title}</h2>
        {meta && <span className="meta">{meta}</span>}
      </div>
      <div className={ruled ? "pb ruled" : "pb"}>{children}</div>
    </article>
  );
}

export function Tile({ label, value, sub }: { label: string; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="d-tile">
      <div className="k">{label}</div>
      <div className="v">{value}</div>
      {sub && <div className="s">{sub}</div>}
    </div>
  );
}

type ButtonProps = Children & {
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
};

/** Bronze. At most one per screen. */
export function PrimaryButton({ children, onClick, disabled, title }: ButtonProps) {
  return (
    <button className="d-btn d-primary" onClick={onClick} disabled={disabled} title={title}>
      {children}
    </button>
  );
}

export function Button({
  children,
  onClick,
  disabled,
  title,
  variant = "default",
}: ButtonProps & { variant?: "default" | "ghost" | "icon" }) {
  const cls = variant === "default" ? "d-btn" : `d-btn ${variant}`;
  return (
    <button className={cls} onClick={onClick} disabled={disabled} title={title}>
      {children}
    </button>
  );
}

/** A write that's locked while WoW runs: muted, a lock, and why on hover.
 *  Never a disabled PrimaryButton. */
export function LockedAction({ children, why }: Children & { why: string }) {
  return (
    <span className="d-locked" title={why} aria-disabled="true" role="button">
      <Lock size={14} aria-hidden />
      {children}
    </span>
  );
}

/** One per screen. Ember for "WoW is running", bad for the one failed thing,
 *  stone for a quiet persistent notice. */
export function Callout({
  children,
  tone,
}: Children & { tone: "ember" | "bad" | "stone" }) {
  return (
    <div className={`d-callout ${tone}`} role={tone === "bad" ? "alert" : "status"}>
      {children}
    </div>
  );
}

export type PillKind = "auto" | "manual" | "safety" | "ok" | "warn" | "bad";

export function Pill({ kind, children }: Children & { kind: PillKind }) {
  return <span className={`d-pill ${kind}`}>{children}</span>;
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: ReactNode }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="d-seg" role="group">
      {options.map((o) => (
        <button key={o.value} aria-pressed={o.value === value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Checkbox({
  checked,
  onChange,
  children,
}: Children & { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className="d-cb">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {children}
    </label>
  );
}

export function DataTable({ head, children }: Children & { head: ReactNode }) {
  return (
    <table className="d-table">
      <thead>{head}</thead>
      <tbody>{children}</tbody>
    </table>
  );
}

export function Dialog({
  title,
  children,
  footer,
  onClose,
}: Children & { title: string; footer: ReactNode; onClose?: () => void }) {
  useEffect(() => {
    if (!onClose) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="d-scrim" onClick={(e) => e.target === e.currentTarget && onClose?.()}>
      <div className="d-dialog" role="dialog" aria-modal="true" aria-label={title}>
        <header>
          <h2>{title}</h2>
        </header>
        <div className="body">{children}</div>
        <footer>{footer}</footer>
      </div>
    </div>
  );
}

/** Ember pulse: live or pending. */
export function LiveDot() {
  return <span className="d-live" aria-hidden />;
}

/** Green: OK. */
export function StatusDot() {
  return <span className="d-okdot" aria-hidden />;
}

export function Meter({ fraction, over }: { fraction: number; over?: boolean }) {
  const pct = Math.max(0, Math.min(1, fraction)) * 100;
  return (
    <div className={`d-meter ${over ? "over" : ""}`} role="meter" aria-valuenow={Math.round(pct)}>
      <i style={{ width: `${pct}%` }} />
    </div>
  );
}
