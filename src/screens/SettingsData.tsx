import { useState } from "react";
import type { ForgetPreview, TidyCharacter } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { Button, Dialog, LockedAction, Panel, PanelHeader } from "@/components/d";
import { useDataSize, useTidy } from "@/hooks/useTidy";
import { bytes, dayMonth, errorText, plural } from "@/lib/format";

// O2, Settings › Data (IMPLEMENTING §22): the one place to forget.

const cc = (cls: string | null | undefined) =>
  ({ "--cc": cls ? `var(--c-${cls})` : undefined }) as React.CSSProperties;

/** "March", or "March 2025" when it isn't this year. */
function month(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    month: "long",
    year: d.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  });
}

/** "a, b and c". */
function list(parts: string[]): string {
  return parts.length < 2 ? parts.join("") : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

function forgetBody(p: ForgetPreview): string {
  const parts: string[] = [];
  if (p.adventures) parts.push(plural(p.adventures, "adventure", "adventures"));
  if (p.gold_since) parts.push(`gold since ${month(p.gold_since)} (the Ledger's past totals drop by it)`);
  parts.push("bags", "bank", "quests", "recipes", "lockouts");
  if (p.notes) parts.push(plural(p.notes, "note", "notes"));
  if (p.plans) parts.push(p.plans === 1 ? "a plan" : plural(p.plans, "plan", "plans"));
  const n = p.name;
  return (
    `This removes ${n} from Forever Buddy: ${list(parts)}. Lists made for ${n} stay, unassigned. ` +
    `Your WTF folder and backups aren't touched, and Forever Buddy won't read ${n} from them again ` +
    `unless you choose Remember again.`
  );
}

function Name({ name, cls }: { name: string; cls: string | null }) {
  return (
    <span className="ch-cc" style={cc(cls)}>
      {name}
    </span>
  );
}

export function DataPanel({ backupRunning }: { backupRunning: boolean }) {
  const { tidy, error, hide, forget, remember } = useTidy();
  const size = useDataSize();
  const [asking, setAsking] = useState<ForgetPreview | null>(null);
  const [askError, setAskError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [compacting, setCompacting] = useState(false);
  const [compacted, setCompacted] = useState<string | null>(null);

  const ask = (c: TidyCharacter) => {
    setDone(null);
    setAskError(null);
    commands.tidyForgetPreview(c.id).then(setAsking, (e) => setAskError(errorText(e)));
  };
  const confirm = async () => {
    if (!asking) return;
    const name = asking.name;
    setAsking(null);
    if (await forget(asking.id)) {
      setDone(`${name} was removed from Forever Buddy.`);
      size.reload();
    }
  };
  const compact = async () => {
    setCompacting(true);
    setCompacted(null);
    const r = await size.compact();
    setCompacting(false);
    if (r) setCompacted(r[1] < r[0] ? `Freed ${bytes(r[0] - r[1])}.` : "Nothing to free right now.");
  };

  const d = size.data;
  const hidden = tidy?.hidden ?? [];
  const gone = tidy?.gone ?? [];
  const forgotten = tidy?.forgotten ?? [];

  return (
    <Panel>
      <PanelHeader title="Data" />
      <div className="st-set full">
        <div className="t">Forever Buddy's own data</div>
        <div className="d">
          {d ? (
            <>
              Database <b className="st-num">{bytes(d.bytes)}</b> · Daily copies {d.copies} ·{" "}
              {bytes(d.copies_bytes)}
            </>
          ) : (
            "…"
          )}
        </div>
        {d && (
          <ul className="st-keep st-counts">
            <li>
              <span>Characters</span>
              <b>{d.characters.toLocaleString()}</b>
            </li>
            <li>
              <span>Days of gold</span>
              <b>{d.gold_days.toLocaleString()}</b>
            </li>
            <li>
              <span>Adventures</span>
              <b>{d.adventures.toLocaleString()}</b>
            </li>
            <li>
              <span>Items seen</span>
              <b>{d.items_seen.toLocaleString()}</b>
            </li>
            <li>
              <span>Days of auction prices</span>
              <b>{d.price_days.toLocaleString()}</b>
            </li>
          </ul>
        )}
        <div className="ctl">
          <span className="d">Frees space the database no longer uses. Takes a few seconds.</span>
          <span className="d-grow" />
          {backupRunning ? (
            <LockedAction why="Waits for the backup to finish">Compact</LockedAction>
          ) : (
            <Button variant="ghost" onClick={compact} disabled={compacting || !d}>
              {compacting ? "Compacting…" : "Compact"}
            </Button>
          )}
        </div>
        {compacted && <div className="d">{compacted}</div>}
        {size.error && <div className="d err">{size.error}</div>}
      </div>

      {done && (
        <div className="st-set full">
          <div className="d ok">{done}</div>
        </div>
      )}
      {(error || askError) && (
        <div className="st-set full">
          <div className="d err">{error ?? askError}</div>
        </div>
      )}

      {hidden.length > 0 && (
        <div className="st-set full">
          <div className="t">Hidden ({hidden.length})</div>
          <ul className="st-chars">
            {hidden.map((c) => (
              <li key={c.id}>
                <Name name={c.name} cls={c.class} />
                {c.hidden_at && <span className="d-dim">hidden {dayMonth(c.hidden_at)}</span>}
                <span className="d-grow" />
                <Button variant="ghost" onClick={() => hide(c.id, false)}>
                  Unhide
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {gone.length > 0 && (
        <div className="st-set full">
          <div className="t">Not in your WTF folder ({gone.length})</div>
          <ul className="st-chars">
            {gone.map((c) => (
              <li key={c.id}>
                <Name name={c.name} cls={c.class} />
                <span className="d-dim">last seen {dayMonth(c.last_seen)}</span>
                <span className="d-grow" />
                <button className="d-btn ghost st-bad" onClick={() => ask(c)}>
                  Forget…
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {forgotten.length > 0 && (
        <div className="st-set full">
          <div className="t">Forgotten ({forgotten.length})</div>
          <ul className="st-chars">
            {forgotten.map((f) => (
              <li key={f.id}>
                <Name name={f.name} cls={f.class} />
                <span className="d-dim">forgotten {dayMonth(f.forgotten_at)}</span>
                <span className="d-grow" />
                <Button variant="ghost" onClick={() => remember(f.id)}>
                  Remember again
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {asking && (
        <Dialog
          title={`Forget ${asking.name}?`}
          onClose={() => setAsking(null)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setAsking(null)}>
                Cancel
              </Button>
              <button className="d-btn ghost st-bad" onClick={confirm}>
                Forget {asking.name}
              </button>
            </>
          }
        >
          <p>{forgetBody(asking)}</p>
          <p className="d-dim st-small">
            The app's daily copies keep the old history for up to 7 days, until they roll off.
          </p>
        </Dialog>
      )}
    </Panel>
  );
}
