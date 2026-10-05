import { useEffect, useState } from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  FolderOpen,
  RefreshCw,
  Scale,
  ScrollText,
  Shield,
  ShoppingBag,
  TrendingUp,
} from "lucide-react";
import {
  type AddonStatus,
  type AltLockout,
  type CharacterCard,
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
  Record,
  PrimaryButton,
  Tile,
} from "@/components/d";
import { useAddon } from "@/hooks/useAddon";
import { useBackups } from "@/hooks/useBackups";
import { useCharacters, useLockouts } from "@/hooks/useCharacters";
import { useLedger } from "@/hooks/useLedger";
import { Coins } from "@/screens/Characters";
import { LastAdventure, useLastAdventure } from "@/screens/LastAdventure";
import { Coins as TileCoins } from "@/screens/Ledger";
import type { useInstall } from "@/hooks/useInstall";
import { thisWeek, useSessions } from "@/hooks/useSessions";
import {
  ago,
  bytes,
  characterName,
  OLDER_FOLDERS,
  OLDER_FOLDERS_WHY,
  duration,
  errorText,
  gold,
  longDate,
  plural,
  resetDay,
  resetsIn,
  sessionWhen,
  span,
} from "@/lib/format";

// Copy from design/mocks/round-3/dashboard-noaddon.html (v0.1, no addon).

const MISSING_WHY = "Unlocks when the game folder is found again.";
/** "v0.2.0 · on 7 / 7 characters". */
function addonCount(s: AddonStatus): string {
  const on = s.enabled_on.length;
  const all = on + s.disabled_on.length;
  return `v${s.installed_version} · on ${on} / ${all} ${all === 1 ? "character" : "characters"}`;
}

/** The Game folder row: the count, plus where it's turned off. */
function addonLine(s: AddonStatus): string {
  const off =
    s.disabled_on.length > 0
      ? ` · off on ${s.disabled_on.map(characterName).join(", ")}`
      : "";
  return addonCount(s) + off;
}

/** Step 1 once installed: the count, and what to do where it's off. The app
 *  doesn't switch it on itself (AddOns.txt is WoW's, per character). */
function addonStepLine(s: AddonStatus): string {
  const off =
    s.disabled_on.length > 0
      ? `. Turned off on ${s.disabled_on.map(characterName).join(", ")}. Turn it on in the in-game AddOns list.`
      : "";
  return addonCount(s) + off;
}

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

/** The Account gold tile's sparkline: one point per day of the week, days
 *  before the first reading left out. Nothing for fewer than two points. */
function Spark({ values }: { values: (number | null)[] }) {
  const ys = values.filter((v): v is number => v != null);
  if (ys.length < 2) return null;
  const lo = Math.min(...ys);
  const hi = Math.max(...ys);
  const pts = ys.map((v, i) => {
    const x = (i / (ys.length - 1)) * 74;
    const y = hi === lo ? 12 : 21 - ((v - lo) / (hi - lo)) * 18;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  return (
    <svg className="d-spark" viewBox="0 0 74 24" aria-hidden>
      <polyline points={pts.join(" ")} />
    </svg>
  );
}

/** Rested XP caps at a level and a half. */
function fullyRested(c: CharacterCard): boolean {
  return c.rested != null && c.xp_max != null && c.xp_max > 0 && c.rested >= c.xp_max * 1.5 * 0.99;
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

/** F3 (IMPLEMENTING.md §8): this week's saves across alts, one row per
 *  instance, soonest reset first, with who is saved in class colour. At
 *  most five rows. The caller leaves it out when nobody is saved. */
function LockoutsThisWeek({ lockouts, onOpen }: { lockouts: AltLockout[]; onOpen: () => void }) {
  const rows: { key: string; l: AltLockout["lockout"]; who: AltLockout[] }[] = [];
  for (const a of lockouts) {
    const key = `${a.lockout.name}|${a.lockout.difficulty}`;
    const row = rows.find((r) => r.key === key);
    if (row) row.who.push(a);
    else rows.push({ key, l: a.lockout, who: [a] });
  }
  const saved = new Set(lockouts.map((a) => a.character_id)).size;
  return (
    <Panel>
      <PanelHeader title="Lockouts this week">
        <span className="d-grow" />
        <span className="d-dim">{plural(saved, "character saved", "characters saved")}</span>
      </PanelHeader>
      <ul className="d-rows">
        {rows.slice(0, 5).map(({ key, l, who }) => (
          <li key={key} className="d-open" onClick={onOpen} title={l.reset_at ? resetDay(l.reset_at) : undefined}>
            <span className="main">
              {l.name}
              {l.difficulty && l.difficulty !== "Normal" && <small className="d-dim"> {l.difficulty}</small>}
            </span>
            <span className="side d-dim" style={{ fontWeight: 400 }}>
              {l.reset_at ? resetsIn(l.reset_at) : ""}
            </span>
            <span className="sub">
              {who.map((a, i) => (
                <span key={a.character_id}>
                  {i > 0 && ", "}
                  <span style={{ color: a.class ? `var(--c-${a.class})` : undefined }}>{a.character}</span>
                </span>
              ))}
            </span>
          </li>
        ))}
      </ul>
    </Panel>
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
  onOpenAdventure,
  onOpenCharacters,
}: {
  game: GameStatus | null;
  install: ReturnType<typeof useInstall>;
  /** Why the saved game folder can't be used, if it can't. */
  folderMissing: string | null;
  onOpenBackups: () => void;
  onCheckFolder: () => void;
  onOpenAdventure: (id: number) => void;
  onOpenCharacters: () => void;
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
  // Older settings folders are listed apart and aren't characters (W1b).
  const counted = characters?.filter((c) => !c.older);
  const older = (characters ?? []).filter((c) => c.older);
  const [showOlder, setShowOlder] = useState(false);
  const names = [...new Set((counted ?? []).map((c) => characterName(c.name)))];
  const addon = useAddon();
  const addonCurrent = addon.status?.installed_version != null && !addon.status.update_available;
  const updating = addon.status?.update_available === true;
  const installLabel = updating ? "Update addon" : "Install addon";
  const installWhy = `Close WoW first. ${updating ? "Updating" : "Installing"} writes to your game folder.`;
  // With the addon's data (V9, dashboard.html): account gold, the last
  // adventure and the roster with gold. Without it, the v0.1 state stays.
  const { overview } = useCharacters();
  const lockouts = useLockouts();
  const withAddon = (overview?.characters.length ?? 0) > 0;
  const { ledger } = useLedger("week");
  const lastAdventure = useLastAdventure();
  const byGold = [...(overview?.characters ?? [])].sort((a, b) => (b.money ?? 0) - (a.money ?? 0));
  const chars = overview?.characters ?? [];
  const topLevel = Math.max(0, ...chars.map((c) => c.level ?? 0));
  const atTop = chars.filter((c) => c.level === topLevel).length;
  const rested = chars.filter(fullyRested).length;
  // The newest logout the addon saw; RFC 3339 UTC strings sort as times.
  const lastPlayed = chars.reduce<CharacterCard | null>(
    (best, c) => (best == null || c.last_seen > best.last_seen ? c : best),
    null,
  );
  const week = thisWeek(sessions ?? []);
  // Launcher tests and crashes at login: counted in the week, not listed.
  const shownSessions = (sessions ?? []).filter((s) => !s.ended_at || sessionMs(s) >= SHORT_MS);
  const lastEnded = sessions?.find((s) => s.ended_at);

  // The setup card (step 1 offers the addon) gives way to Last adventure.
  const setupShown = !(withAddon && lastAdventure);
  // While step 1 offers the addon, its button is the page's one bronze.
  const addonOffered = setupShown && addon.status != null && !addonCurrent;
  const BackUpButton = addonOffered ? Button : PrimaryButton;
  /** Install or update the addon, or why not now. Bronze in step 1; stone
   *  on the Game folder row, where "Back up now" is the screen's bronze. */
  const addonAction = (primary: boolean) => (
    <div style={{ marginTop: 8, display: "flex", gap: 10, alignItems: "center", flexWrap: "wrap" }}>
      {folderMissing != null ? (
        <LockedAction why={MISSING_WHY}>{installLabel}</LockedAction>
      ) : running || game?.unknown ? (
        <>
          <LockedAction why={installWhy}>{installLabel}</LockedAction>
          <span className="sd">
            <LiveDot /> {installWhy}
          </span>
        </>
      ) : primary ? (
        <PrimaryButton onClick={addon.install} disabled={addon.busy || !addon.status}>
          {addon.busy ? "Installing…" : installLabel}
        </PrimaryButton>
      ) : (
        <Button onClick={addon.install} disabled={addon.busy || !addon.status}>
          {addon.busy ? "Installing…" : installLabel}
        </Button>
      )}
    </div>
  );

  // With the setup card gone, the Game folder row carries the action (V4's
  // update path stays reachable once there's addon data).
  const rowAction = (
    <div style={{ gridColumn: 2 }}>
      {addonAction(false)}
      {addon.error && <p style={{ margin: "6px 0 0", fontSize: 11.5, color: "var(--bad)" }}>{addon.error}</p>}
    </div>
  );

  // With addon data the roster comes first (it's the payoff, and the Ledger
  // journal covers sessions); without, v0.1's order.
  const recentSessions = (
    <Panel>
      <PanelHeader title="Recent sessions">
        <span className="d-grow" />
        <span className="d-dim">from the game process</span>
      </PanelHeader>
      {sessions && shownSessions.length === 0 ? (
        <PanelBody>
          <p className="d-muted">
            Sessions appear here after you play. Forever Buddy notes when WoW starts and stops.
          </p>
        </PanelBody>
      ) : (
        <ul className="d-rows">
          {shownSessions.slice(0, 5).map((s) => (
            // With the addon, a session opens its (first) adventure.
            <li
              key={s.id}
              className={s.adventures.length > 0 ? "d-open" : undefined}
              title={s.adventures.length > 0 ? "Open this adventure" : undefined}
              onClick={s.adventures.length > 0 ? () => onOpenAdventure(s.adventures[0]) : undefined}
            >
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
  );

  const rescan = () => {
    install.refresh();
    addon.refresh();
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
          withAddon
            ? plural(overview?.characters.length ?? 0, "character", "characters")
            : counted && plural(counted.length, "character found", "characters found"),
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
              // One bronze per screen: while step 1 offers the addon, that's it.
              <BackUpButton onClick={() => backUpNow()} disabled={progress != null}>
                {progress
                  ? `Backing up… ${progress.total > 0 ? `${progress.done} of ${progress.total}` : ""}`
                  : "Back up now"}
              </BackUpButton>
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
        {withAddon && (
          <Tile
            label="Account gold"
            value={<TileCoins copper={overview?.gold ?? 0} />}
            corner={ledger && <Spark values={ledger.chart.account} />}
            sub={
              ledger && (ledger.tiles.this_week ?? 0) !== 0
                ? `${gold(ledger.tiles.this_week ?? 0, true)} this week`
                : `across ${plural(overview?.characters.length ?? 0, "character", "characters")}`
            }
          />
        )}
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
              ? withAddon
                ? [game?.since && `${duration(game.since)} so far`, lastPlayed && `last played ${lastPlayed.name}`]
                    .filter(Boolean)
                    .join(" · ")
                : [flavor?.exe?.split(/[\\/]/).pop(), game?.since && `${duration(game.since)} this session`]
                    .filter(Boolean)
                    .join(" · ")
              : lastEnded?.ended_at
                ? `Last played ${ago(lastEnded.ended_at)}${withAddon && lastPlayed ? ` · ${lastPlayed.name}` : ""}`
                : "Sessions are noted while the app is open."
          }
        />
        {withAddon ? (
          <Tile
            label="Characters"
            value={
              <>
                {chars.length}
                {topLevel > 0 && <small>· {atTop} at {topLevel}</small>}
              </>
            }
            sub={rested > 0 ? `${rested} fully rested` : "Seen by the addon"}
          />
        ) : (
          <Tile
            label="Characters found"
            value={counted?.length ?? "…"}
            sub="From your WTF folder"
          />
        )}
        {!withAddon && (
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
        )}
      </section>

      <section className="d-cols">
        {withAddon && lastAdventure ? (
          <LastAdventure a={lastAdventure} onOpen={onOpenAdventure} />
        ) : (
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
              <li className={addonCurrent ? "done" : undefined}>
                <div>
                  <div className="st">
                    {addonCurrent
                      ? "ForeverBuddy is installed"
                      : updating
                        ? "Update ForeverBuddy"
                        : "Install ForeverBuddy"}
                  </div>
                  <div className="sd">
                    {addonCurrent
                      ? addonStepLine(addon.status!)
                      : updating
                        ? `Version ${addon.status!.installed_version} is installed; this update brings ${addon.status!.bundled_version}.`
                        : (
                            <>
                              Copies the addon into{" "}
                              <span className="d-mono">Interface\AddOns\ForeverBuddy</span>.
                            </>
                          )}
                  </div>
                  {!addonCurrent && addonAction(true)}
                  {addon.error && (
                    <p className="d-letter-bad" style={{ marginTop: 8 }}>
                      {addon.error}
                    </p>
                  )}
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
        )}

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
              {addonCurrent ? (
                <li>
                  <Check size={14} className="ok" aria-hidden />
                  <span className="main">ForeverBuddy addon</span>
                  <span className="sub">{addonLine(addon.status!)}</span>
                </li>
              ) : addon.status?.update_available ? (
                <li>
                  <LiveDot />
                  <span className="main">ForeverBuddy addon v{addon.status.installed_version}</span>
                  <span className="sub">Update available: v{addon.status.bundled_version}</span>
                  {!setupShown && rowAction}
                </li>
              ) : (
                <li>
                  <LiveDot />
                  <span className="main">ForeverBuddy addon not installed</span>
                  <span className="sub">Needed for gold, gear and session details</span>
                  {!setupShown && rowAction}
                </li>
              )}
            </ul>
          </Panel>

          {!withAddon && recentSessions}

          <Panel>
            <PanelHeader title="Characters">
              <span className="d-grow" />
              {withAddon ? (
                <button className="d-link" style={{ whiteSpace: "nowrap" }} onClick={onOpenCharacters}>
                  All <ChevronRight size={12} aria-hidden style={{ display: "inline", verticalAlign: "-2px" }} />
                </button>
              ) : (
                <span className="d-dim">from WTF folders</span>
              )}
            </PanelHeader>
            {withAddon ? (
              // dashboard.html: richest first, in class colour, gold and level.
              <ul className="d-rows d-roster">
                {byGold.slice(0, 5).map((c) => (
                  <li
                    key={c.id}
                    className="d-open"
                    onClick={onOpenCharacters}
                    style={{ "--cc": c.class ? `var(--c-${c.class})` : undefined } as React.CSSProperties}
                  >
                    <span className="d-cdot" aria-hidden />
                    <span className="cc">{c.surname ? `${c.name} ${c.surname}` : c.name}</span>
                    <span className="note">{c.bank_alt ? "bank" : ""}</span>
                    <span className="side">
                      <Coins copper={c.money} silver={false} />
                    </span>
                    <span className="lv">{c.level ?? ""}</span>
                  </li>
                ))}
              </ul>
            ) : characters && characters.length === 0 ? (
              <PanelBody>
                <p className="d-muted">No character folders yet. They appear after you log in once.</p>
              </PanelBody>
            ) : (
              <ul className="d-rows">
                {(counted ?? []).slice(0, 5).map((c) => (
                  <li key={`${c.account}|${c.realm}|${c.name}`}>
                    <span className="main">{characterName(c.name)}</span>
                    <span className="side d-dim">
                      {c.last_played ? `last played ${ago(c.last_played)}` : ""}
                    </span>
                  </li>
                ))}
                {older.length > 0 && (
                  <li className="d-muted">
                    <button
                      type="button"
                      className="main d-link"
                      style={{ color: "inherit" }}
                      title={OLDER_FOLDERS_WHY}
                      aria-expanded={showOlder}
                      onClick={() => setShowOlder((v) => !v)}
                    >
                      {showOlder ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden />}{" "}
                      {OLDER_FOLDERS} · {older.length}
                    </button>
                  </li>
                )}
                {showOlder &&
                  older.map((c) => (
                    <li key={`${c.account}|${c.realm}|${c.name}`} className="d-dim">
                      <i className="main">
                        {c.name} ({c.realm})
                      </i>
                    </li>
                  ))}
              </ul>
            )}
          </Panel>

          {withAddon && lockouts && lockouts.length > 0 && (
            <LockoutsThisWeek lockouts={lockouts} onOpen={onOpenCharacters} />
          )}
          {withAddon && recentSessions}
        </div>
      </section>
    </Page>
  );
}
