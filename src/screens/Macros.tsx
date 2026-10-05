import { useEffect, useMemo, useState } from "react";
import { Check, ChevronDown, FileText } from "lucide-react";
import type { Macro } from "@/lib/bindings";
import { Button, Callout, Page, PageHeader, Panel, PanelBody, PanelHeader, Segmented } from "@/components/d";
import { useCharacters } from "@/hooks/useCharacters";
import { useMacros } from "@/hooks/useMacros";
import { characterName } from "@/lib/format";
import { classStyle } from "@/screens/Characters";

// design/mocks/round-3/macros-readonly.html, IMPLEMENTING.md §12: the first,
// read-only cut of the Macros screen. Nothing here writes: no editing, icon
// picker, library, "New macro" or copy-to-character. Macro text is the
// player's (or pasted from anywhere): it's tokenized simply and rendered as
// React text only.

type Scope = "account" | "character";

/** Warn from here; above the game's limit is red. */
const HOT = 230;

const key = (account: string, group: string, folder: string) =>
  `${account}/${group}/${folder}`.toLowerCase();

/** "3 Oct" from an RFC 3339 time. */
const day = (iso: string) => new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });

/** One line of a macro, lightly tinted: `#showtooltip` tan, `/commands`
 *  ember, `[conditions]` blue, the rest plain. */
function Line({ text }: { text: string }) {
  const trimmed = text.trimStart();
  const lead = text.slice(0, text.length - trimmed.length);
  if (!trimmed.startsWith("#") && !trimmed.startsWith("/")) return <>{text}</>;
  const space = trimmed.search(/\s/);
  const head = space < 0 ? trimmed : trimmed.slice(0, space);
  const rest = space < 0 ? "" : trimmed.slice(space);
  const parts = rest.split(/(\[[^\]]*\])/).filter(Boolean);
  return (
    <>
      {lead}
      <span className={trimmed.startsWith("#") ? "meta-d" : "cmd"}>{head}</span>
      {parts.map((p, i) =>
        p.startsWith("[") ? (
          <span key={i} className="cond">
            {p}
          </span>
        ) : (
          <span key={i} className="sp">
            {p}
          </span>
        ),
      )}
    </>
  );
}

function Len({ n, max }: { n: number; max: number }) {
  return <span className={`len${n > max ? " over" : n >= HOT ? " hot" : ""}`}>{n}</span>;
}

export function Macros() {
  const { list, error } = useMacros();
  const { overview } = useCharacters();
  const [scope, setScope] = useState<Scope>("character");
  const [who, setWho] = useState(0);
  const [picking, setPicking] = useState(false);
  const [selected, setSelected] = useState(0);
  const [copied, setCopied] = useState(false);

  const classOf = useMemo(
    () => new Map((overview?.characters ?? []).map((c) => [key(c.account, c.group_dir, c.folder), c.class])),
    [overview],
  );
  const chars = list?.characters ?? [];
  const ch = chars[Math.min(who, Math.max(0, chars.length - 1))];
  const chName = ch ? characterName(ch.folder) : "";
  const chStyle = ch ? classStyle({ class: classOf.get(key(ch.account, ch.group, ch.folder)) ?? null }) : {};
  // Account macros from every account folder (usually one).
  const accountMacros = (list?.accounts ?? []).flatMap((a) => a.macros);
  const accountModified = (list?.accounts ?? []).map((a) => a.modified).filter(Boolean).sort().pop() ?? null;
  const macros: Macro[] = scope === "account" ? accountMacros : (ch?.macros ?? []);
  const modified = scope === "account" ? accountModified : (ch?.modified ?? null);
  const current = macros[Math.min(selected, macros.length - 1)] ?? null;
  const max = list?.max ?? 255;

  // A different list starts at its first macro.
  useEffect(() => setSelected(0), [scope, who]);
  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(t);
  }, [copied]);

  const copy = () => {
    if (!current) return;
    navigator.clipboard.writeText(current.body).then(() => setCopied(true));
  };

  if (list === undefined && !error) return <Page>{null}</Page>;
  return (
    <Page>
      <PageHeader
        title="Macros"
        lede={list ? "From each character's macros-cache.txt, as of their last logout" : "Set the game folder first."}
      />
      {error && <Callout tone="bad">{error}</Callout>}
      {list && (
        <>
          <div className="mc-toolbar">
            <Segmented<Scope>
              value={scope}
              onChange={setScope}
              options={[
                { value: "account", label: `Account ${accountMacros.length}` },
                { value: "character", label: `Character ${ch?.macros.length ?? 0}` },
              ]}
            />
            {ch && (
              <span className="mc-who">
                <span className="ad-dot" style={chStyle} />
                <span className="ch-cc mc-name" style={chStyle}>
                  {chName}
                </span>
                <span className="mc-pick">
                  <Button variant="ghost" onClick={() => setPicking((p) => !p)}>
                    Change <ChevronDown size={13} aria-hidden />
                  </Button>
                  {picking && (
                    <ul className="mc-menu" role="listbox">
                      {chars.map((c, i) => {
                        const st = classStyle({ class: classOf.get(key(c.account, c.group, c.folder)) ?? null });
                        return (
                          <li
                            key={`${c.account}/${c.group}/${c.folder}`}
                            role="option"
                            aria-selected={i === who}
                            onClick={() => {
                              setWho(i);
                              setScope("character");
                              setPicking(false);
                            }}
                          >
                            <span className="ad-dot" style={st} />
                            <span className="ch-cc" style={st}>
                              {characterName(c.folder)}
                            </span>
                            <span className="n">{c.macros.length}</span>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </span>
              </span>
            )}
            <span className="meta">
              {accountMacros.length} account macros{ch ? ` · ${ch.macros.length} for ${chName}` : ""}
            </span>
          </div>

          <section className="mc-grid">
            <Panel>
              <PanelHeader title={scope === "account" ? "Account" : chName}>
                <span className="d-dim mc-meta">
                  {macros.length === 1 ? "1 macro" : `${macros.length} macros`}
                </span>
              </PanelHeader>
              {macros.length === 0 ? (
                <PanelBody>
                  <p className="d-muted">
                    {scope === "account"
                      ? "No account macros yet."
                      : `No macros for ${chName} yet. WoW writes them when you log out.`}
                  </p>
                </PanelBody>
              ) : (
                <ul className="mc-list">
                  {macros.map((m, i) => (
                    <li
                      key={i}
                      className={i === selected ? "sel" : undefined}
                      onClick={() => setSelected(i)}
                    >
                      <span className="mc-ico" aria-hidden>
                        <b>{(m.name.trim() || "?").slice(0, 1)}</b>
                      </span>
                      <span className="nm">{m.name.trim() || "Unnamed"}</span>
                      <Len n={m.length} max={max} />
                    </li>
                  ))}
                </ul>
              )}
            </Panel>

            {current && (
              <Panel className="mc-viewer">
                <PanelHeader title={current.name.trim() || "Unnamed"}>
                  <span className="d-dim mc-meta">
                    {scope === "account" ? "account macro" : "character macro"}
                    {modified ? ` · as of logout, ${day(modified)}` : ""}
                  </span>
                </PanelHeader>
                <PanelBody>
                  <div className="mc-body">
                    <div className="mc-top">
                      <span className="mc-ico lg" aria-hidden>
                        <b>{(current.name.trim() || "?").slice(0, 1)}</b>
                      </span>
                      <div className="grow">
                        <div className="t">{current.name.trim() || "Unnamed"}</div>
                        <div className="s">
                          {scope === "account" ? "Account" : chName} · slot {Math.min(selected, macros.length - 1) + 1}
                        </div>
                      </div>
                      {copied && (
                        <span className="mc-copied" role="status">
                          <Check size={13} aria-hidden />
                          Copied
                        </span>
                      )}
                      <Button onClick={copy}>
                        <FileText size={13} aria-hidden />
                        Copy
                      </Button>
                    </div>
                    <div className="mc-code">
                      {current.body.split("\n").map((l, i) => (
                        <div key={i}>{l === "" ? " " : <Line text={l} />}</div>
                      ))}
                    </div>
                    <div className={`mc-count${current.length > max ? " over" : current.length >= HOT ? " hot" : ""}`}>
                      <span>
                        <b>{current.length}</b> / {max}
                      </span>
                      <span className="ch-bar mc-bar">
                        <i className="fill" style={{ width: `${Math.min(100, (current.length / max) * 100)}%` }} />
                      </span>
                      {current.length > max ? (
                        <span className="why">{current.length - max} over: WoW cuts it off at {max}</span>
                      ) : current.length >= HOT ? (
                        <span className="why">{max - current.length} left</span>
                      ) : null}
                    </div>
                    <p className="ad-note">
                      <FileText size={13} aria-hidden />
                      <span>
                        Editing macros and copying them to other characters come later, with a safety snapshot first.
                        For now, Copy puts the text on your clipboard to paste in-game.
                      </span>
                    </p>
                  </div>
                </PanelBody>
              </Panel>
            )}
          </section>
        </>
      )}
    </Page>
  );
}
