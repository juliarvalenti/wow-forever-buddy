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
import { commands, type StartupFailure } from "@/lib/bindings";
import { LiveDot, StatusDot } from "@/components/d";
import { useSnapshotCount } from "@/hooks/useBackups";
import { useGameStatus } from "@/hooks/useGameStatus";
import { useInstall } from "@/hooks/useInstall";
import { useRecovery } from "@/hooks/useRestore";
import { duration } from "@/lib/format";
import { Backups } from "@/screens/Backups";
import { Characters } from "@/screens/Characters";
import { Dashboard } from "@/screens/Dashboard";
import { GameFolder } from "@/screens/GameFolder";
import { RecoveryBanner, RecoveryDialog } from "@/screens/Recovery";
import { StartupError } from "@/screens/StartupError";

type Screen = "dashboard" | "characters" | "backups" | "game";

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
  const [screen, setScreen] = useState<Screen>("dashboard");
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
    { id: "characters", label: "Characters", icon: Users },
    { soon: "Ledger", icon: Coins },
    { soon: "Adventures", icon: ScrollText },
    { soon: "Auction House", icon: Scale },
    { group: "Game files" },
    { id: "backups", label: "Backups", icon: Archive, n: snapshots },
    { id: "game", label: "Game folder", icon: FolderOpen },
    { soon: "Addons", icon: Puzzle },
    { soon: "Macros", icon: SquareTerminal },
  ];
  const folderOk = install.state.kind === "ok";
  // The saved folder went missing (drive unplugged, folder moved). Backups
  // stay viewable, which is when you'd want them, with writes locked.
  const folderMissing = install.state.kind === "invalid" ? install.state.error : null;
  // Until a game folder has been set at all, that's the screen.
  const current: Screen = folderOk || folderMissing != null ? screen : "game";

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
                onClick={() => setScreen(row.id)}
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
          <SoonNav label="Settings" icon={Settings} />
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
          />
        )}
        {current === "characters" && <Characters />}
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
        {current === "game" && <GameFolder install={install} />}
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
