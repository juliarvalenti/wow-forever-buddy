import { useMemo, useState } from "react";
import { AlertTriangle, FileText, FolderOpen, Search } from "lucide-react";
import { commands, type AddonInfo, type AddonsList } from "@/lib/bindings";
import { Button, Callout, Page, PageHeader, Panel, PanelBody, PanelHeader, Segmented } from "@/components/d";
import { useAddons } from "@/hooks/useAddons";
import { useCharacters } from "@/hooks/useCharacters";
import { ago, characterName, errorText } from "@/lib/format";
import { classStyle } from "@/screens/Characters";

// design/mocks/round-3/addons-readonly.html, IMPLEMENTING.md §9: the first,
// read-only cut of the Addons screen. Nothing here writes: no sets,
// installs, updates, toggles, sources or sizes. TOC text is the addon
// author's, already stripped of WoW markup by the backend, and rendered as
// React text only.

type Show = "all" | "old" | "off";

/** One column per character: its folder name as people say it, and its
 *  class once the addon has seen it (else a neutral diamond). */
type Col = { name: string; class: string | null };

const key = (account: string, group: string, folder: string) =>
  `${account}/${group}/${folder}`.toLowerCase();

/** "all", "none" or "3 of 5". */
function enabledLabel(enabled: boolean[]): string {
  const on = enabled.filter(Boolean).length;
  if (on === enabled.length) return "all";
  if (on === 0) return "none";
  return `${on} of ${enabled.length}`;
}

/** `…\AddOns\Questie`: the end of a path, with the full one in the tooltip. */
function tail(path: string, keep = 2): string {
  const sep = path.includes("\\") ? "\\" : "/";
  const parts = path.split(sep).filter(Boolean);
  return parts.length > keep ? `…${sep}${parts.slice(-keep).join(sep)}` : path;
}

export function Addons() {
  const { list, error } = useAddons();
  const { overview } = useCharacters();
  const [filter, setFilter] = useState("");
  const [show, setShow] = useState<Show>("all");
  const [selected, setSelected] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  const cols: Col[] = useMemo(() => {
    const classOf = new Map(
      (overview?.characters ?? []).map((c) => [key(c.account, c.group_dir, c.folder), c.class]),
    );
    return (list?.characters ?? []).map((c) => ({
      name: characterName(c.folder),
      class: classOf.get(key(c.account, c.group, c.folder)) ?? null,
    }));
  }, [list, overview]);

  const addons = list?.addons ?? [];
  const old = addons.filter((a) => a.out_of_date).length;
  const offEverywhere = addons.filter((a) => a.enabled.length > 0 && a.enabled.every((on) => !on)).length;
  const shown = useMemo(() => {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    return addons.filter((a) => {
      if (show === "old" && !a.out_of_date) return false;
      if (show === "off" && !(a.enabled.length > 0 && a.enabled.every((on) => !on))) return false;
      const text = `${a.title} ${a.name} ${a.author ?? ""}`.toLowerCase();
      return words.every((w) => text.includes(w));
    });
  }, [addons, filter, show]);
  // The first one by default, and whatever was picked while it's still shown.
  const current = shown.find((a) => a.name === selected) ?? shown[0] ?? null;

  const openFolder = () => {
    setOpenError(null);
    commands.appOpenFolder("addons").catch((e) => setOpenError(errorText(e)));
  };
  const openButton = (
    <Button onClick={openFolder}>
      <FolderOpen size={14} aria-hidden />
      Open AddOns folder
    </Button>
  );

  if (list === undefined && !error) return <Page>{null}</Page>;

  // "_classic_beta_\Interface\AddOns", with the platform's separator.
  const where = list ? [list.flavor, "Interface", "AddOns"].join(list.folder.includes("\\") ? "\\" : "/") : "";
  return (
    <Page>
      <PageHeader
        title="Addons"
        lede={
          list
            ? `${addons.length} installed in ${where}${
                list.interface != null ? ` · ${list.game} reads Interface ${list.interface}` : ""
              }`
            : "Set the game folder first."
        }
        actions={list && addons.length > 0 ? openButton : undefined}
      />
      {error && <Callout tone="bad">{error}</Callout>}
      {openError && <Callout tone="bad">{openError}</Callout>}
      {list && addons.length === 0 && (
        <Panel>
          <PanelBody>
            <div className="ad-empty">
              <p className="d-muted">No addons in {where} yet.</p>
              {openButton}
            </div>
          </PanelBody>
        </Panel>
      )}
      {list && addons.length > 0 && (
        <>
          <div className="ad-toolbar">
            <label className="ad-filter">
              <Search size={14} aria-hidden />
              <input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                onKeyDown={(e) => e.key === "Escape" && setFilter("")}
                placeholder="Filter addons…"
                aria-label="Filter addons by name or author"
                spellCheck={false}
              />
            </label>
            <Segmented<Show>
              value={show}
              onChange={setShow}
              options={[
                { value: "all", label: `All ${addons.length}` },
                { value: "old", label: `Out of date ${old}` },
                { value: "off", label: `Off everywhere ${offEverywhere}` },
              ]}
            />
            <span className="meta">Read from each character's AddOns.txt · {ago(list.read_at)}</span>
          </div>
          <div className="ad-split">
            <Panel className="ad-tablewrap">
              <table className="d-table ad-table">
                <thead>
                  <tr>
                    <th>Addon</th>
                    <th className="auth">Author</th>
                    <th>Interface</th>
                    <th>Enabled for</th>
                  </tr>
                </thead>
                <tbody>
                  {shown.map((a) => (
                    <Row
                      key={a.name}
                      a={a}
                      cols={cols}
                      list={list}
                      selected={current?.name === a.name}
                      onSelect={() => setSelected(a.name)}
                    />
                  ))}
                </tbody>
              </table>
              {shown.length === 0 && (
                <PanelBody>
                  <p className="d-dim">No addons match.</p>
                </PanelBody>
              )}
            </Panel>
            {current && <Detail a={current} cols={cols} list={list} />}
          </div>
        </>
      )}
    </Page>
  );
}

function Interface({ a, list }: { a: AddonInfo; list: AddonsList }) {
  if (a.interfaces.length === 0) return <span className="d-dim">Not listed</span>;
  // The client's own number if listed, else the highest.
  const n = list.interface != null && a.interfaces.includes(list.interface) ? list.interface : Math.max(...a.interfaces);
  return <span className={`ad-toc${a.out_of_date ? " old" : ""}`}>{a.out_of_date ? `${n} · out of date` : n}</span>;
}

function Dots({ enabled, cols }: { enabled: boolean[]; cols: Col[] }) {
  const tip = cols.map((c, i) => `${c.name}: ${enabled[i] ? "on" : "off"}`).join("\n");
  return (
    <span className="ad-chars" title={tip}>
      {cols.map((c, i) => (
        <span key={i} className={`ad-dot${enabled[i] ? "" : " off"}`} style={classStyle(c)} />
      ))}
      <small>{cols.length > 0 ? enabledLabel(enabled) : "no characters yet"}</small>
    </span>
  );
}

function Row({
  a,
  cols,
  list,
  selected,
  onSelect,
}: {
  a: AddonInfo;
  cols: Col[];
  list: AddonsList;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <tr className={selected ? "sel" : undefined} onClick={onSelect}>
      <td>
        <span className="ad-name">
          <span className="ad-ico" aria-hidden>
            <b>{a.title.slice(0, 1)}</b>
          </span>
          <span>
            <b>{a.title}</b>
            {a.version && <small>{a.version}</small>}
          </span>
        </span>
      </td>
      <td className="auth">{a.author ?? ""}</td>
      <td>
        <Interface a={a} list={list} />
      </td>
      <td>
        <Dots enabled={a.enabled} cols={cols} />
      </td>
    </tr>
  );
}

function Detail({ a, cols, list }: { a: AddonInfo; cols: Col[]; list: AddonsList }) {
  return (
    <Panel className="ad-side">
      <PanelHeader title={a.title}>{a.version && <span className="d-dim ad-meta">{a.version}</span>}</PanelHeader>
      <PanelBody>
        <div className="ad-detail">
          {a.notes && <p className="what">{a.notes}</p>}
          <dl className="ad-kv">
            <dt>Author</dt>
            <dd>{a.author ?? "Not listed"}</dd>
            <dt>Interface</dt>
            <dd>
              <Interface a={a} list={list} />
            </dd>
            <dt>Needs</dt>
            <dd>{a.needs.length > 0 ? a.needs.join(", ") : "nothing else"}</dd>
            <dt>Folder</dt>
            <dd>
              <code title={a.path}>{tail(a.path)}</code>
            </dd>
          </dl>
          {cols.length > 0 && (
            <div>
              <div className="ad-sec">Enabled for</div>
              <ul className="ad-who">
                {cols.map((c, i) => (
                  <li key={i} className={a.enabled[i] ? undefined : "off"} style={classStyle(c)}>
                    <span className="ad-dot" />
                    <span className="ch-cc">{c.name}</span>
                    <span className="st">{a.enabled[i] ? "on" : "off"}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {a.out_of_date && (
            <p className="ad-note warn">
              <AlertTriangle size={13} aria-hidden />
              <span>Built for an older interface. WoW loads it only with 'Load out of date AddOns' checked.</span>
            </p>
          )}
          <p className="ad-note">
            <FileText size={13} aria-hidden />
            <span>
              Toggling addons comes later, with a safety snapshot first. For now, change them in-game from the
              AddOns button on the character screen.
            </span>
          </p>
        </div>
      </PanelBody>
    </Panel>
  );
}
