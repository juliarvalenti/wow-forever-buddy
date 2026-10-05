import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Check, FileText, RotateCcw } from "lucide-react";
import {
  commands,
  events,
  type Category,
  type GameStatus,
  type RestoreMode,
  type RestoreSelection,
  type ScopeItem,
  type SnapshotDetail,
  type SnapshotSummary,
} from "@/lib/bindings";
import {
  Button,
  Callout,
  Checkbox,
  DataTable,
  Dialog,
  LiveDot,
  LockedAction,
  Meter,
  Page,
  PageHeader,
  Panel,
  PanelBody,
  PanelHeader,
  Pill,
  type PillKind,
  PrimaryButton,
  Segmented,
} from "@/components/d";
import { useBackups, useSnapshot } from "@/hooks/useBackups";
import { useEvent } from "@/hooks/useEvent";
import { useRestore } from "@/hooks/useRestore";
import { PlanDetails, planBlocked } from "@/screens/PlanDetails";
import { ago, bytes, plural, when, whenInline } from "@/lib/format";

// Copy from design/mocks/round-3/IMPLEMENTING.md §4.

const KIND_LABEL: Record<SnapshotSummary["kind"], string> = {
  Manual: "Manual",
  Auto: "Auto",
  Safety: "Safety",
};
const KIND_PILL: Record<SnapshotSummary["kind"], PillKind> = {
  Manual: "manual",
  Auto: "auto",
  Safety: "safety",
};
const AUTO_NOTE: Partial<Record<SnapshotSummary["trigger"], string>> = {
  app_start: "When the app started",
  game_exit: "On game exit",
  scheduled: "Scheduled",
};
const CATEGORY_LABEL: Record<Category, string> = {
  BindingsMacros: "Keybindings & macros",
  AddonSettings: "Addon settings",
  ChatLayout: "Chat & layout",
  Other: "Other files",
};
const RUNNING_WHY = "Unlocks when WoW closes. You can still pick what to restore.";
const PENDING_WHY = "Roll back or finish the interrupted restore first.";
const MISSING_WHY = "Unlocks when the game folder is found again.";

type Filter = "all" | SnapshotSummary["kind"];
type Scope = "everything" | "characters" | "addons";

function note(s: SnapshotSummary) {
  if (s.kind === "Auto") return <span>{AUTO_NOTE[s.trigger] ?? "Automatic"}</span>;
  if (s.kind === "Safety") return <span>{s.label ?? "Safety copy"}</span>;
  return (
    <span>
      {s.label ? <b>{s.label}</b> : "Manual backup"}
      {s.game_running && <span className="d-dim"> · taken mid-session</span>}
    </span>
  );
}

function contents(s: SnapshotSummary) {
  const parts = [];
  if (s.char_count > 0) parts.push(plural(s.char_count, "char", "chars"));
  if (s.addon_count > 0) parts.push(plural(s.addon_count, "addon", "addons"));
  if (parts.length === 0) parts.push(plural(s.file_count, "file", "files"));
  return parts.join(" · ");
}

// Selection keys: "all", "addon|<name>", "acct|<account>|<category>",
// "char|<account>|<realm>|<character>|<category>".
type Keys = Set<string>;

function toSelection(keys: Keys): RestoreSelection {
  if (keys.has("all")) return { items: [{ kind: "Everything" }] };
  const items: ScopeItem[] = [];
  const accounts = new Map<string, Category[]>();
  const chars = new Map<string, { a: string; r: string; c: string; cats: Category[] }>();
  for (const k of keys) {
    const p = k.split("|");
    if (p[0] === "addon") {
      items.push({ kind: "AddonData", addon: p[1], target: { kind: "Everywhere" } });
    } else if (p[0] === "acct") {
      accounts.set(p[1], [...(accounts.get(p[1]) ?? []), p[2] as Category]);
    } else if (p[0] === "char") {
      const id = p.slice(1, 4).join("|");
      const e = chars.get(id) ?? { a: p[1], r: p[2], c: p[3], cats: [] };
      e.cats.push(p[4] as Category);
      chars.set(id, e);
    }
  }
  for (const [account, categories] of accounts) items.push({ kind: "Account", account, categories });
  for (const e of chars.values())
    items.push({
      kind: "Character",
      account: e.a,
      realm: e.r,
      character: e.c,
      categories: e.cats,
    });
  return { items };
}

/** Button and dialog title for the current selection, and `who` when it's
 *  exactly one character ("Thrandor's keybindings… will be replaced"). */
function restoreLabel(keys: Keys): { button: string; title: string; who?: string } {
  if (keys.has("all")) return { button: "Restore everything…", title: "Restore everything?" };
  const chars = new Set(
    [...keys].filter((k) => k.startsWith("char|")).map((k) => k.split("|").slice(1, 4).join("|")),
  );
  const addons = [...keys].filter((k) => k.startsWith("addon|"));
  if (chars.size > 0 && addons.length === 0 && ![...keys].some((k) => k.startsWith("acct|"))) {
    const only = [...chars][0].split("|")[2];
    return {
      button: `Restore ${plural(chars.size, "character", "characters")}…`,
      title: chars.size === 1 ? `Restore ${only}?` : `Restore ${chars.size} characters?`,
      who: chars.size === 1 ? only : undefined,
    };
  }
  if (addons.length > 0 && chars.size === 0)
    return {
      button: `Restore ${plural(addons.length, "addon", "addons")}…`,
      title: "Restore these addon settings?",
    };
  return { button: "Restore selection…", title: "Restore these settings?" };
}

function SnapshotTree({
  detail,
  scope,
  keys,
  setKeys,
}: {
  detail: SnapshotDetail;
  scope: Scope;
  keys: Keys;
  setKeys: (k: Keys) => void;
}) {
  const toggle = (ks: string[], on: boolean) => {
    const next = new Set(keys);
    next.delete("all");
    for (const k of ks) (on ? next.add(k) : next.delete(k));
    setKeys(next);
  };
  const size = (b: number | null) => <span className="size">{bytes(b)}</span>;

  if (scope === "everything")
    return (
      <div className="d-tree">
        <Checkbox checked={keys.has("all")} onChange={(on) => setKeys(new Set(on ? ["all"] : []))}>
          Everything in this snapshot {size(detail.summary.total_bytes)}
        </Checkbox>
      </div>
    );

  if (scope === "addons")
    return (
      <div className="d-tree">
        {detail.addons.length === 0 && <p className="d-muted">No addon settings in this snapshot.</p>}
        {detail.addons.map((a) => {
          const k = `addon|${a.name}`;
          return (
            <Checkbox key={k} checked={keys.has(k)} onChange={(on) => toggle([k], on)}>
              {a.name} {size(a.totals.bytes)}
            </Checkbox>
          );
        })}
      </div>
    );

  return (
    <div className="d-tree">
      {detail.accounts.map((acct) => (
        <div key={acct.name}>
          {acct.characters.map((ch) => {
            const base = `char|${acct.name}|${ch.realm}|${ch.name}`;
            const ks = ch.categories.map((c) => `${base}|${c.category}`);
            const all = ks.every((k) => keys.has(k));
            return (
              <div key={base}>
                <Checkbox checked={all} onChange={(on) => toggle(ks, on)}>
                  <b>{ch.name}</b> <span className="d-dim">{ch.realm}</span> {size(ch.totals.bytes)}
                </Checkbox>
                <div className="indent">
                  {ch.categories.map((c) => {
                    const k = `${base}|${c.category}`;
                    const count =
                      c.category === "AddonSettings" ? ` (${c.totals.files.toLocaleString()})` : "";
                    return (
                      <Checkbox key={k} checked={keys.has(k)} onChange={(on) => toggle([k], on)}>
                        {CATEGORY_LABEL[c.category]}
                        {count} {size(c.totals.bytes)}
                      </Checkbox>
                    );
                  })}
                </div>
              </div>
            );
          })}
          {acct.categories.length > 0 && (
            <div>
              <span className="d-muted">Account-wide ({acct.name})</span>
              <div className="indent">
                {acct.categories.map((c) => {
                  const k = `acct|${acct.name}|${c.category}`;
                  return (
                    <Checkbox key={k} checked={keys.has(k)} onChange={(on) => toggle([k], on)}>
                      {CATEGORY_LABEL[c.category]} {size(c.totals.bytes)}
                    </Checkbox>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

/** "…\Thrandor\SavedVariables\Details.lua": the last three parts, Windows-style. */
function shortFile(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts.length > 3 ? `…\\${parts.slice(-3).join("\\")}` : parts.join("\\");
}

/** How many older snapshots to check for an intact copy before giving up. */
const OLDER_TRIES = 5;

/** The restore stopped because the snapshot's stored copies don't check out
 *  (backups.html?error=corrupt). Offers the newest older full snapshot that
 *  verifies, and a check of every snapshot. Nothing was changed. */
function DamagedSnapshot({
  id,
  files,
  snapshots,
  onUse,
  onClose,
}: {
  id: string;
  files: string[];
  snapshots: SnapshotSummary[];
  onUse: (olderId: string) => void;
  onClose: () => void;
}) {
  const snap = snapshots.find((s) => s.id === id);
  const candidates = useMemo(
    () =>
      snapshots
        .filter((s) => snap && s.scope === "full" && s.flavor === snap.flavor && s.created_at < snap.created_at)
        .sort((a, b) => b.created_at.localeCompare(a.created_at))
        .slice(0, OLDER_TRIES),
    [snapshots, snap],
  );

  // undefined while looking; null if none of the tries checked out.
  const [older, setOlder] = useState<SnapshotSummary | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    (async () => {
      for (const c of candidates) {
        const report = await commands.backupVerify(c.id).catch(() => null);
        if (report && report.corrupt.length === 0) {
          if (live) setOlder(c);
          return;
        }
      }
      if (live) setOlder(null);
    })();
    return () => {
      live = false;
    };
  }, [candidates]);

  const [verify, setVerify] = useState<{ done: number; bad: SnapshotSummary[] } | null>(null);
  const verifying = verify != null && verify.done < snapshots.length;
  const verifyAll = async () => {
    const bad: SnapshotSummary[] = [];
    setVerify({ done: 0, bad });
    for (const [i, s] of snapshots.entries()) {
      const report = await commands.backupVerify(s.id).catch(() => null);
      if (!report || report.corrupt.length > 0) bad.push(s);
      setVerify({ done: i + 1, bad: [...bad] });
    }
  };

  return (
    <Dialog
      title="This snapshot is damaged"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={verifyAll} disabled={verifying}>
            Verify all snapshots
          </Button>
          <span className="d-grow" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          {older === undefined && candidates.length > 0 && (
            <span className="d-muted">Looking for an older copy…</span>
          )}
          {older && (
            <Button onClick={() => onUse(older.id)}>
              <RotateCcw size={13} aria-hidden /> Use {whenInline(older.created_at)} instead
            </Button>
          )}
        </>
      }
    >
      <p>
        We checked {snap ? <b>{whenInline(snap.created_at)}</b> : "this snapshot"} before restoring, and{" "}
        {files.length === 1 ? "1 of its files doesn't" : `${files.length} of its files don't`} match
        what was saved. The backup copy on disk has changed or is damaged.
      </p>
      <ul className="d-files dmg">
        {files.map((f) => (
          <li key={f} title={f}>
            <FileText size={13} aria-hidden />
            {shortFile(f)}
            <span className="h">damaged</span>
          </li>
        ))}
      </ul>
      <div className="d-okline">
        <Check size={14} aria-hidden />
        <span>Nothing was changed. We stop before writing a single file.</span>
      </div>
      {verify && (
        <p className="d-muted">
          {verifying
            ? `Checking snapshots… ${verify.done} of ${snapshots.length}`
            : verify.bad.length === 0
              ? `All ${plural(snapshots.length, "snapshot checks", "snapshots check")} out.`
              : `Checked ${snapshots.length}: ${plural(verify.bad.length, "is", "are")} damaged (${verify.bad
                  .map((s) => whenInline(s.created_at))
                  .join(", ")}).`}
        </p>
      )}
      {older === null && (
        <p className="d-muted">
          {candidates.length === 0
            ? "There's no older full snapshot to fall back to."
            : candidates.length === 1
              ? "The one older snapshot is damaged too."
              : `The ${candidates.length} older snapshots we checked are damaged too. Pick another from the list to try.`}
        </p>
      )}
    </Dialog>
  );
}

function ConfirmRestore({
  id: chosen,
  keys,
  mode,
  running,
  snapshots,
  onClose,
}: {
  id: string;
  keys: Keys;
  mode: RestoreMode;
  running: boolean;
  /** All snapshots, newest first: for dates and an older one to fall back to. */
  snapshots: SnapshotSummary[];
  onClose: () => void;
}) {
  const { plan, planError, loading, changed, run, preview, start, reset } = useRestore();
  // Starts as the snapshot the user picked; "Use <older> instead" switches it
  // when that one is damaged, keeping the same selection.
  const [id, setId] = useState(chosen);
  const selection = useMemo(() => toSelection(keys), [keys]);
  const again = useCallback(() => preview(id, selection, mode), [id, selection, mode, preview]);
  const { title, who } = restoreLabel(keys);
  const takenAt = snapshots.find((s) => s.id === id)?.created_at;

  // A plan from before WoW's exit writes is stale. When WoW closes, re-plan
  // and keep Restore locked until that fresh plan is in ("Checking what
  // changed…"). Don't wait for backup-created: it never comes when exit
  // backups are off or skipped as identical. If it does come, re-plan again.
  const [checking, setChecking] = useState(false);
  const wasRunning = useRef(running);
  useEffect(() => {
    again();
  }, [again]);
  useEffect(() => {
    if (wasRunning.current && !running) {
      setChecking(true);
      again().finally(() => setChecking(false));
    }
    wasRunning.current = running;
  }, [running, again]);
  useEvent(events.backupCreated, () => {
    if (run.kind !== "running") again();
  });
  // The backend refused because more would be deleted than confirmed: show
  // the new list for the user to check again.
  useEffect(() => {
    if (run.kind === "error" && run.deletionsChanged) again();
  }, [run, again]);

  if (run.kind === "error" && run.corrupt)
    return (
      <DamagedSnapshot
        id={id}
        files={run.corrupt}
        snapshots={snapshots}
        onUse={(older) => {
          reset();
          setId(older);
        }}
        onClose={onClose}
      />
    );

  if (run.kind === "done")
    return (
      <Dialog title="Restored" onClose={onClose} footer={<PrimaryButton onClick={onClose}>Done</PrimaryButton>}>
        <p>
          Restored {run.report.summary}.{" "}
          {run.report.pre_restore_snapshot &&
            "The files it replaced are in a safety snapshot, so you can undo this."}
        </p>
      </Dialog>
    );

  const busy = run.kind === "running";
  const footer = (
    <>
      {running || checking ? (
        <>
          <LiveDot />
          {running ? (
            <span>
              Waiting for WoW to close…{" "}
              <span className="d-muted">Restore unlocks once WoW closes and the list is checked again.</span>
            </span>
          ) : (
            <span>WoW closed. Checking what changed…</span>
          )}
          <span className="d-grow" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <LockedAction why={RUNNING_WHY}>Restore</LockedAction>
        </>
      ) : (
        <>
          {busy && (
            <span className="d-muted">
              Restoring… {run.total > 0 && `${run.done} of ${run.total}`}
            </span>
          )}
          <span className="d-grow" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <PrimaryButton
            onClick={() => plan && start(id, selection, mode, plan)}
            disabled={busy || loading || planBlocked(plan)}
          >
            Restore
          </PrimaryButton>
        </>
      )}
    </>
  );

  return (
    <Dialog title={title} onClose={busy ? undefined : onClose} footer={footer}>
      {id !== chosen && (
        <p className="d-muted">
          From the copy taken {whenInline(snapshots.find((s) => s.id === id)?.created_at ?? "")}, since the
          one you picked is damaged.
        </p>
      )}
      {planError && <Callout tone="bad">{planError}</Callout>}
      {run.kind === "error" && (
        <Callout tone="bad">
          {run.deletionsChanged
            ? "Restore stopped before changing anything. More files would be removed than you confirmed."
            : run.message}
        </Callout>
      )}
      {!plan && !planError && <p className="d-muted">Working out what changes…</p>}
      {plan && (
        <>
          <PlanDetails
            plan={plan}
            lead={
              plan.write_count > 0 && takenAt ? (
                <>
                  {who ? `${who}'s ` : ""}
                  {/* The summary's "; removes N" part is listed separately below. */}
                  <b>{plan.summary.split(";")[0]}</b> will be replaced with the copy from{" "}
                  <b>{whenInline(takenAt)}</b>.
                </>
              ) : undefined
            }
          />
          <p className="d-muted">
            A safety snapshot of the current files is taken before anything changes, so you can
            undo this.
          </p>
          {changed && (
            <p style={{ color: "var(--ember-2)" }}>
              The list changed after WoW closed. Please check it again.
            </p>
          )}
        </>
      )}
    </Dialog>
  );
}

export function Backups({
  game,
  restoresLocked,
  folderMissing = null,
  onCheckFolder,
  select,
  show,
}: {
  game: GameStatus | null;
  /** An interrupted restore is unresolved: restores stay locked. */
  restoresLocked: boolean;
  /** The saved game folder can't be found (why). Backups stay viewable;
   *  restores and new backups are locked until it's back or re-chosen. */
  folderMissing?: string | null;
  onCheckFolder?: () => void;
  /** A snapshot to open (e.g. "Open the safety copy"). */
  select?: string | null;
  /** A list filter to apply (e.g. "Open Backups" on the safety copies). */
  show?: Filter | null;
}) {
  const { list, storage, error, progress, failed, autoFailed, backUpNow } = useBackups();
  // > 0: the automatic backup was taken but left files out (a warning).
  const autoSkipped = autoFailed?.skipped ?? 0;
  const [filter, setFilter] = useState<Filter>(show ?? "all");
  const [selected, setSelected] = useState<string | null>(select ?? null);
  const [scope, setScope] = useState<Scope>("characters");
  const [keys, setKeys] = useState<Keys>(new Set());
  const [mirror, setMirror] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const { detail, error: detailError } = useSnapshot(selected);
  // Can't tell (the process list failed) locks restores like running does.
  const unknown = game?.unknown ?? false;
  const running = (game?.running ?? false) || unknown;

  useEffect(() => {
    if (select) setSelected(select);
  }, [select]);
  useEffect(() => {
    if (show) setFilter(show);
  }, [show]);
  useEffect(() => setKeys(new Set()), [selected, scope]);
  // A restore that failed partway hands over to the recovery dialog.
  useEffect(() => {
    if (restoresLocked) setConfirming(false);
  }, [restoresLocked]);

  const counts = useMemo(() => {
    const c = { all: 0, Auto: 0, Manual: 0, Safety: 0 };
    for (const s of list ?? []) {
      c.all++;
      c[s.kind]++;
    }
    return c;
  }, [list]);
  const shown = (list ?? []).filter((s) => filter === "all" || s.kind === filter);
  const { button } = restoreLabel(keys);
  const canPick = keys.size > 0;

  const restoreAction = (label: string, onClick: () => void, variant?: "ghost") =>
    folderMissing != null ? (
      <LockedAction why={MISSING_WHY}>{label}</LockedAction>
    ) : restoresLocked ? (
      <LockedAction why={PENDING_WHY}>{label}</LockedAction>
    ) : running ? (
      <LockedAction why={RUNNING_WHY}>{label}</LockedAction>
    ) : (
      <Button variant={variant} onClick={onClick}>
        {label}
      </Button>
    );

  return (
    <Page>
      <PageHeader
        title="Backups"
        lede="Snapshots of your WTF and SavedVariables, taken every time the game closes."
        actions={
          folderMissing != null ? (
            <LockedAction why={MISSING_WHY}>Back up now</LockedAction>
          ) : (
            <PrimaryButton onClick={() => backUpNow()} disabled={progress != null}>
              {progress
                ? `Backing up… ${progress.total > 0 ? `${progress.done} of ${progress.total}` : ""}`
                : "Back up now"}
            </PrimaryButton>
          )
        }
      />

      {folderMissing != null ? (
        <Callout tone="ember">
          <span>
            <b>We can't find your game folder, so restores and new backups are paused.</b> Your
            backups are safe and you can still look through them.{" "}
            <span className="d-dim">({folderMissing})</span>
          </span>
          <span className="d-grow" />
          {onCheckFolder && <Button onClick={onCheckFolder}>Check game folder</Button>}
        </Callout>
      ) : unknown ? (
        <Callout tone="ember">
          <LiveDot />
          <span>
            <b>Can't tell if WoW is running, so restores are locked.</b> Backing up still works. This
            usually clears up on its own within a few seconds.
          </span>
        </Callout>
      ) : running ? (
        <Callout tone="ember">
          <LiveDot />
          <span>
            <b>WoW is running, so restores are locked.</b> Backing up is safe now. Close the game to
            restore; WoW rewrites these files when it exits.
          </span>
        </Callout>
      ) : failed ? (
        <Callout tone="bad">
          <span>
            <b>That backup didn't finish.</b> {failed}
          </span>
          <span className="d-grow" />
          <Button onClick={() => backUpNow()}>Retry</Button>
        </Callout>
      ) : null}
      {autoFailed && !failed && (
        <Callout tone={autoSkipped > 0 ? "ember" : "bad"}>
          <span>
            <b>
              {autoSkipped > 0
                ? `The last automatic backup left out ${plural(autoSkipped, "file", "files")}`
                : "The last automatic backup didn't finish"}
            </b>{" "}
            ({AUTO_NOTE[autoFailed.trigger]?.toLowerCase() ?? "automatic"}, {when(autoFailed.at)}).{" "}
            {autoFailed.error}
          </span>
          <span className="d-grow" />
          {folderMissing == null && <Button onClick={() => backUpNow()}>Back up now</Button>}
        </Callout>
      )}
      {error && <Callout tone="bad">{error}</Callout>}

      <div className="d-toolbar">
        <Segmented<Filter>
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: `All ${counts.all}` },
            { value: "Auto", label: `Auto ${counts.Auto}` },
            { value: "Manual", label: `Manual ${counts.Manual}` },
            { value: "Safety", label: `Safety ${counts.Safety}` },
          ]}
        />
        {storage && (
          <div className="d-storage">
            {/* Verbatim from the backend: it's generated from the real policy. */}
            <span className="retention">{storage.retention_summary}</span>
            {storage.budget_bytes != null && storage.used_bytes != null && (
              <span className="usage">
                <b>{bytes(storage.used_bytes)}</b> of {bytes(storage.budget_bytes)}
                <Meter
                  fraction={storage.used_bytes / storage.budget_bytes}
                  over={storage.over_budget}
                />
              </span>
            )}
          </div>
        )}
      </div>
      {storage?.over_budget && (
        <p className="d-dim">Over budget: only manual, pinned and the newest few backups are left.</p>
      )}
      {storage?.cleanup_blocked && (
        <Callout tone="ember">
          <span>
            <b>Old backups aren't being cleaned up</b> because one backup's record is damaged, so
            it isn't safe to tell which stored files are still needed. Your backups are untouched.{" "}
            <span className="d-dim">({storage.cleanup_blocked})</span>
          </span>
        </Callout>
      )}

      <div style={{ display: "grid", gridTemplateColumns: selected ? "minmax(0, 1fr) 360px" : "1fr", gap: 16 }}>
        <Panel>
          {list && list.length === 0 && (
            <PanelBody>
              <p className="d-muted">No snapshots yet. The first one is taken when the game closes.</p>
            </PanelBody>
          )}
          {shown.length > 0 && (
            <DataTable
              className={selected ? "with-panel" : undefined}
              head={
                <tr>
                  <th>When</th>
                  <th>Type</th>
                  <th>Note</th>
                  {/* Hidden beside the snapshot panel only in the narrow layout (d.css). */}
                  <th className="col-contents">Contents</th>
                  <th className="num">Size</th>
                  <th />
                </tr>
              }
            >
              {shown.map((s) => (
                <tr key={s.id} className={s.id === selected ? "sel" : ""} onClick={() => setSelected(s.id)}>
                  <td className="nowrap">
                    {when(s.created_at)}
                    <div className="d-dim">{ago(s.created_at)}</div>
                  </td>
                  <td>
                    <Pill kind={KIND_PILL[s.kind]}>{KIND_LABEL[s.kind]}</Pill>
                  </td>
                  <td>{note(s)}</td>
                  {/* Not "contents": that's Tailwind's display: contents utility. */}
                  <td className="col-contents d-muted nowrap">{contents(s)}</td>
                  <td className="num">{bytes(s.total_bytes)}</td>
                  <td onClick={(e) => e.stopPropagation()}>
                    {restoreAction("Restore", () => setSelected(s.id), "ghost")}
                  </td>
                </tr>
              ))}
            </DataTable>
          )}
        </Panel>

        {selected && (
          <Panel>
            <PanelHeader title={detail ? when(detail.summary.created_at) : "Snapshot"}>
              <span className="d-grow" />
              <Button variant="ghost" onClick={() => setSelected(null)}>
                Close
              </Button>
            </PanelHeader>
            <PanelBody>
              {detailError && (
                <Callout tone="bad">
                  <span>
                    <b>This snapshot couldn't be opened.</b> {detailError}
                  </span>
                </Callout>
              )}
              {!detail && !detailError && <p className="d-muted">Loading…</p>}
              {detail && (
                <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                  <p className="d-muted">
                    {detail.summary.game_running ? "Taken while WoW ran" : "Taken when WoW was closed"} ·{" "}
                    {bytes(detail.summary.total_bytes)} · {plural(detail.summary.file_count, "file", "files")}
                  </p>
                  {detail.skipped.length > 0 && (
                    <Callout tone="ember">
                      <span>
                        <b>{plural(detail.skipped.length, "file was", "files were")} left out</b> because{" "}
                        {detail.skipped.length === 1 ? "it" : "they"} couldn't be read:{" "}
                        {detail.skipped.map((f) => f.path).join(", ")}
                      </span>
                    </Callout>
                  )}
                  <Segmented<Scope>
                    value={scope}
                    onChange={setScope}
                    options={[
                      { value: "everything", label: "Everything" },
                      { value: "characters", label: "Characters" },
                      { value: "addons", label: "Addons" },
                    ]}
                  />
                  <SnapshotTree detail={detail} scope={scope} keys={keys} setKeys={setKeys} />
                  <Checkbox checked={mirror} onChange={setMirror}>
                    Also remove files that aren't in this snapshot
                  </Checkbox>
                  <div>
                    {canPick ? (
                      restoreAction(button, () => setConfirming(true))
                    ) : (
                      <span className="d-muted">Pick what to restore.</span>
                    )}
                    {canPick && running && !restoresLocked && folderMissing == null && (
                      <p className="d-dim">{RUNNING_WHY}</p>
                    )}
                  </div>
                </div>
              )}
            </PanelBody>
          </Panel>
        )}
      </div>

      {confirming && selected && (
        <ConfirmRestore
          id={selected}
          keys={keys}
          mode={mirror ? "mirror" : "overlay"}
          running={running}
          snapshots={list ?? []}
          onClose={() => setConfirming(false)}
        />
      )}
    </Page>
  );
}
