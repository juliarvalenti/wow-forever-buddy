import { useEffect, useState } from "react";
import { commands, type StartupFailure } from "@/lib/bindings";
import { LiveDot, StatusDot } from "@/components/d";
import { useGameStatus } from "@/hooks/useGameStatus";
import { useInstall } from "@/hooks/useInstall";
import { useRecovery } from "@/hooks/useRestore";
import { duration } from "@/lib/format";
import { Backups } from "@/screens/Backups";
import { GameFolder } from "@/screens/GameFolder";
import { RecoveryBanner, RecoveryDialog } from "@/screens/Recovery";
import { StartupError } from "@/screens/StartupError";

type Screen = "backups" | "game";

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
  const [screen, setScreen] = useState<Screen>("backups");
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

  const nav: { id: Screen; label: string }[] = [
    { id: "backups", label: "Backups" },
    { id: "game", label: "Game folder" },
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
        <div className="d-brand">Forever Buddy</div>
        <nav className="d-nav">
          {nav.map((n) => (
            <button
              key={n.id}
              aria-current={current === n.id ? "page" : undefined}
              onClick={() => setScreen(n.id)}
            >
              {n.label}
            </button>
          ))}
        </nav>
        <div className="d-status">
          {game?.running ? (
            <div className="d-status-row">
              <LiveDot />
              <span>
                WoW is running
                {game.since && <span className="d-dim"> · {duration(game.since)}</span>}
              </span>
            </div>
          ) : (
            <div className="d-status-row d-dim">WoW isn't running</div>
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
      </aside>

      <main className="d-main">
        {deferred && (
          <div style={{ padding: "16px 24px 0" }}>
            <RecoveryBanner status={recovery.status} onReview={() => setDeferred(false)} />
          </div>
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
