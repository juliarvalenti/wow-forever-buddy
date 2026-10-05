import { useEffect, useState } from "react";
import {
  Archive,
  Coins,
  FolderOpen,
  Home,
  type LucideIcon,
  Puzzle,
  Scale,
  ScrollText,
  Settings,
  Shield,
  SquareTerminal,
  Users,
} from "lucide-react";
import { type CharacterCard, commands, type StartupFailure } from "@/lib/bindings";
import { LiveDot, StatusDot } from "@/components/d";
import { useSnapshotCount } from "@/hooks/useBackups";
import { useGameStatus } from "@/hooks/useGameStatus";
import { useCharacters } from "@/hooks/useCharacters";
import { useInstall } from "@/hooks/useInstall";
import { useRecovery } from "@/hooks/useRestore";
import { duration } from "@/lib/format";
import { Addons } from "@/screens/Addons";
import { Adventure } from "@/screens/Adventure";
import { Backups } from "@/screens/Backups";
import { Characters } from "@/screens/Characters";
import { Dashboard } from "@/screens/Dashboard";
import { GameFolder } from "@/screens/GameFolder";
import { Ledger } from "@/screens/Ledger";
import { Macros } from "@/screens/Macros";
import { RecoveryBanner, RecoveryDialog } from "@/screens/Recovery";
import { Settings as SettingsScreen } from "@/screens/Settings";
import { StartupError } from "@/screens/StartupError";

type Screen =
  | "dashboard"
  | "characters"
  | "ledger"
  | "adventures"
  | "backups"
  | "game"
  | "addons"
  | "macros"
  | "settings";

type NavRow =
  | { group: string }
  | { id: Screen; label: string; icon: LucideIcon; n?: number | null }
  | { soon: string; icon: LucideIcon };

/** A sidebar row for a screen that isn't built yet: listed, not clickable. */
function SoonNav({ label, icon: Icon }: { label: string; icon: LucideIcon }) {
  return (
    <button className="soon" disabled title={`${label} · coming soon`}>
      <Icon size={16} aria-hidden />
      <span className="lbl">{label}</span>
      <span className="n">soon</span>
    </button>
  );
}

/** Asks first whether the app could start; only then mounts the app, since
 *  in the failure case no other command has state to work with. */
export default function App() {
  const [failure, setFailure] = useState<StartupFailure | null | undefined>(undefined);
  useEffect(() => {
    commands.startupFailure().then(setFailure, () => setFailure(null));
  }, []);
  if (failure === undefined) return <div className="d-root" />;
  return <div className="d-root">{failure ? <StartupError failure={failure} /> : <Shell />}</div>;
}

function Shell() {
  const game = useGameStatus();
  const install = useInstall();
  const recovery = useRecovery();
  const snapshots = useSnapshotCount();
  // With the addon's data: who logged out last (RFC 3339 UTC sorts as time).
  const { overview } = useCharacters();
  const lastPlayed = overview?.characters.reduce<CharacterCard | null>(
    (best, c) => (best == null || c.last_seen > best.last_seen ? c : best),
    null,
  )?.name;
  const [screen, setScreen] = useState<Screen>("dashboard");
  // The adventure on show; null is the newest. The sidebar opens the newest.
  const [adventureId, setAdventureId] = useState<number | null>(null);
  const openAdventure = (id: number | null) => {
    setAdventureId(id);
    setScreen("adventures");
  };
  const [deferred, setDeferred] = useState(false);
  const [openSnapshot, setOpenSnapshot] = useState<string | null>(null);
  const [backupsFilter, setBackupsFilter] = useState<"Safety" | null>(null);
  const [, tick] = useState(0);

  // "Decide later" applies to one interrupted restore; a new one asks again.
  useEffect(() => {
    if (!recovery.pending) setDeferred(false);
  }, [recovery.pending]);
  // First run: no game folder yet, so start there.
  useEffect(() => {
    if (install.state.kind === "none") setScreen("game");
  }, [install.state.kind]);
  // Keep "session 1h 42m" current.
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 60_000);
    return () => clearInterval(t);
  }, []);

  // The mocks' sidebar: two groups. Screens that aren't built yet are listed
  // as "soon" and do nothing, never dead links.
  const nav: NavRow[] = [
    { group: "Overview" },
    { id: "dashboard", label: "Dashboard", icon: Home },
    { id: "characters", label: "Characters", icon: Users, n: overview?.characters.length || null },
    { id: "ledger", label: "Ledger", icon: Coins },
    { id: "adventures", label: "Adventures", icon: ScrollText },
    { soon: "Auction House", icon: Scale },
    { group: "Game files" },
    { id: "backups", label: "Backups", icon: Archive, n: snapshots },
    { id: "game", label: "Game folder", icon: FolderOpen },
    { id: "addons", label: "Addons", icon: Puzzle },
    { id: "macros", label: "Macros", icon: SquareTerminal },
  ];
  const folderOk = install.state.kind === "ok";
  // The saved folder went missing (drive unplugged, folder moved). Backups
  // stay viewable, which is when you'd want them, with writes locked.
  const folderMissing = install.state.kind === "invalid" ? install.state.error : null;
  // Until a game folder has been set at all, that's the screen.
  // (Settings stays reachable: keys and backup options don't need one.)
  const current: Screen = folderOk || folderMissing != null || screen === "settings" ? screen : "game";

  return (
    <div className="d-app">
      <aside className="d-side">
        <div className="d-brand">
          <span className="mark" aria-hidden>
            <Shield size={18} />
          </span>
          <span className="bt">
            <span className="name" style={{ display: "block" }}>Forever Buddy</span>
            <span className="sub">for WoW: Forever</span>
          </span>
        </div>
        <nav className="d-nav">
          {nav.map((row) => {
            if ("group" in row)
              return (
                <div key={row.group} className="d-nav-group">
                  {row.group}
                </div>
              );
            const { icon: Icon } = row;
            if ("soon" in row) return <SoonNav key={row.soon} label={row.soon} icon={Icon} />;
            return (
              <button
                key={row.id}
                title={row.label}
                aria-current={current === row.id ? "page" : undefined}
                onClick={() => (row.id === "adventures" ? openAdventure(null) : setScreen(row.id))}
              >
                <Icon size={16} aria-hidden />
                <span className="lbl">{row.label}</span>
                {row.n != null && <span className="n">{row.n}</span>}
              </button>
            );
          })}
        </nav>
        <div className="d-status">
          {game?.unknown ? (
            <div className="d-status-row">
              <LiveDot />
              <span>Can't tell if WoW is running</span>
            </div>
          ) : game?.running ? (
            <div className="d-status-row">
              <LiveDot />
              <span>
                WoW is running
                {game.since && <span className="d-dim"> · {duration(game.since)}</span>}
              </span>
            </div>
          ) : (
            <div className="d-status-row d-dim">
              <StatusDot muted />
              <span>WoW isn't running</span>
            </div>
          )}
          {lastPlayed && (
            <div className="d-status-row d-status-last d-dim">
              <span>Last played {lastPlayed}</span>
            </div>
          )}
          <div className="d-status-row">
            {folderOk ? <StatusDot /> : <LiveDot />}
            <span>
              {folderOk
                ? "Game folder found"
                : folderMissing != null
                  ? "Game folder missing"
                  : "Game folder not set"}
            </span>
          </div>
        </div>
        <div className="d-side-foot d-nav">
          <button
            title="Settings"
            aria-current={current === "settings" ? "page" : undefined}
            onClick={() => setScreen("settings")}
          >
            <Settings size={16} aria-hidden />
            <span className="lbl">Settings</span>
          </button>
        </div>
      </aside>

      <main className="d-main">
        {deferred && (
          <div style={{ padding: "16px 24px 0" }}>
            <RecoveryBanner status={recovery.status} onReview={() => setDeferred(false)} />
          </div>
        )}
        {current === "dashboard" && (
          <Dashboard
            game={game}
            install={install}
            folderMissing={folderMissing}
            onOpenBackups={() => setScreen("backups")}
            onCheckFolder={() => setScreen("game")}
            onOpenAdventure={openAdventure}
            onOpenCharacters={() => setScreen("characters")}
          />
        )}
        {current === "characters" && <Characters onOpenDashboard={() => setScreen("dashboard")} />}
        {current === "backups" && (
          <Backups
            game={game}
            restoresLocked={recovery.pending}
            folderMissing={folderMissing}
            onCheckFolder={() => setScreen("game")}
            select={openSnapshot}
            show={backupsFilter}
          />
        )}
        {current === "addons" && <Addons />}
        {current === "macros" && <Macros />}
        {current === "ledger" && (
          <Ledger onOpenDashboard={() => setScreen("dashboard")} onOpenAdventure={openAdventure} />
        )}
        {current === "adventures" && (
          <Adventure
            id={adventureId}
            onOpen={openAdventure}
            onOpenDashboard={() => setScreen("dashboard")}
            onOpenJournal={() => setScreen("ledger")}
          />
        )}
        {current === "game" && <GameFolder install={install} />}
        {current === "settings" && (
          <SettingsScreen install={install} onOpenGameFolder={() => setScreen("game")} />
        )}
      </main>

      {recovery.pending && !deferred && (
        <RecoveryDialog
          recovery={recovery}
          onLater={() => setDeferred(true)}
          onOpenSafety={(id) => {
            setDeferred(true);
            setScreen("backups");
            if (id) setOpenSnapshot(id);
            else setBackupsFilter("Safety");
          }}
        />
      )}
    </div>
  );
}
