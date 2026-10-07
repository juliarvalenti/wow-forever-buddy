import { AlertTriangle, Check, FileText, Lock } from "lucide-react";
import type { Decision, LoginNote, NoteView, Proposal } from "@/lib/bindings";
import { Button, Page, PageHeader, Panel, PanelHeader, PrimaryButton } from "@/components/d";
import { useApprovals } from "@/hooks/useApprovals";
import { useSettings } from "@/hooks/useSettings";
import { ago } from "@/lib/format";
import { classStyle } from "@/screens/Characters";

// design/mocks/round-3/approvals.html, IMPLEMENTING §17; rules from
// docs/specs/agent-mcp.md §4. Every proposed string (note text, reason, the
// producer) is React text, never HTML. The producer is quoted as a claim.
// Only the kinds that exist are shown: login notes now; plans and lists
// join the same list when they land.

const day = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });

/** "Tue 21:02": the Decided list's time. */
const stamp = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" });

const timing = (n: { once: boolean; until: string | null }) =>
  n.once ? "next login" : n.until ? `until ${day(n.until)}` : "";

const KIND: Record<string, string> = { login_note: "Login note" };

function Who({ n }: { n: NoteView }) {
  return (
    <span className="ch-cc" style={classStyle({ class: n.class })}>
      {n.character}
    </span>
  );
}

function Title({ p }: { p: Proposal }) {
  const kind = KIND[p.kind] ?? "Suggestion";
  return p.note ? (
    <>
      {kind} for <Who n={p.note} />
    </>
  ) : (
    <>{kind}</>
  );
}

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

function Waiting({ p, decide }: { p: Proposal; decide: (ids: number[], d: Decision) => void }) {
  const n = p.note;
  const r = n?.replaces;
  const conflict = r?.conflict ?? false;
  return (
    <div className="ap-prop">
      <div className="ap-top">
        <span className="ap-kind" aria-hidden>
          <FileText size={15} />
        </span>
        <div className="ap-t">
          <b>
            <Title p={p} />
          </b>
          <small>
            from "{p.producer}" · {ago(p.created_at)}
            {r && !conflict ? " · replaces a note" : ""}
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
              <Button onClick={() => decide([p.id], "approve")}>Approve</Button>
            </>
          )}
        </div>
      </div>
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

export function Approvals({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { approvals, error, decide } = useApprovals();
  const { settings } = useSettings();
  const on = settings?.agent_access ?? false;
  const waiting = approvals?.waiting ?? [];
  const together = waiting.filter((p) => !p.note?.replaces?.conflict);
  const picks = waiting.length - together.length;

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
                <Waiting key={p.id} p={p} decide={decide} />
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
