import { useState, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import type { CharacterCard, Goal, GoalKind, GoalView } from "@/lib/bindings";
import { Button, Dialog, Panel, PanelHeader, PrimaryButton, Segmented } from "@/components/d";
import type { useGoals } from "@/hooks/useGoals";
import { classStyle } from "@/screens/Characters";
import { plural } from "@/lib/format";

// G1 goals (IMPLEMENTING §21, dashboard.html): a level for a character, or
// gold for a character or the whole account, optionally by a date. Plain
// facts only: no red, no "behind", no streaks. Item goals are Lists.

const DAY = 86_400_000;
const COPPER = 10_000;
const MAX_LABEL = 24;

/** "Fri" within the week, else "Fri 9 Oct". */
function day(iso: string, now = Date.now()): string {
  const t = new Date(iso);
  const near = Math.abs(t.getTime() - now) < 6 * DAY;
  return t.toLocaleDateString(undefined, near ? { weekday: "short" } : { weekday: "short", day: "numeric", month: "short" });
}

const g = (copper: number) => `${Math.floor(copper / COPPER).toLocaleString()}g`;

function Who({ name, cls }: { name: string; cls: string | null }) {
  return (
    <span className="ch-cc" style={classStyle({ class: cls })}>
      {name}
    </span>
  );
}

/** What both a goal and a proposed one have. */
type Shape = Pick<Goal, "character" | "class" | "kind" | "target" | "label" | "by">;

/** "Brannic to level 55", "500g for Fizzwick's mount", "500g for the mount". */
export function GoalWords({ v }: { v: Shape }) {
  const target = v.target ?? 0;
  const who = v.character ? <Who name={v.character} cls={v.class} /> : null;
  if (v.kind === "level") return <>{who} to level {target}</>;
  const amount = g(target);
  const label = v.label?.trim();
  if (!who) return <>{label ? `${amount} ${label}` : `${amount} across the account`}</>;
  // "for the mount" reads as Fizzwick's mount.
  const own = label?.match(/^for the (.+)$/i);
  if (own)
    return (
      <>
        {amount} for {who}'s {own[1]}
      </>
    );
  if (label)
    return (
      <>
        {amount} {label}, {who}
      </>
    );
  return (
    <>
      {who} to {amount}
    </>
  );
}

/** The right-hand date: "by Fri", "was Fri", "no date", "done Thu". */
function when(v: Pick<Goal, "by" | "done_at">, now: number): { text: string; cls: string } {
  if (v.done_at) return { text: `done ${day(v.done_at, now)}`, cls: "ok" };
  if (!v.by) return { text: "no date", cls: "" };
  if (new Date(v.by).getTime() < now) return { text: `was ${day(v.by, now)}`, cls: "" };
  return { text: `by ${day(v.by, now)}`, cls: "" };
}

function fraction(goal: Goal): number {
  const target = goal.target ?? 0;
  const start = goal.start ?? 0;
  const cur = goal.current ?? start;
  if (goal.kind === "gold") return target > 0 ? cur / target : 0;
  return target > start ? (cur - start) / (target - start) : 0;
}

/** "52 and 40% now · 3 days left · ~0.9 levels a day", "212g of 500g · 288g to go". */
function facts(goal: Goal, now: number): string {
  const parts: string[] = [];
  const by = goal.producer.startsWith("agent:") ? `from "${goal.producer.slice(6)}", approved` : null;
  if (goal.done_at) {
    if (by) parts.push(by);
    parts.push(`leaves this list on ${day(new Date(new Date(goal.done_at).getTime() + 3 * DAY).toISOString(), now)}`);
    return parts.join(" · ");
  }
  const target = goal.target ?? 0;
  const cur = goal.current;
  if (cur == null) parts.push("nothing known yet");
  else if (goal.kind === "level") {
    // Rounded: 52.4 is 39.99…% in floating point.
    const pct = Math.min(99, Math.round((cur % 1) * 100));
    parts.push(pct > 0 ? `${Math.floor(cur)} and ${pct}% now` : `${Math.floor(cur)} now`);
  } else {
    parts.push(`${g(cur)} of ${g(target)}`, `${g(Math.max(0, target - cur))} to go`);
  }
  if (goal.by) {
    const left = Math.ceil((new Date(goal.by).getTime() - now) / DAY);
    if (left > 0) parts.push(plural(left, "day left", "days left"));
  }
  if (goal.kind === "level" && goal.per_day != null) parts.push(`~${goal.per_day.toFixed(1)} levels a day`);
  if (by) parts.push(by);
  if (goal.as_of && now - new Date(goal.as_of).getTime() > DAY) parts.push("as of logout");
  return parts.join(" · ");
}

function GoalRow({ goal, onRemove }: { goal: Goal; onRemove?: () => void }) {
  const now = Date.now();
  const w = when(goal, now);
  return (
    <li className="gl-row">
      <div className="gl-top">
        <span className="gl-what">
          <GoalWords v={goal} />
        </span>
        <span className={`gl-when ${w.cls}`}>{w.text}</span>
        {onRemove && (
          <button className="gl-x" aria-label="Remove goal" title="Remove goal" onClick={onRemove}>
            <X size={12} aria-hidden />
          </button>
        )}
      </div>
      {!goal.done_at && (
        <div className="ch-bar">
          <i
            className={goal.kind === "level" ? "xp" : "fill"}
            style={{ width: `${Math.max(0, Math.min(1, fraction(goal))) * 100}%` }}
          />
        </div>
      )}
      <small>{facts(goal, now)}</small>
    </li>
  );
}

/** The Approvals preview: the row as it will look, before any progress. */
export function GoalPreview({ v }: { v: GoalView }) {
  const now = Date.now();
  const w = when({ by: v.by, done_at: null }, now);
  return (
    <ul className="gl-list ap-goal">
      <li className="gl-row">
        <div className="gl-top">
          <span className="gl-what">
            <GoalWords v={v} />
          </span>
          <span className="gl-when">{w.text}</span>
        </div>
      </li>
    </ul>
  );
}

export function NewGoalButton({ onClick }: { onClick: () => void }) {
  return (
    <Button variant="ghost" onClick={onClick}>
      <Plus size={12} aria-hidden /> New goal
    </Button>
  );
}

/** The Dashboard panel. Without goals it's not shown; Characters' header
 *  carries the New goal button instead. */
export function GoalsPanel({ goals, onNew, onRemove }: { goals: Goal[]; onNew: () => void; onRemove: (id: number) => void }) {
  const done = goals.filter((x) => x.done_at).length;
  const active = goals.length - done;
  const meta = [active > 0 ? `${active} active` : null, done > 0 ? `${done} done` : null].filter(Boolean).join(" · ");
  return (
    <Panel>
      <PanelHeader title="Goals">
        <span className="d-dim">{meta}</span>
        <span className="d-grow" />
        <NewGoalButton onClick={onNew} />
      </PanelHeader>
      <ul className="gl-list">
        {goals.map((x) => (
          <GoalRow key={x.id} goal={x} onRemove={x.done_at ? undefined : () => onRemove(x.id)} />
        ))}
      </ul>
    </Panel>
  );
}

type Mode = GoalKind | "items";

/** New goal (§21): Level / Gold / Collect items…, who, the target, an
 *  optional date and, for gold, a short label. */
export function NewGoalDialog({
  characters,
  add,
  onClose,
  onCollect,
}: {
  characters: CharacterCard[];
  add: ReturnType<typeof useGoals>["add"];
  onClose: () => void;
  /** "Collect items…": item goals are Lists. */
  onCollect: () => void;
}) {
  const latest = [...characters].sort((a, b) => (a.last_seen < b.last_seen ? 1 : -1))[0];
  const [mode, setMode] = useState<Mode>("level");
  // "" is the whole account (gold only).
  const [who, setWho] = useState<string>(latest ? String(latest.id) : "");
  const [target, setTarget] = useState("");
  const [by, setBy] = useState("");
  const [label, setLabel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const pick = (m: Mode) => {
    if (m === "items") return onCollect();
    setMode(m);
    setError(null);
    if (m === "level" && who === "" && latest) setWho(String(latest.id));
  };
  const n = Number(target);
  const valid = target !== "" && Number.isInteger(n) && n > 0 && (mode === "gold" || who !== "");

  const save = async () => {
    if (mode === "items") return;
    setBusy(true);
    const refused = await add({
      character_id: who === "" ? null : Number(who),
      kind: mode,
      target: mode === "gold" ? n * COPPER : n,
      label: mode === "gold" && label.trim() ? label.trim() : null,
      // The end of the chosen day, local time.
      by: by ? new Date(`${by}T23:59:59`).getTime() / 1000 : null,
    });
    setBusy(false);
    if (refused) setError(refused.replace(/^goal: /, "Needs "));
    else onClose();
  };

  let field: ReactNode;
  if (mode === "level")
    field = (
      <label className="d-field gl-target">
        <span className="d-dim">Level</span>
        <input inputMode="numeric" value={target} placeholder="55" aria-label="Level" autoFocus onChange={(e) => setTarget(e.target.value.replace(/\D/g, ""))} />
      </label>
    );
  else
    field = (
      <label className="d-field gl-target">
        <input inputMode="numeric" value={target} placeholder="500" aria-label="Gold" autoFocus onChange={(e) => setTarget(e.target.value.replace(/\D/g, ""))} />
        <span className="d-dim">g</span>
      </label>
    );

  return (
    <Dialog
      title="New goal"
      onClose={onClose}
      footer={
        <>
          <span className="d-grow" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <PrimaryButton onClick={save} disabled={busy || !valid}>
            Add goal
          </PrimaryButton>
        </>
      }
    >
      <Segmented<Mode>
        options={[
          { value: "level", label: "Level" },
          { value: "gold", label: "Gold" },
          { value: "items", label: "Collect items…" },
        ]}
        value={mode}
        onChange={pick}
      />
      <div className="gl-form">
        <label className="d-field">
          <select value={who} aria-label="Who" onChange={(e) => setWho(e.target.value)}>
            {mode === "gold" && <option value="">Whole account</option>}
            {characters.map((c) => (
              <option key={c.id} value={c.id} style={classStyle(c)}>
                {c.surname ? `${c.name} ${c.surname}` : c.name}
              </option>
            ))}
          </select>
        </label>
        {field}
        <label className="d-field gl-by">
          <span className="d-dim">by</span>
          <input type="date" value={by} aria-label="By" onChange={(e) => setBy(e.target.value)} />
        </label>
        {mode === "gold" && (
          <label className="d-field gl-label">
            <input value={label} maxLength={MAX_LABEL} placeholder="for the mount" aria-label="What it's for" onChange={(e) => setLabel(e.target.value)} />
          </label>
        )}
      </div>
      {error && <p className="d-letter-bad">{error}</p>}
    </Dialog>
  );
}
