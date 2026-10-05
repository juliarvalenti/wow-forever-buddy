import { useEffect, useState } from "react";
import { Check, Clock, FolderOpen, RefreshCw, Scale, ScrollText, Shield, ShoppingBag, TrendingUp } from "lucide-react";
import {
  commands,
  type GameStatus,
  type PlaySession,
  type Settings,
  type SnapshotSummary,
} from "@/lib/bindings";
import {
  Button,
  Callout,
  LiveDot,
  LockedAction,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  Pill,
  Record,
  PrimaryButton,
  Tile,
} from "@/components/d";
import { useBackups } from "@/hooks/useBackups";
import type { useInstall } from "@/hooks/useInstall";
import { thisWeek, useSessions } from "@/hooks/useSessions";
import {
  ago,
  bytes,
  characterName,
  duration,
  errorText,
  longDate,
  plural,
  sessionWhen,
  span,
} from "@/lib/format";

// Copy from design/mocks/round-3/dashboard-noaddon.html (v0.1, no addon).

const MISSING_WHY = "Unlocks when the game folder is found again.";

const TRIGGER_NOTE: Partial<Record<SnapshotSummary["trigger"], string>> = {
  app_start: "when the app started",
  game_exit: "on game exit",
  scheduled: "scheduled",
  pre_restore: "before a restore",
};

/** "Auto · on game exit · 48 MB". */
function backupLine(s: SnapshotSummary): string {
  const what = s.kind === "Manual" ? (s.label ?? "manual backup") : (TRIGGER_NOTE[s.trigger] ?? "");
  return [s.kind, what, bytes(s.total_bytes)].filter(Boolean).join(" · ");
}

/** The last two parts of a path: "…\World of Warcraft\_classic_beta_". */
function shortPath(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  const sep = p.includes("\\") ? "\\" : "/";
  return parts.length > 2 ? `…${sep}${parts.slice(-2).join(sep)}` : p;
}

/** Shorter sessions are hidden from the list (still counted in the week). */
const SHORT_MS = 2 * 60_000;

function sessionMs(s: PlaySession, now = Date.now()): number {
  const end = s.ended_at ? new Date(s.ended_at).getTime() : now;
  return end - new Date(s.started_at).getTime();
}

/** Who played: "Thrandor, Coinpurse", or "Thrandor and 2 others" with the
 *  full list on hover. Never guessed from last played. */
function SessionWho({ s }: { s: PlaySession }) {
  const ended = s.crashed ? " · ended unexpectedly" : "";
  if (!s.ended_at) return <span className="sub">Character known after you log out</span>;
  const names = s.characters.map((c) => characterName(c.name));
  if (names.length === 0) return <span className="sub">No character settings changed{ended}</span>;
  const who =
    names.length <= 2 ? names.join(", ") : `${names[0]} and ${names.length - 1} others`;
  return (
    <span className="sub" title={names.length > 2 ? names.join(", ") : undefined}>
      {who}
      {ended}
    </span>
  );
}

const UNLOCKS = [
  { icon: TrendingUp, title: "Gold over time", text: "Per character and account-wide." },
  { icon: ShoppingBag, title: "Satchels & gear", text: "Search every alt's bags and bank." },
  { icon: ScrollText, title: "Adventures", text: "Gold, XP and loot for each session." },
  { icon: Scale, title: "AH prices", text: "From your own in-game scans." },
];

/** Home screen. v0.1 has no addon, so it shows what the app knows without
 *  one (backups, the game process, WTF folders) and invites the addon. */
export function Dashboard({
  game,
  install,
  folderMissing,
  onOpenBackups,
  onCheckFolder,
}: {
  game: GameStatus | null;
  install: ReturnType<typeof useInstall>;
  /** Why the saved game folder can't be used, if it can't. */
  folderMissing: string | null;
  onOpenBackups: () => void;
  onCheckFolder: () => void;
}) {
  const { list, storage, progress, failed, backUpNow, refresh: refreshBackups } = useBackups();
  const { sessions, characters, refresh: refreshSessions } = useSessions();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  useEffect(() => {
    commands.settingsGet().then(setSettings, () => setSettings(null));
  }, []);

  const active = install.state.kind === "ok" ? install.state.install : null;
  const flavor = active?.flavors.find((f) => f.id === active.active);
  const running = game?.running ?? false;
  const last = list?.[0];
  const names = [...new Set((characters ?? []).map((c) => characterName(c.name)))];
  const week = thisWeek(sessions ?? []);
  // Launcher tests and crashes at login: counted in the week, not listed.
  const shownSessions = (sessions ?? []).filter((s) => !s.ended_at || sessionMs(s) >= SHORT_MS);
  const lastEnded = sessions?.find((s) => s.ended_at);

  const rescan = () => {
    install.refresh();
    refreshBackups();
    refreshSessions();
  };
  const openGameFolder = () => {
    setOpenError(null);
    commands.appOpenFolder("game").catch((e) => setOpenError(errorText(e)));
  };

  return (
    <Page>
      <PageHeader
        title="Dashboard"
        lede={[
          longDate(),
          flavor?.label,
          characters && plural(characters.length, "character found", "characters found"),
        ]
          .filter(Boolean)
          .join(" · ")}
        actions={
          <>
            {folderMissing == null && (
              <Button variant="ghost" onClick={openGameFolder}>
                <FolderOpen size={14} aria-hidden /> Open game folder
              </Button>
            )}
            {folderMissing != null ? (
              <LockedAction why={MISSING_WHY}>Back up now</LockedAction>
            ) : (
              <PrimaryButton onClick={() => backUpNow()} disabled={progress != null}>
                {progress
                  ? `Backing up… ${progress.total > 0 ? `${progress.done} of ${progress.total}` : ""}`
                  : "Back up now"}
              </PrimaryButton>
            )}
          </>
        }
      />

      {folderMissing != null && (
        <Callout tone="stone">
          <span>
            <b>We can't find your game folder, so new backups are paused.</b> Your backups are safe.{" "}
            <span className="d-dim">({folderMissing})</span>
          </span>
          <span className="d-grow" />
          <Button onClick={onCheckFolder}>Check game folder</Button>
        </Callout>
      )}
      {failed && (
        <Callout tone="bad">
          <span>
            <b>That backup didn't finish.</b> {failed}
          </span>
        </Callout>
      )}
      {openError && <Callout tone="bad">{openError}</Callout>}

      <section className="d-strip">
        <Tile
          label="Last backup"
          value={last ? ago(last.created_at) : list ? "None yet" : "…"}
          sub={last ? backupLine(last) : "The first one is taken when the game closes."}
        />
        <Tile
          label="Game"
          value={
            game?.unknown ? (
              <>
                <LiveDot />
                Can't tell
              </>
            ) : running ? (
              <>
                <LiveDot />
                Running
              </>
            ) : (
              "Not running"
            )
          }
          sub={
            game?.unknown
              ? "Can't read the process list right now"
              : running
              ? [flavor?.exe?.split(/[\\/]/).pop(), game?.since && `${duration(game.since)} this session`]
                  .filter(Boolean)
                  .join(" · ")
              : lastEnded?.ended_at
                ? `Last played ${ago(lastEnded.ended_at)}`
                : "Sessions are noted while the app is open."
          }
        />
        <Tile
          label="Characters found"
          value={characters?.length ?? "…"}
          sub="From your WTF folder"
        />
        <Tile
          label="Backups"
          value={
            <>
              {list?.length ?? "…"} <small>{list?.length === 1 ? "snapshot" : "snapshots"}</small>
            </>
          }
          sub={
            storage?.used_bytes != null
              ? storage.budget_bytes != null
                ? `${bytes(storage.used_bytes)} of ${bytes(storage.budget_bytes)} budget`
                : `${bytes(storage.used_bytes)} used`
              : undefined
          }
        />
      </section>

      <section className="d-cols">
        <Record ruled tilt>
          <PanelHeader title="Your ledger is blank">
            <span className="d-grow" />
            <span className="d-dim">one small addon away</span>
          </PanelHeader>
          <PanelBody>
            <p className="d-muted">
              Your backups and play sessions are already being kept. To fill this page with gold,
              gear and what happened in each session, Forever Buddy needs its companion addon,
              which notes down each character when you log out.
            </p>
            <div className="d-unlocks">
              {UNLOCKS.map(({ icon: Icon, title, text }) => (
                <div key={title}>
                  <Icon size={16} aria-hidden />
                  <b>{title}</b>
                  <span>{text}</span>
                </div>
              ))}
            </div>
            <p className="d-dim" style={{ display: "flex", gap: 6, alignItems: "flex-start" }}>
              <Shield size={13} aria-hidden style={{ flexShrink: 0, marginTop: 2 }} />
              The addon only writes to its own SavedVariables file. No network, and nothing else
              in your UI is touched.
            </p>
            <ol className="d-steps">
              <li>
                <div>
                  <div className="st">Install ForeverBuddy</div>
                  <div className="sd">
                    The companion addon arrives with the next update of Forever Buddy. Backups
                    already work in the meantime.
                  </div>
                  <div style={{ marginTop: 8 }}>
                    <Pill kind="auto">
                      <Clock size={12} aria-hidden /> Coming in the next update
                    </Pill>
                  </div>
                </div>
              </li>
              <li>
                <div>
                  <div className="st">Log in once on each character</div>
                  <div className="sd">
                    Or type <span className="d-mono">/reload</span>. Each one is ticked off here
                    as it's noted.
                  </div>
                  {names.length > 0 && (
                    <div className="d-chips">
                      {names.slice(0, 12).map((n) => (
                        <span key={n}>{n}</span>
                      ))}
                      {names.length > 12 && <span>+{names.length - 12} more</span>}
                    </div>
                  )}
                </div>
              </li>
              <li>
                <div>
                  <div className="st">Your ledger fills in</div>
                  <div className="sd">
                    Gold, gear and the session journal appear after each logout. History starts
                    from today.
                  </div>
                </div>
              </li>
            </ol>
          </PanelBody>
        </Record>

        <div className="d-stack">
          <Panel>
            <PanelHeader title="Game folder">
              <span className="d-grow" />
              <Button variant="icon" onClick={rescan} title="Rescan">
                <RefreshCw size={13} aria-label="Rescan" />
              </Button>
            </PanelHeader>
            <ul className="d-rows d-checks">
              {flavor ? (
                <li>
                  <Check size={14} className="ok" aria-hidden />
                  <span className="main">
                    {[flavor.label, flavor.version].filter(Boolean).join(" · ")}
                  </span>
                  <span className="sub">
                    {[shortPath(flavor.dir), flavor.exe?.split(/[\\/]/).pop()].filter(Boolean).join(" · ")}
                  </span>
                </li>
              ) : (
                <li>
                  <LiveDot />
                  <span className="main">{folderMissing != null ? "Game folder missing" : "Game folder not set"}</span>
                  <span className="sub">{folderMissing ?? "Choose it on the Game folder screen."}</span>
                </li>
              )}
              {flavor?.has_wtf && (
                <li>
                  <Check size={14} className="ok" aria-hidden />
                  <span className="main">WTF folder found</span>
                  <span className="sub">
                    {plural(flavor.accounts.length, "account", "accounts")} ·{" "}
                    {plural(flavor.characters, "character", "characters")}
                  </span>
                </li>
              )}
              {settings &&
                (settings.backup?.on_game_exit !== false ? (
                  <li>
                    <Check size={14} className="ok" aria-hidden />
                    <span className="main">Backups are on</span>
                    <span className="sub">
                      On game exit{last && ` · last one ${ago(last.created_at)}`}
                    </span>
                  </li>
                ) : (
                  <li>
                    <LiveDot />
                    <span className="main">Backups on game exit are off</span>
                    <span className="sub">
                      <button className="d-link" onClick={onOpenBackups}>
                        Back up by hand in Backups
                      </button>
                    </span>
                  </li>
                ))}
              <li>
                <LiveDot />
                <span className="main">ForeverBuddy addon not installed</span>
                <span className="sub">Needed for gold, gear and session details</span>
              </li>
            </ul>
          </Panel>

          <Panel>
            <PanelHeader title="Recent sessions">
              <span className="d-grow" />
              <span className="d-dim">from the game process</span>
            </PanelHeader>
            {sessions && shownSessions.length === 0 ? (
              <PanelBody>
                <p className="d-muted">
                  Sessions appear here after you play. Forever Buddy notes when WoW starts and
                  stops.
                </p>
              </PanelBody>
            ) : (
              <ul className="d-rows">
                {shownSessions.slice(0, 5).map((s) => (
                  <li key={s.id}>
                    <span className="main">
                      {!s.ended_at && <LiveDot />}
                      {sessionWhen(s.started_at, s.ended_at)}
                    </span>
                    <span className="side" style={{ color: "var(--chalk-hi)", fontWeight: 600 }}>
                      {span(sessionMs(s))}
                    </span>
                    <SessionWho s={s} />
                  </li>
                ))}
              </ul>
            )}
            {sessions && sessions.length > 0 && (
              <div className="d-panel-foot">
                {week.ms > 0
                  ? `This week: ${span(week.ms)}${week.characters > 0 ? ` across ${plural(week.characters, "character", "characters")}` : ""}. `
                  : "This week: none yet. "}
                The character is the one whose settings changed during the session.
              </div>
            )}
          </Panel>

          <Panel>
            <PanelHeader title="Characters">
              <span className="d-grow" />
              <span className="d-dim">from WTF folders</span>
            </PanelHeader>
            {characters && characters.length === 0 ? (
              <PanelBody>
                <p className="d-muted">No character folders yet. They appear after you log in once.</p>
              </PanelBody>
            ) : (
              <ul className="d-rows">
                {(characters ?? []).slice(0, 5).map((c) => (
                  <li key={`${c.account}|${c.realm}|${c.name}`}>
                    <span className="main">{characterName(c.name)}</span>
                    <span className="side d-dim">
                      {c.last_played ? `last played ${ago(c.last_played)}` : ""}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </div>
      </section>
    </Page>
  );
}
