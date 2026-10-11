import { useEffect, useState } from "react";
import { Plus, X } from "lucide-react";
import type {
  CharacterCard,
  Decision,
  Delivery,
  List,
  ListChange,
  ListItem,
  Proposal,
  SeenItem,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import {
  Button,
  Callout,
  Dialog,
  ItemIcon,
  LiveDot,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  PrimaryButton,
  StatusDot,
} from "@/components/d";
import { useApprovals } from "@/hooks/useApprovals";
import { useCharacters } from "@/hooks/useCharacters";
import { useLists } from "@/hooks/useLists";
import { useGameStatus } from "@/hooks/useGameStatus";
import { useNotes } from "@/hooks/useNotes";
import { ago, coins, holder, plural } from "@/lib/format";
import { LoginNotes } from "@/screens/LoginNotes";
import "@/styles/lists.css";

// B2 (IMPLEMENTING §15): lists of what you're gathering across characters,
// shown in game at vendors, the AH and the mailbox (INGAME §10). The app only
// keeps the lists and sends them; nothing here or in game buys or sends.

/** "~1g 12s", "~24g", "~50s": the last scan's lowest buyout. */
function price(copper: number): string {
  const [g, s, c] = coins(copper);
  if (g > 0) return `~${g.toLocaleString()}g${s ? ` ${s}s` : ""}`;
  return s > 0 ? `~${s}s` : `~${c}c`;
}

const cc = (cls: string | null | undefined) =>
  ({ "--cc": cls ? `var(--c-${cls})` : undefined }) as React.CSSProperties;

function Name({ who }: { who: { name: string; class: string | null } }) {
  return (
    <span className="ch-cc" style={cc(who.class)}>
      {who.name}
    </span>
  );
}

/** Where the most of it is: "Sela bank". */
function place(h: ListItem["holders"][number]): string {
  const best = Math.max(h.bags, h.bank, h.mail);
  return best === h.bags ? "bags" : best === h.bank ? "bank" : "mail";
}

/** The Lists slot's state, in bridge.html's words. */
function sent(d: Delivery, waitingFor: string): { live: boolean; text: string; hint?: string } {
  switch (d.state) {
    case "synced":
      return {
        live: false,
        text: `in the game since ${new Date(d.since).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`,
      };
    case "pending":
      return { live: true, text: "waiting for a sync", hint: "/reload in game to send" };
    case "waiting":
      return { live: true, text: `waiting for ${waitingFor} to close` };
    case "restart":
      return { live: true, text: "restart WoW once", hint: "after the addon update" };
    case "failed":
      return { live: true, text: "couldn't write", hint: "the game keeps the last lists" };
  }
}

export function Lists({
  focus,
  startNew,
  onStarted,
  onReview,
}: {
  focus?: number | null;
  /** Open on a new list (New goal's "Collect items…"). */
  startNew?: boolean;
  onStarted?: () => void;
  onReview: () => void;
}) {
  const lists = useLists();
  // P2 (§15): agents' list proposals also show in place. Same queue entries
  // as Approvals; new lists stay there until approved.
  const { approvals, decide } = useApprovals();
  const proposalsFor = (id: number) =>
    (approvals?.waiting ?? []).filter((p) => p.list?.list_id === id && !p.list.gone);
  const { overview } = useCharacters();
  const characters = overview?.characters ?? [];
  const [selected, setSelected] = useState<number | null>(focus ?? null);
  useEffect(() => {
    if (focus != null) setSelected(focus);
  }, [focus]);
  const [editing, setEditing] = useState<List | "new" | null>(startNew ? "new" : null);
  useEffect(() => {
    if (startNew) onStarted?.();
  }, [startNew, onStarted]);

  const { notes } = useNotes();
  const all = lists.view?.lists ?? [];
  const list = all.find((l) => l.id === selected) ?? all[0] ?? null;
  const errands = all.reduce((n, l) => n + l.items.reduce((m, i) => m + i.errands.length, 0), 0);
  // Notes still to show: not yet shown once.
  const waiting = (notes ?? []).filter((n) => !(n.once && n.shown_at)).length;

  // BUG-LISTS: a failed load used to leave a blank page. Say why, with a
  // way to try again.
  if (!lists.view) {
    return (
      <Page>
        <PageHeader title="Lists" />
        {lists.error && (
          <Callout tone="bad">
            Couldn't load your lists: {lists.error}{" "}
            <Button variant="ghost" onClick={lists.reload}>
              Try again
            </Button>
          </Callout>
        )}
      </Page>
    );
  }

  const newList = (
    <Button onClick={() => setEditing("new")}>
      <Plus size={14} aria-hidden />
      New list
    </Button>
  );

  return (
    <Page>
      <PageHeader
        title="Lists"
        lede="What you're gathering, across all your characters. Shown in game at vendors, the auction house and the mailbox."
        actions={all.length > 0 ? newList : undefined}
      />
      {all.length === 0 ? (
        <Panel>
          <PanelBody>
            <div className="ls-empty">
              <p>
                No lists yet. Make one for anything you're gathering across characters: mats for a profession,
                consumables for raid night.
              </p>
              {newList}
            </div>
          </PanelBody>
        </Panel>
      ) : (
        <section className="ls-split">
          <Panel>
            <PanelHeader title="Your lists" />
            <ul className="ls-ll">
              {all.map((l) => {
                const pending = proposalsFor(l.id).length;
                return (
                  <li key={l.id}>
                    <button aria-current={l.id === list?.id ? "true" : undefined} onClick={() => setSelected(l.id)}>
                      <span className="nm">{l.name}</span>
                      {pending > 0 ? (
                        <span className="pend">{plural(pending, "proposal", "proposals")}</span>
                      ) : (
                        <span className="n">{plural(l.items.length, "item", "items")}</span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          </Panel>

          {list && (
            <ListTable
              key={list.id}
              list={list}
              scanAt={lists.view.scan_at}
              onEdit={() => setEditing(list)}
              lists={lists}
              proposals={proposalsFor(list.id)}
              onDecide={async (ids, d) => {
                await decide(ids, d);
                lists.reload();
              }}
              onReview={onReview}
            />
          )}

          <div className="ls-side">
            <LoginNotes characters={characters} onChange={lists.reload} />
            <Panel>
              <PanelHeader title="Sent to the game" />
              <SentRow
                label="Lists and errands"
                delivery={lists.view.delivery}
                summary={[
                  plural(all.length, "list", "lists"),
                  errands ? plural(errands, "errand", "errands") : null,
                ]}
              />
              <SentRow
                label="Login briefing"
                delivery={lists.view.briefing}
                summary={[waiting ? plural(waiting, "note", "notes") : "no notes waiting"]}
              />
            </Panel>
          </div>
        </section>
      )}
      {lists.error && !editing && <p className="d-letter-bad ls-err">{lists.error}</p>}
      {editing && (
        <ListDialog
          list={editing === "new" ? null : editing}
          characters={characters}
          error={lists.error}
          onClose={() => setEditing(null)}
          onSave={async (name, forCharacter) => {
            const ok =
              editing === "new"
                ? await lists.create(name, forCharacter)
                : await lists.update(editing.id, name, forCharacter);
            if (ok) setEditing(null);
          }}
          onDelete={
            editing === "new"
              ? undefined
              : async () => {
                  if (await lists.remove(editing.id)) {
                    setEditing(null);
                    setSelected(null);
                  }
                }
          }
        />
      )}
    </Page>
  );
}

/** One Bridge slot this screen feeds, in bridge.html's four states. */
function SentRow({
  label,
  delivery,
  summary,
}: {
  label: string;
  delivery: Delivery;
  summary: (string | null)[];
}) {
  const s = sent(delivery, holder(useGameStatus()));
  return (
    <div className="ls-sent">
      {s.live ? <LiveDot /> : <StatusDot />}
      <span>{label}</span>
      <span className={`st${s.live ? " wait" : ""}`}>{s.text}</span>
      <small>{[...summary, s.hint].filter(Boolean).join(" · ")}</small>
    </div>
  );
}

/** An item an agent proposes adding: its own ember row. (A changed need
 *  shows in the item's existing row instead.) Who and why are in the bar. */
function ProposedRow({ c }: { c: ListChange }) {
  const q = c.quality != null ? `ch-q${c.quality}` : "";
  return (
    <tr className="prop">
      <td>
        <div className="ls-an">
          <span className={`ch-ico ${q}`} aria-hidden>
            <b>{c.name.slice(0, 1)}</b>
            <ItemIcon id={c.icon_file_id} />
          </span>
          <span className={`nm ${q}`}>{c.name}</span>
        </div>
      </td>
      <td className="num need">{c.need}</td>
      <td className="have">
        <small className="chg">proposed</small>
      </td>
      <td className="num" />
      <td />
    </tr>
  );
}

function ListTable({
  list,
  scanAt,
  onEdit,
  lists,
  proposals,
  onDecide,
  onReview,
}: {
  list: List;
  scanAt: string | null;
  onEdit: () => void;
  lists: ReturnType<typeof useLists>;
  proposals: Proposal[];
  onDecide: (ids: number[], d: Decision) => Promise<void>;
  onReview: () => void;
}) {
  const changes = proposals.flatMap((p) => (p.list?.changes ?? []).map((c) => ({ p, c })));
  const ids = proposals.map((p) => p.id);
  const producers = [...new Set(proposals.map((p) => p.producer))];
  const meta = [
    list.for_character ? (
      <span key="for">
        for <Name who={list.for_character} />
      </span>
    ) : null,
    list.producer.startsWith("agent:") ? `from "${list.producer.slice("agent:".length)}"` : null,
    scanAt && list.items.some((i) => i.price != null) ? `prices from your scan ${ago(scanAt)}` : null,
  ].filter(Boolean);
  return (
    <Panel className="ls-main">
      <PanelHeader title={list.name}>
        <span className="d-dim ls-meta">
          {meta.map((m, i) => (
            <span key={i}>
              {i > 0 && " · "}
              {m}
            </span>
          ))}
        </span>
        <span className="d-grow" />
        <Button variant="ghost" onClick={onEdit}>
          Edit list
        </Button>
      </PanelHeader>
      <table className="d-table ls-grid">
        <thead>
          <tr>
            <th>Item</th>
            <th className="num">Need</th>
            <th>Your characters have</th>
            <th className="num">Last scan</th>
            <th aria-label="Remove" />
          </tr>
        </thead>
        <tbody>
          {list.items.map((i) => (
            <Row
              key={i.id}
              item={i}
              list={list}
              lists={lists}
              proposed={changes.find(({ c }) => c.item_id === i.item_id)?.c}
            />
          ))}
          {changes
            .filter(({ c }) => !list.items.some((i) => i.item_id === c.item_id))
            .map(({ p, c }) => (
              <ProposedRow key={`${p.id}-${c.item_id}`} c={c} />
            ))}
          <AddRow listId={list.id} lists={lists} />
        </tbody>
      </table>
      {changes.length > 0 && (
        <div className="ls-applybar">
          <span className="grow">
            <b>{plural(changes.length, "proposed change", "proposed changes")}</b> from{" "}
            {producers.map((p) => `"${p}"`).join(", ")}
            {proposals.length === 1 && proposals[0].reason ? `: "${proposals[0].reason}"` : ""}. Nothing changes
            until you apply it.{" "}
            <button className="d-link" onClick={onReview}>
              Review in Approvals
            </button>
          </span>
          <Button variant="ghost" onClick={() => onDecide(ids, "decline")}>
            Discard
          </Button>
          <PrimaryButton onClick={() => onDecide(ids, "approve")}>Apply</PrimaryButton>
        </div>
      )}
    </Panel>
  );
}

function Row({
  item,
  list,
  lists,
  proposed,
}: {
  item: ListItem;
  list: List;
  lists: ReturnType<typeof useLists>;
  /** An agent's proposed new need for this item (§15): shown in this row. */
  proposed?: ListChange;
}) {
  const [editing, setEditing] = useState(false);
  const [need, setNeed] = useState(String(item.need));
  // For a character: what it holds itself (the rest are errands). Otherwise
  // all characters together.
  const forId = list.for_character?.id;
  const own =
    forId == null
      ? item.have
      : item.holders.filter((h) => h.character.id === forId).reduce((n, h) => n + h.bags + h.bank + h.mail, 0);
  const done = own >= item.need;
  const top = item.holders[0];
  const q = item.quality != null ? `ch-q${item.quality}` : "";
  const errand = item.errands[0];
  const sub = errand
    ? `errand: send ${errand.count} to ${list.for_character?.name ?? ""}`
    : own > 0 && !done
      ? `of ${item.need}`
      : null;

  const commit = async () => {
    const n = Number(need);
    setEditing(false);
    if (Number.isInteger(n) && n !== item.need && !(await lists.setNeed(item.id, n))) setNeed(String(item.need));
  };

  return (
    <tr className={proposed ? "prop" : done ? "done" : undefined}>
      <td>
        <div className="ls-an">
          <span className={`ch-ico ${q}`} aria-hidden>
            <b>{item.name.slice(0, 1)}</b>
            <ItemIcon id={item.icon_file_id} />
          </span>
          <span className={`nm ${q}`}>{item.name}</span>
        </div>
      </td>
      <td className="num need">
        {proposed ? (
          <>
            <s className="was">{item.need}</s>
            {proposed.need}
          </>
        ) : editing ? (
          <input
            className="ls-needin"
            type="number"
            min={1}
            max={9999}
            value={need}
            autoFocus
            aria-label={`Need of ${item.name}`}
            onChange={(e) => setNeed(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") {
                setNeed(String(item.need));
                setEditing(false);
              }
            }}
          />
        ) : (
          <button className="ls-needbtn" title={`Need ${item.need}. Click to change`} onClick={() => setEditing(true)}>
            {done ? "done" : own > 0 ? `${item.need - own} more` : item.need}
          </button>
        )}
      </td>
      <td className="have">
        {top ? (
          <>
            {done ? `${own} of ${item.need}` : item.have.toLocaleString()} · <Name who={top.character} />{" "}
            {place(top)}
          </>
        ) : (
          "none"
        )}
        {proposed ? <small className="chg">proposed</small> : sub && <small>{sub}</small>}
      </td>
      <td className="num">{item.price != null ? price(item.price) : ""}</td>
      <td className="rm">
        <button className="ls-x" title="Remove from the list" aria-label={`Remove ${item.name}`} onClick={() => lists.removeItem(item.id)}>
          <X size={14} aria-hidden />
        </button>
      </td>
    </tr>
  );
}

/** "+ Add an item…": search items your characters have seen, or type a name. */
function AddRow({ listId, lists }: { listId: number; lists: ReturnType<typeof useLists> }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<SeenItem | null>(null);
  const [found, setFound] = useState<SeenItem[]>([]);
  const [need, setNeed] = useState("1");

  useEffect(() => {
    if (picked || !query.trim()) {
      setFound([]);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      commands.itemsSeenSearch(query).then((r) => live && setFound(r), () => live && setFound([]));
    }, 120);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [query, picked]);

  const reset = () => {
    setQuery("");
    setPicked(null);
    setNeed("1");
    setOpen(false);
  };
  const add = async () => {
    const n = Number(need);
    const item = picked ? { id: picked.item_id } : { name: query.trim() };
    if (await lists.addItem(listId, item, n)) reset();
  };
  const ready = (picked != null || query.trim() !== "") && Number.isInteger(Number(need)) && Number(need) >= 1;

  if (!open)
    return (
      <tr className="ls-add">
        <td colSpan={5}>
          <button className="d-link" onClick={() => setOpen(true)}>
            + Add an item…
          </button>
          <span className="d-dim"> search your characters' items or type a name</span>
        </td>
      </tr>
    );
  return (
    <tr className="ls-add open">
      <td colSpan={5}>
        <div className="ls-addform">
          <span className="d-field ls-q">
            <input
              value={picked ? picked.name : query}
              autoFocus
              placeholder="Runecloth"
              aria-label="Item to add"
              spellCheck={false}
              onChange={(e) => {
                setPicked(null);
                setQuery(e.target.value);
              }}
              onKeyDown={(e) => {
                if (e.key === "Escape") reset();
                if (e.key === "Enter" && ready) add();
              }}
            />
            {found.length > 0 && (
              <ul className="ls-found" role="listbox">
                {found.map((f) => (
                  <li key={f.item_id}>
                    <button
                      className={f.quality != null ? `ch-q${f.quality}` : undefined}
                      onClick={() => {
                        setPicked(f);
                        setFound([]);
                      }}
                    >
                      {f.name}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </span>
          <span className="d-dim">need</span>
          <span className="d-field ls-n">
            <input
              type="number"
              min={1}
              max={9999}
              value={need}
              aria-label="Need"
              onChange={(e) => setNeed(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && ready && add()}
            />
          </span>
          <Button variant="ghost" onClick={reset}>
            Cancel
          </Button>
          <Button onClick={add} disabled={!ready}>
            Add
          </Button>
        </div>
      </td>
    </tr>
  );
}

function ListDialog({
  list,
  characters,
  error,
  onClose,
  onSave,
  onDelete,
}: {
  list: List | null;
  characters: CharacterCard[];
  error: string | null;
  onClose: () => void;
  onSave: (name: string, forCharacter: number | null) => void;
  onDelete?: () => void;
}) {
  const [name, setName] = useState(list?.name ?? "");
  const [who, setWho] = useState<number | null>(list?.for_character?.id ?? null);
  return (
    <Dialog
      title={list ? "Edit list" : "New list"}
      onClose={onClose}
      footer={
        <>
          {onDelete && (
            <Button variant="ghost" onClick={onDelete}>
              Delete list
            </Button>
          )}
          <span className="d-grow" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <PrimaryButton onClick={() => onSave(name, who)} disabled={!name.trim()}>
            {list ? "Save" : "Make list"}
          </PrimaryButton>
        </>
      }
    >
      <div className="ls-form">
        <label>
          <span className="k">Name</span>
          <span className="d-field">
            <input
              value={name}
              maxLength={60}
              autoFocus
              placeholder="Tailoring 300"
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && name.trim() && onSave(name, who)}
            />
          </span>
        </label>
        <label>
          <span className="k">For</span>
          <span className="d-field">
            <select value={who ?? ""} onChange={(e) => setWho(e.target.value ? Number(e.target.value) : null)}>
              <option value="">Anyone</option>
              {characters.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </span>
        </label>
        <p className="d-dim">
          With a character, what it still needs and your other characters hold becomes an errand at the mailbox.
        </p>
        {error && <p className="d-letter-bad">{error}</p>}
      </div>
    </Dialog>
  );
}
