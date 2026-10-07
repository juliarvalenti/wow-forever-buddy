import { useEffect, useRef, useState } from "react";
import { AlertTriangle, Check, X } from "lucide-react";
import { Button, LockedAction, Page, PrimaryButton, Record, Switch } from "@/components/d";
import { useAddon } from "@/hooks/useAddon";
import { useBackups } from "@/hooks/useBackups";
import { useGameStatus } from "@/hooks/useGameStatus";
import type { useInstall } from "@/hooks/useInstall";
import { useSettings } from "@/hooks/useSettings";
import { plural, when } from "@/lib/format";
import { FindGame } from "@/screens/GameFolder";

// O1, the guided first launch (onboarding.html, IMPLEMENTING §20): find the
// game, a first backup, the companion addon, two optional extras, then a
// summary. Every step can be skipped and the app remembers where setup got
// to (settings.ui `onboarding.step`), so a restart picks it up. It's opened
// on first run and from Settings › Game ("Run setup again").

/** settings.ui key: the step setup is on, or "done". */
export const SETUP_KEY = "onboarding.step";
export const STEPS = ["find", "backup", "addon", "extras", "done"] as const;
export type SetupStep = (typeof STEPS)[number];

type Outcome = "done" | "skipped";

const LABELS: { step: SetupStep; label: string; optional?: boolean }[] = [
  { step: "find", label: "Find the game" },
  { step: "backup", label: "First backup" },
  { step: "addon", label: "Companion addon" },
  { step: "extras", label: "Extras", optional: true },
];

export function Onboarding({
  install,
  start,
  onFinish,
}: {
  install: ReturnType<typeof useInstall>;
  /** Where to open: the saved step, or "find" for a rerun. */
  start: SetupStep;
  /** Setup is over (finished or skipped): back to the app. */
  onFinish: () => void;
}) {
  const { settings, update } = useSettings();
  const [step, setStep] = useState<SetupStep>(start);
  const [outcome, setOutcome] = useState<Partial<Record<SetupStep, Outcome>>>({});
  const hasFolder = install.state.kind === "ok";
  const active = install.state.kind === "ok" ? install.state.install : null;
  const characters = active?.flavors.find((f) => f.id === active.active)?.characters ?? null;

  // Remembered so a restart opens here; "done" once it's over.
  const go = (next: SetupStep, result?: Outcome) => {
    if (result) setOutcome((o) => ({ ...o, [step]: result }));
    setStep(next);
    void update({ ui: { [SETUP_KEY]: next } });
  };
  const nextOf = (s: SetupStep): SetupStep => STEPS[Math.min(STEPS.indexOf(s) + 1, STEPS.length - 1)];
  const skipAll = () => {
    void update({ ui: { [SETUP_KEY]: "done" } });
    onFinish();
  };

  const current = STEPS.indexOf(step);
  // A step passed in an earlier session: done if its job is done (a backup
  // exists, the addon is installed), skipped otherwise.
  const { list } = useBackups();
  const { status } = useAddon();
  const known: Partial<Record<SetupStep, Outcome>> = {
    find: hasFolder ? "done" : undefined,
    backup: list == null ? undefined : list.length > 0 ? "done" : "skipped",
    addon: status == null ? undefined : status.installed_version != null ? "done" : "skipped",
  };
  return (
    <Page>
      <div className="d-letter-wrap">
        <div className="ob-top">
          <ol className="d-stepper" aria-label="Setup">
            {LABELS.map((l, i) => {
              const o = outcome[l.step] ?? (i < current ? (known[l.step] ?? "done") : undefined);
              const cls = l.step === step ? "on" : o === "done" ? "done" : o === "skipped" ? "skipped" : undefined;
              return (
                <li key={l.step} className={cls}>
                  <span>{o === "done" ? <Check size={11} aria-label="done" /> : i + 1}</span>
                  {l.label} {l.optional && <i>optional</i>}
                </li>
              );
            })}
          </ol>
          {step !== "done" && (hasFolder || step !== "find") && (
            <button className="d-link ob-skip" title="Run it again from Settings › Game" onClick={skipAll}>
              {step === "find" ? "Skip setup" : "Skip the rest"}
            </button>
          )}
        </div>
        <Record>
          <div className="d-letter">
            {step === "find" && <FindGame install={install} onContinue={() => go("backup", "done")} />}
            {step === "backup" && (
              <BackupStep onDone={() => go("addon", "done")} onSkip={() => go("addon", "skipped")} />
            )}
            {step === "addon" && (
              <AddonStep onDone={() => go("extras", "done")} onSkip={() => go("extras", "skipped")} />
            )}
            {step === "extras" && (
              <ExtrasStep
                icons={settings?.item_icons ?? false}
                agents={settings?.agent_access ?? false}
                onIcons={(v) => update({ item_icons: v })}
                onAgents={(v) => update({ agent_access: v })}
                onFinish={() => go(nextOf("extras"), "done")}
              />
            )}
            {step === "done" && (
              <DoneStep
                outcome={outcome}
                characters={characters}
                icons={settings?.item_icons ?? false}
                agents={settings?.agent_access ?? false}
                onOpen={onFinish}
              />
            )}
          </div>
        </Record>
      </div>
    </Page>
  );
}

/** A ✓ or a spinner, a label, and a count on the right. */
function Row({ done, label, right }: { done: boolean; label: string; right?: string }) {
  return (
    <li>
      {done ? <Check size={14} className="ok" aria-label="done" /> : <span className="ob-spin" aria-label="working" />}
      <span className="w">{label}</span>
      <span className="r">{right}</span>
    </li>
  );
}

/** 2. A safety copy first: starts by itself, works with WoW open. A rerun
 *  never takes a second "first" backup. */
function BackupStep({ onDone, onSkip }: { onDone: () => void; onSkip: () => void }) {
  const { list, progress, failed, backUpNow } = useBackups();
  const started = useRef(false);
  const [had, setHad] = useState<boolean | null>(null);
  useEffect(() => {
    if (list == null || started.current) return;
    started.current = true;
    setHad(list.length > 0);
    if (list.length === 0) void backUpNow("First backup");
  }, [list, backUpNow]);

  const latest = [...(list ?? [])].sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
  const running = progress != null;
  const done = !running && !failed && latest != null;
  return (
    <>
      <h1>A safety copy first.</h1>
      <p className="d-muted">
        Before anything else, Forever Buddy copies your settings: account, characters, macros and addon
        settings. It's fine if WoW is open.
      </p>
      {had && latest ? (
        <ul className="ob-rows">
          <Row done label="Last backup" right={when(latest.created_at)} />
        </ul>
      ) : (
        <>
          <ul className="ob-rows">
            <Row done={done} label="Account settings" />
            <Row
              done={done}
              label={done && latest ? `${plural(latest.char_count, "character's", "characters'")} settings and macros` : "Characters' settings and macros"}
            />
            <Row
              done={done}
              label="Addon settings"
              right={done && latest ? plural(latest.addon_count, "addon", "addons") : undefined}
            />
          </ul>
          {running && progress.total > 0 && (
            <div className="ob-progress">
              <div className="d-meter">
                <i style={{ width: `${Math.round((progress.done / progress.total) * 100)}%` }} />
              </div>
              <div className="lbl">
                <span>Copying…</span>
                <span>
                  {progress.done.toLocaleString()} of {progress.total.toLocaleString()} files
                </span>
              </div>
            </div>
          )}
          {done && latest && (
            <p className="d-letter-status">
              <Check size={14} className="ok" aria-hidden /> {plural(latest.file_count, "file", "files")} copied.
            </p>
          )}
        </>
      )}
      {failed && (
        <p className="d-letter-bad">
          The backup didn't finish: {failed}
        </p>
      )}
      <div className="d-letter-acts">
        {failed ? (
          <PrimaryButton onClick={() => backUpNow("First backup")}>Retry</PrimaryButton>
        ) : (
          <PrimaryButton onClick={onDone} disabled={!done}>
            Continue
          </PrimaryButton>
        )}
        {failed && (
          <Button variant="ghost" onClick={onSkip}>
            Skip this step
          </Button>
        )}
      </div>
      <p className="d-letter-note">
        Your first backup is kept until you delete it. Every later backup is automatic when WoW closes.
      </p>
    </>
  );
}

/** 3. The companion addon: install, update, or not now. Writing it needs WoW
 *  closed, so with WoW running the button is locked and the step waits. */
function AddonStep({ onDone, onSkip }: { onDone: () => void; onSkip: () => void }) {
  const addon = useAddon();
  const game = useGameStatus();
  const s = addon.status;
  const installed = s?.installed_version ?? null;
  const current = installed != null && !s?.update_available;
  const running = game?.running === true;
  const label = installed ? "Update addon" : "Install addon";
  return (
    <>
      <h1>The companion addon.</h1>
      <p className="d-muted">A small addon of ours that fills in what the game doesn't write down.</p>
      <div className="d-letter-label">What it does</div>
      <ul className="ob-rows ob-what">
        <li>
          <Check size={14} className="ok" aria-hidden />
          <span className="w">Records at logout</span>
          <span className="r">gold, bags, bank, quests and what you gained</span>
        </li>
        <li>
          <Check size={14} className="ok" aria-hidden />
          <span className="w">Shows in game</span>
          <span className="r">your alts on item tooltips, a login briefing, your plan</span>
        </li>
        <li>
          <Check size={14} className="ok" aria-hidden />
          <span className="w">Display only</span>
          <span className="r">never plays, buys or sends anything</span>
        </li>
      </ul>
      {current && (
        <p className="d-letter-status">
          <Check size={14} className="ok" aria-hidden /> Installed, {installed}
        </p>
      )}
      {running && !current && (
        <p className="ob-ember">
          <AlertTriangle size={13} aria-hidden /> WoW is running. Close it to install: WoW only loads addons when it
          starts. This step waits for you.
        </p>
      )}
      {addon.error && <p className="d-letter-bad">{addon.error}</p>}
      <div className="d-letter-acts">
        {current ? (
          <PrimaryButton onClick={onDone}>Continue</PrimaryButton>
        ) : running ? (
          <LockedAction why="WoW is running. Close it to install the addon.">{label}</LockedAction>
        ) : (
          <PrimaryButton
            disabled={addon.busy || s == null}
            onClick={async () => {
              await addon.install();
              addon.refresh();
            }}
          >
            {addon.busy ? "Installing…" : label}
          </PrimaryButton>
        )}
        {!current && (
          <Button variant="ghost" onClick={onSkip}>
            Not now
          </Button>
        )}
      </div>
      {!current && s && (
        <p className="d-letter-note">
          Installs ForeverBuddy {s.bundled_version} into Interface\AddOns. A safety snapshot is taken first.
        </p>
      )}
    </>
  );
}

/** 4. Two extras, both off by default, in Settings' own words. */
function ExtrasStep({
  icons,
  agents,
  onIcons,
  onAgents,
  onFinish,
}: {
  icons: boolean;
  agents: boolean;
  onIcons: (v: boolean) => void;
  onAgents: (v: boolean) => void;
  onFinish: () => void;
}) {
  return (
    <>
      <h1>Two extras.</h1>
      <p className="d-muted">Both are off unless you turn them on, and both live in Settings afterwards.</p>
      <div className="ob-extra">
        <div>
          <div className="t">Show item icons from my game files</div>
          <div className="d">
            Reads icon pictures from your own WoW install. Off: items show their first letter, and the game's files
            aren't opened.
          </div>
        </div>
        <Switch checked={icons} onChange={onIcons} label="Show item icons from my game files" />
      </div>
      <div className="ob-extra">
        <div>
          <div className="t">Let AI agents read my characters and suggest plans</div>
          <div className="d">
            An agent like Claude Desktop can read your characters, gear, quests and prices. It can't change anything.
            Setup is in Settings › Agents.
          </div>
        </div>
        <Switch checked={agents} onChange={onAgents} label="Let AI agents read my characters and suggest plans" />
      </div>
      <div className="d-letter-acts">
        <PrimaryButton onClick={onFinish}>Finish</PrimaryButton>
      </div>
    </>
  );
}

/** "You're set.": what happened, and what was skipped or left off. */
function DoneStep({
  outcome,
  characters,
  icons,
  agents,
  onOpen,
}: {
  outcome: Partial<Record<SetupStep, Outcome>>;
  /** In the game folder's active flavor. */
  characters: number | null;
  icons: boolean;
  agents: boolean;
  onOpen: () => void;
}) {
  const line = (ok: boolean, label: string, note: string) => (
    <li className={ok ? undefined : "off"}>
      {ok ? <Check size={14} className="ok" aria-label="done" /> : <X size={14} aria-label="not done" />}
      <span className="w">{label}</span>
      <span className="r">{note}</span>
    </li>
  );
  // What's true now, not what this session clicked: a resumed setup didn't
  // see the earlier steps.
  const { list } = useBackups();
  const { status } = useAddon();
  const backup = (list?.length ?? 0) > 0 && outcome.backup !== "skipped";
  const addon = status?.installed_version != null;
  // The first backup: the oldest one kept.
  const first = [...(list ?? [])].sort((a, b) => a.created_at.localeCompare(b.created_at))[0];
  return (
    <>
      <h1>You're set.</h1>
      <ul className="ob-rows">
        {line(true, "Game folder", characters != null ? plural(characters, "character", "characters") : "found")}
        {line(
          backup,
          "First backup",
          backup && first ? `${plural(first.file_count, "file", "files")} · kept until you delete it` : "skipped",
        )}
        {line(addon, "Companion addon", addon ? "log in once on each character" : "skipped")}
        {line(icons, "Item icons", icons ? "on" : "off")}
        {line(agents, "AI agents", agents ? "on" : "off")}
      </ul>
      <div className="d-letter-acts">
        <PrimaryButton onClick={onOpen}>Open the dashboard</PrimaryButton>
      </div>
      <p className="d-letter-note">You can run this again any time from Settings › Game.</p>
    </>
  );
}
