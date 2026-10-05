import { useEffect, useState } from "react";
import { commands, type StartupFailure } from "@/lib/bindings";
import { Icon, type IconName } from "@/components/d/Icon";
import { useSnapshotCount } from "@/hooks/useBackups";
import { useGameStatus } from "@/hooks/useGameStatus";
import { useInstall } from "@/hooks/useInstall";
import { useRecovery } from "@/hooks/useRestore";
import { Backups } from "@/screens/Backups";
import { Dashboard } from "@/screens/Dashboard";
import { GameFolder } from "@/screens/GameFolder";
import { RecoveryBanner, RecoveryDialog } from "@/screens/Recovery";
import { StartupError } from "@/screens/StartupError";

type Screen = "dashboard" | "backups" | "game";

/** A sidebar row: a group heading, a screen, or a screen that isn't built yet. */
type NavRow =
  | { group: string }
  | { id: Screen; label: string; icon: IconName; n?: number | null }
  | { soon: string; icon: IconName };

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

  const folderOk = install.state.kind === "ok";
  // The saved folder went missing (drive unplugged, folder moved). Backups
  // stay viewable, which is when you'd want them, with writes locked.
  const folderMissing = install.state.kind === "invalid" ? install.state.error : null;
  // Until a game folder has been set at all, that's the screen.
  const current: Screen = folderOk || folderMissing != null ? screen : "game";

  // The mocks' sidebar (_shell.js). Screens that don't exist yet are listed
  // as "soon", never as dead links.
  const nav: NavRow[] = [
    { group: "Overview" },
    { id: "dashboard", label: "Dashboard", icon: "home" },
    { soon: "Characters", icon: "users" },
    { soon: "Ledger", icon: "coins" },
    { soon: "Adventures", icon: "scroll" },
    { soon: "Auction House", icon: "scale" },
    { group: "Game files" },
    { id: "backups", label: "Backups", icon: "archive", n: folderOk ? snapshots : null },
    { id: "game", label: "Game folder", icon: "folder" },
    { soon: "Addons", icon: "puzzle" },
    { soon: "Macros", icon: "terminal" },
  ];

  return (
    <div className="app">
      <aside className="side">
        <div className="brand">
          <div className="mark">
            <Icon name="shield" />
          </div>
          <div className="bt">
            <div className="name">Forever Buddy</div>
            <div className="sub">for WoW: Forever</div>
          </div>
        </div>
        <nav className="nav">
          {nav.map((n, i) =>
            "group" in n ? (
              <div key={i} className="nav-group">
                {n.group}
              </div>
            ) : "soon" in n ? (
              <span key={i} className="nav-item soon" title={`${n.soon} · coming soon`} aria-disabled="true">
                <Icon name={n.icon} />
                <span className="lbl">{n.soon}</span>
                <span className="n">soon</span>
              </span>
            ) : (
              <button
                key={n.id}
                className={`nav-item ${current === n.id ? "active" : ""}`}
                aria-current={current === n.id ? "page" : undefined}
                title={n.label}
                onClick={() => setScreen(n.id)}
              >
                <Icon name={n.icon} />
                <span className="lbl">{n.label}</span>
                {n.n != null && <span className="n">{n.n}</span>}
              </button>
            ),
          )}
        </nav>
        <div className="status">
          {game?.unknown ? (
            <div className="st-row">
              <span className="live" />
              <span className="st-t">Can't tell if WoW is running</span>
            </div>
          ) : game?.running ? (
            <div className="st-row">
              <span className="live" />
              <span className="st-t">
                <b>WoW is running</b>
              </span>
            </div>
          ) : (
            <div className="st-row">
              <span className="okdot closed" />
              <span className="st-t">
                <b>WoW is closed</b>
              </span>
            </div>
          )}
          <div className="st-row">
            <span className={`okdot ${folderOk ? "" : "warn"}`} />
            <span className="st-t">
              {folderOk
                ? "Game folder found"
                : folderMissing != null
                  ? "Game folder missing"
                  : "Game folder not set"}
            </span>
          </div>
        </div>
        <div className="side-foot">
          <span className="nav-item soon" title="Settings · coming soon" aria-disabled="true">
            <Icon name="gear" />
            <span className="lbl">Settings</span>
            <span className="n">soon</span>
          </span>
        </div>
      </aside>

      <main className="main">
        <div className="vignette" />
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
