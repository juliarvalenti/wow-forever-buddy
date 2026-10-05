import { useEffect, useState } from "react";
import { ChevronRight } from "lucide-react";
import { commands, events, type Adventure } from "@/lib/bindings";
import { PanelBody, PanelHeader, Record } from "@/components/d";
import { useEvent } from "@/hooks/useEvent";
import { gold, sessionWhen, span } from "@/lib/format";
import { ItemName, label } from "@/screens/Adventure";

// The Dashboard's "Last adventure" card (design/mocks/round-3/dashboard.html,
// V9): the newest adventure, condensed, with a way into the full recap.

/** The newest adventure, followed as new notes are read. `undefined` while
 *  loading, `null` when there's none yet. */
export function useLastAdventure() {
  const [adventure, setAdventure] = useState<Adventure | null | undefined>(undefined);
  const load = () => {
    commands.adventureGet(null).then(setAdventure, () => setAdventure(null));
  };
  useEffect(load, []);
  useEvent(events.ingestCompleted, load);
  return adventure;
}

export function LastAdventure({ a, onOpen }: { a: Adventure; onOpen: (id: number) => void }) {
  const levelled = a.level_start != null && a.level_end != null && a.level_end > a.level_start;
  const repairs = a.tally.repairs ?? 0;
  const xp = a.tally.xp ?? (a.tally.quest_xp ?? 0);
  const cls = a.class?.toLowerCase();
  return (
    <Record ruled tilt>
      <PanelHeader title="Last adventure">
        <span className="d-grow" />
        <span>
          {sessionWhen(a.login, a.logout)}
          {a.played_secs != null && ` · ${span(a.played_secs * 1000)}`}
        </span>
        <button className="d-link" style={{ marginLeft: 12, whiteSpace: "nowrap" }} onClick={() => onOpen(a.id)}>
          Read entry <ChevronRight size={12} aria-hidden style={{ display: "inline", verticalAlign: "-2px" }} />
        </button>
      </PanelHeader>
      <PanelBody>
        <div className="d-adv-head" style={{ color: "var(--ink)" }}>
          <div>
            <div className="nm ch-cc" style={{ "--cc": cls ? `var(--c-${cls})` : undefined } as React.CSSProperties}>
              {a.name}
            </div>
            <div className="sub">
              {[label(a.race), label(a.class)].filter(Boolean).join(" ")}
              {a.level_start != null && ` · Level ${a.level_start}${levelled ? ` → ${a.level_end}` : ""}`}
            </div>
            <div className="when">{a.title}</div>
          </div>
          {levelled && (
            <div className="seal" title={`Reached level ${a.level_end}`}>
              <span>
                Ding!
                <b>{a.level_end}</b>
              </span>
            </div>
          )}
        </div>

        <div className="d-tally" style={{ color: "var(--ink)" }}>
          <div>
            <div className="k">Gold</div>
            <div className={`v ${(a.tally.gold ?? 0) < 0 ? "d-down" : "d-up"}`}>
              {a.tally.gold != null ? gold(a.tally.gold, true) : ""}
            </div>
          </div>
          <div>
            <div className="k">{a.tally.xp != null ? "Experience" : "Quest XP"}</div>
            <div className="v">+{xp.toLocaleString()}</div>
          </div>
          <div>
            <div className="k">Loot</div>
            <div className="v">
              {a.tally.loot.toLocaleString()} <small>{a.tally.loot === 1 ? "item" : "items"}</small>
            </div>
          </div>
          <div>
            <div className="k">Quests</div>
            <div className="v">{a.quests.length}</div>
          </div>
        </div>

        {(a.gained.length > 0 || a.spent.length > 0 || repairs > 0) && (
          <div className="d-adv" style={{ marginTop: 10 }}>
            <div className="col" style={{ padding: "0 12px 0 0" }}>
              <div className="d-sechead" style={{ marginTop: 4 }}>Gained</div>
              <ul className="d-quests">
                {a.gained.slice(0, 4).map((i) => (
                  <li key={`${i.item_id}|${i.how}`}>
                    <ItemName i={i} />
                    {i.count > 1 && <span className="r">×{i.count}</span>}
                  </li>
                ))}
              </ul>
            </div>
            <div className="col" style={{ padding: "0 0 0 12px" }}>
              <div className="d-sechead" style={{ marginTop: 4 }}>Spent</div>
              <ul className="d-quests">
                {a.spent.slice(0, repairs > 0 ? 3 : 4).map((i) => (
                  <li key={`${i.item_id}|${i.how}`}>
                    <ItemName i={i} />
                    <span className="r">
                      {i.how === "sold" ? "sold " : ""}×{i.count}
                    </span>
                  </li>
                ))}
                {repairs > 0 && (
                  <li>
                    Repairs <span className="r d-down">{gold(-repairs)}</span>
                  </li>
                )}
              </ul>
            </div>
          </div>
        )}

        {a.travelled.length > 0 && (
          <div className="d-zones">
            Travelled
            {a.travelled.map((z) => (
              <span key={z} className="d-chips">
                <span>{z}</span>
              </span>
            ))}
          </div>
        )}
      </PanelBody>
    </Record>
  );
}
