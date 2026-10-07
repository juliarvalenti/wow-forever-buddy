import { AlertTriangle, Check, FileText, Lock, Map as MapIcon, ShoppingBag } from "lucide-react";
import type { Decision, ListView, LoginNote, NoteView, PlanView, Proposal, Step } from "@/lib/bindings";
import { Button, ItemIcon, Page, PageHeader, Panel, PanelHeader, PrimaryButton } from "@/components/d";
import { useApprovals } from "@/hooks/useApprovals";
import { useSettings } from "@/hooks/useSettings";
import { ago, plural } from "@/lib/format";
import { classStyle } from "@/screens/Characters";

// design/mocks/round-3/approvals.html, IMPLEMENTING §17; rules from
// docs/specs/agent-mcp.md §4. Every proposed string (note text, reason, the
// producer) is React text, never HTML. The producer is quoted as a claim.
// Three kinds: login notes, quest plans (the active plan alongside) and list
// changes (only the items that change, the old need struck through).

const day = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });

/** "Tue 21:02": the Decided list's time. */
const stamp = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" });

const timing = (n: { once: boolean; until: string | null }) =>
  n.once ? "next login" : n.until ? `until ${day(n.until)}` : "";

const KIND: Record<string, string> = { login_note: "Login note", quest_plan: "Quest plan", list: "List change" };

function Who({ n }: { n: NoteView | PlanView }) {
  return (
    <span className="ch-cc" style={classStyle({ class: n.class })}>
      {n.character}
    </span>
  );
}

function Title({ p }: { p: Proposal }) {
  const kind = KIND[p.kind] ?? "Suggestion";
  const who = p.note ?? p.plan;
  if (who)
    return (
      <>
        {kind} for <Who n={who} />
      </>
    );
  if (p.list) return <>{p.list.list_id == null ? `New list ${p.list.name}` : `Changes to ${p.list.name}`}</>;
  return <>{kind}</>;
}

/** A plan's steps, crossed out where done. */
function Steps({ steps, done = [] }: { steps: Step[]; done?: number[] }) {
  return (
    <ol className="ap-steps">
      {steps.map((s, i) => (
        <li key={i} className={done.includes(i + 1) ? "done" : undefined}>
          {s.text}
          {s.zone && <small>{s.zone}</small>}
        </li>
      ))}
    </ol>
  );
}

function PlanPreview({ v }: { v: PlanView }) {
  const r = v.replaces;
  return (
    <div className={`ap-cmp${r ? "" : " one"}`}>
      {r && (
        <div className="ap-pv">
          <h3>
            Active now · {r.title} · {r.done.length} of {r.steps.length} done
          </h3>
          <Steps steps={r.steps} done={r.done} />
        </div>
      )}
      <div className="ap-pv new">
        <h3>
          Proposed · {v.title} · {plural(v.steps.length, "step", "steps")}
        </h3>
        <Steps steps={v.steps} />
      </div>
    </div>
  );
}

function ListPreview({ v }: { v: ListView }) {
  return (
    <div className="ap-cmp one">
      <div className="ap-pv new">
        <h3>
          {v.list_id == null ? "New list" : plural(v.changes.length, "change", "changes")}
          {v.for_character && (
            <>
              {" "}
              · for{" "}
              <span className="ch-cc" style={classStyle({ class: v.for_character.class || null })}>
                {v.for_character.name}
              </span>
            </>
          )}
        </h3>
        <table className="ap-items">
          <tbody>
            {v.changes.map((c) => {
              const q = c.quality != null ? `ch-q${c.quality}` : "";
              return (
                <tr key={c.item_id}>
                  <td>
                    <div className="ap-an">
                      <span className={`ch-ico ${q}`} aria-hidden>
                        <b>{c.name.slice(0, 1)}</b>
                        <ItemIcon id={c.icon_file_id} />
                      </span>
                      <span className={q}>{c.name}</span>
                    </div>
                  </td>
                  <td>
                    {c.was != null && <span className="old">{c.was}</span>}need {c.need}
                  </td>
                  <td className="chg">{c.was == null ? "added" : "changed"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** Not part of Approve all: a note to pick for, or a list that's gone. */
const needsYou = (p: Proposal) => (p.note?.replaces?.conflict ?? false) || (p.list?.gone ?? false);

function Mine({ now }: { now: LoginNote | null }) {
  if (!now) return <div className="ap-notetxt ap-gone">Removed since. Using the proposal adds it as a new note.</div>;
  return (
    <>
      <div className="ap-notetxt">{now.text}</div>
      <small>
        {now.author === "claude" ? `from "${now.producer ?? "an agent"}", approved` : "by you"} · {timing(now)}
      </small>
    </>
  );
}

function Waiting({
  p,
  decide,
  onOpenList,
}: {
  p: Proposal;
  decide: (ids: number[], d: Decision) => void;
  onOpenList: (id: number) => void;
}) {
  const n = p.note;
  const r = n?.replaces;
  const conflict = r?.conflict ?? false;
  return (
    <div className="ap-prop">
      <div className="ap-top">
        <span className="ap-kind" aria-hidden>
          {p.plan ? <MapIcon size={15} /> : p.list ? <ShoppingBag size={15} /> : <FileText size={15} />}
        </span>
        <div className="ap-t">
          <b>
            <Title p={p} />
          </b>
          <small>
            from "{p.producer}" · {ago(p.created_at)}
            {r && !conflict ? " · replaces a note" : ""}
            {p.plan?.replaces ? " · replaces the active plan" : ""}
            {p.list?.list_id != null && !p.list.gone && (
              <>
                {" · "}
                <button className="d-link" onClick={() => onOpenList(p.list!.list_id!)}>
                  also shown in Lists
                </button>
              </>
            )}
          </small>
        </div>
        <div className="ap-acts">
          {conflict ? (
            <>
              <Button variant="ghost" onClick={() => decide([p.id], "decline")}>
                Keep mine
              </Button>
              <Button onClick={() => decide([p.id], "use_proposed")}>Use proposed</Button>
            </>
          ) : (
            <>
              <Button variant="ghost" onClick={() => decide([p.id], "decline")}>
                Decline
              </Button>
              <Button onClick={() => decide([p.id], "approve")} disabled={p.list?.gone}>
                Approve
              </Button>
            </>
          )}
        </div>
      </div>
      {p.list?.gone && (
        <div className="ap-conflict">
          <AlertTriangle size={13} aria-hidden />
          You deleted this list after "{p.producer}" suggested this, so there's nothing to change.
        </div>
      )}
      {conflict && (
        <div className="ap-conflict">
          <AlertTriangle size={13} aria-hidden />
          You changed this note after "{p.producer}" read it. Pick one; nothing is replaced until you do.
        </div>
      )}
      {p.reason && (
        <div className="ap-why">
          Reason given: <q>{p.reason}</q>
        </div>
      )}
      {n && (
        <div className={`ap-cmp${r ? "" : " one"}`}>
          {r && (
            <div className="ap-pv">
              <h3>{conflict ? "Yours now" : "Replaces"}</h3>
              {conflict ? <Mine now={r.now} /> : <div className="ap-notetxt">{r.saw}</div>}
            </div>
          )}
          <div className="ap-pv new">
            <h3>Proposed</h3>
            <div className="ap-notetxt">{n.text}</div>
            <small>{timing(n)}</small>
          </div>
        </div>
      )}
      {p.plan && <PlanPreview v={p.plan} />}
      {p.list && <ListPreview v={p.list} />}
    </div>
  );
}

function DecidedRow({ p }: { p: Proposal }) {
  const verb = p.status === "applied" ? "approved" : p.status === "discarded" ? "declined" : "not queued";
  const dot = p.status === "applied" ? " ok" : p.status === "rejected" ? " no" : "";
  return (
    <div className="ap-hist">
      <span className={`ap-dot${dot}`} aria-hidden />
      <span>
        <Title p={p} /> {verb}
      </span>
      <small>
        from "{p.producer}" · {stamp(p.decided_at ?? p.created_at)}
        {p.status_reason ? ` · ${p.status_reason}` : ""}
      </small>
    </div>
  );
}

export function Approvals({
  onOpenSettings,
  onOpenList,
}: {
  onOpenSettings: () => void;
  onOpenList: (id: number) => void;
}) {
  const { approvals, error, decide } = useApprovals();
  const { settings } = useSettings();
  const on = settings?.agent_access ?? false;
  const waiting = approvals?.waiting ?? [];
  const together = waiting.filter((p) => !needsYou(p));
  const picks = waiting.filter((p) => p.note?.replaces?.conflict).length;

  return (
    <Page>
      <PageHeader
        title="Approvals"
        lede="What agents you've connected have suggested. Nothing changes until you approve it."
      />
      {error && <p className="d-letter-bad" style={{ color: "var(--bad)", margin: "0 0 12px" }}>{error}</p>}
      <section className="ap-split">
        <Panel>
          <PanelHeader title="Waiting for you">
            {waiting.length > 0 && <span className="d-dim">{waiting.length} · newest first</span>}
          </PanelHeader>
          {!settings || !approvals ? null : waiting.length > 0 ? (
            <>
              {waiting.map((p) => (
                <Waiting key={p.id} p={p} decide={decide} onOpenList={onOpenList} />
              ))}
              {waiting.length > 1 && together.length > 0 && (
                <div className="ap-bar">
                  <span className="grow">
                    <b>
                      {together.length === 1 ? "1 can be approved." : `${together.length} can be approved together.`}
                    </b>
                    {picks === 1 ? " The note needs you to pick." : picks > 1 ? ` ${picks} notes need you to pick.` : ""}
                  </span>
                  <Button
                    variant="ghost"
                    onClick={() =>
                      decide(
                        waiting.map((p) => p.id),
                        "decline",
                      )
                    }
                  >
                    Decline all
                  </Button>
                  <PrimaryButton
                    onClick={() =>
                      decide(
                        together.map((p) => p.id),
                        "approve",
                      )
                    }
                  >
                    Approve {together.length}
                  </PrimaryButton>
                </div>
              )}
            </>
          ) : on ? (
            <div className="ap-blank">
              <span className="ap-kind" aria-hidden>
                <Check size={18} />
              </span>
              <span>
                Nothing waiting. When an agent suggests a quest plan, a login note or a list change, it shows up here.
              </span>
            </div>
          ) : (
            <div className="ap-blank">
              <span className="ap-kind" aria-hidden>
                <Lock size={18} />
              </span>
              <span>Agent access is off, so agents can't read your characters or suggest anything.</span>
              <Button onClick={onOpenSettings}>Open Settings › Agents</Button>
            </div>
          )}
        </Panel>
        <div className="d-stack">
          <Panel>
            <PanelHeader title="Decided">
              <span className="d-dim">last 30 days</span>
            </PanelHeader>
            {approvals && approvals.decided.length === 0 ? (
              <div className="ap-acc">Nothing yet.</div>
            ) : (
              approvals?.decided.map((p) => <DecidedRow key={p.id} p={p} />)
            )}
          </Panel>
          <Panel>
            <PanelHeader title="Agent access" />
            <div className="ap-acc">
              {on ? (
                <span>
                  <b>On.</b> Agents can read your characters and suggest. Only you can apply.
                </span>
              ) : (
                <span>
                  <b>Off.</b> Every request is refused, and nothing new is queued.
                </span>
              )}
              <button className="d-link" onClick={onOpenSettings}>
                Settings › Agents
              </button>
            </div>
          </Panel>
        </div>
      </section>
    </Page>
  );
}
