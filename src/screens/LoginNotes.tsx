import { useState } from "react";
import type { LoginNote } from "@/lib/bindings";
import { Button, Panel, PanelBody, PanelHeader, Segmented } from "@/components/d";
import { useNotes } from "@/hooks/useNotes";

// B1 login notes (IMPLEMENTING §15): a line for one character, shown in the
// game's chat at its next login (once) or at each login until a date. It
// lives on the character sheet until B2's Lists screen takes it in.

const MAX_TEXT = 300;

const day = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });

/** "by you · next login", "from an agent, approved · until Thu 9 Oct", "shown Thu 2 Oct".
 *  P2b swaps "an agent" for the quoted client name it stores ("Claude Desktop"). */
function meta(n: LoginNote): string {
  const by = n.author === "claude" ? "from an agent, approved" : "by you";
  if (n.once && n.shown_at) return `${by} · shown ${day(n.shown_at)}`;
  return `${by} · ${n.once ? "next login" : n.until ? `until ${day(n.until)}` : ""}`;
}

export function LoginNotes({ characterId }: { characterId: number }) {
  const { notes, error, add, remove } = useNotes();
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState("");
  const [mode, setMode] = useState<"once" | "until">("once");
  const [until, setUntil] = useState("");
  const [busy, setBusy] = useState(false);

  const mine = (notes ?? []).filter((n) => n.character_id === characterId);

  const save = async () => {
    setBusy(true);
    // The end of the chosen day, local time.
    const untilSecs = until ? new Date(`${until}T23:59:59`).getTime() / 1000 : null;
    const ok = await add({ character_id: characterId, text, once: mode === "once", until: mode === "once" ? null : untilSecs });
    setBusy(false);
    if (ok) {
      setText("");
      setUntil("");
      setMode("once");
      setAdding(false);
    }
  };

  return (
    <Panel>
      <PanelHeader title="Login notes">
        <span className="d-dim">shown in chat at login</span>
      </PanelHeader>
      <PanelBody>
        {mine.length === 0 && !adding && (
          <p className="d-dim">No notes. Add one to see it in game next time you log in.</p>
        )}
        {mine.map((n) => {
          const done = n.once && n.shown_at != null;
          return (
            <div key={n.id} className={`ch-note${done ? " done" : ""}`}>
              <div className="t">“{n.text}”</div>
              <div className="m">
                <span>{meta(n)}</span>
                {!done && (
                  <Button variant="ghost" onClick={() => remove(n.id)}>
                    Remove
                  </Button>
                )}
              </div>
            </div>
          );
        })}
        {adding ? (
          <div className="ch-note-form">
            <label className="d-field">
              <input
                value={text}
                maxLength={MAX_TEXT}
                placeholder="Hand in the attunement before Thursday's raid"
                aria-label="Note"
                autoFocus
                onChange={(e) => setText(e.target.value)}
              />
            </label>
            <div className="row">
              <Segmented
                options={[
                  { value: "once", label: "Next login" },
                  { value: "until", label: "Until…" },
                ]}
                value={mode}
                onChange={setMode}
              />
              {mode === "until" && (
                <label className="d-field">
                  <input
                    type="date"
                    value={until}
                    aria-label="Show until"
                    onChange={(e) => setUntil(e.target.value)}
                  />
                </label>
              )}
            </div>
            <div className="row">
              <span className="d-grow" />
              <Button variant="ghost" onClick={() => setAdding(false)}>
                Cancel
              </Button>
              <Button onClick={save} disabled={busy || !text.trim() || (mode === "until" && !until)}>
                Add note
              </Button>
            </div>
          </div>
        ) : (
          <button className="d-link ch-note-add" onClick={() => setAdding(true)}>
            + Add a note…
          </button>
        )}
        {error && <p className="d-letter-bad" style={{ marginTop: 6 }}>{error}</p>}
      </PanelBody>
    </Panel>
  );
}
