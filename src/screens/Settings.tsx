import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, KeyRound, Lock } from "lucide-react";
import type { AhStatus, AppInfo, IconCacheStatus, IntegrationId, StorageInfo } from "@/lib/bindings";
import { commands, events } from "@/lib/bindings";
import {
  Button,
  Meter,
  Page,
  PageHeader,
  Panel,
  PanelHeader,
  Pill,
  type PillKind,
  PrimaryButton,
  Switch,
} from "@/components/d";
import { useAhStatus } from "@/hooks/useAh";
import { useBackups } from "@/hooks/useBackups";
import { useEvent } from "@/hooks/useEvent";
import type { useInstall } from "@/hooks/useInstall";
import { useAppInfo, useSecrets, useSettings } from "@/hooks/useSettings";
import { bytes, errorText, plural } from "@/lib/format";

// Copy and layout from design/mocks/round-3/settings.html (F1).

/** The backend's folder inside a picked backup location (state.rs
 *  STORE_FOLDER): the store only ever sees a folder it owns. */
const STORE_FOLDER = "WoW Forever Buddy backups";

function join(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : `${dir}${sep}${name}`;
}

/** Where the OS keeps the keys, in its own words. */
function keyStore(): string {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "Windows Credential Manager";
  if (/Mac/i.test(ua)) return "the macOS Keychain";
  return "your system keyring";
}

/** "Automatic: …. Safety: …. Manual and pinned: …" (retention.rs summary)
 *  as rows; anything else is shown whole. */
function keepRows(summary: string): { k: string; v: string }[] | null {
  const parts = summary.split(/(?<=\.)\s+(?=[A-Z][A-Za-z ]+:)/);
  const rows = parts.map((p) => {
    const i = p.indexOf(": ");
    return i > 0 ? { k: p.slice(0, i), v: p.slice(i + 2).replace(/\.$/, "") } : null;
  });
  return rows.every((r) => r != null) && rows.length > 1 ? (rows as { k: string; v: string }[]) : null;
}

type Service = {
  mark: string;
  name: string;
  what: string;
  keys: { id: IntegrationId; placeholder: string }[];
  caveat?: string;
};

// wago_io (WeakAuras) is left out: WoW: Forever has no WeakAuras.
const SERVICES: Service[] = [
  {
    mark: "CF",
    name: "CurseForge",
    what: "Search and update addons hosted on CurseForge.",
    keys: [{ id: "curseforge", placeholder: "Paste API key" }],
  },
  {
    mark: "WA",
    name: "Wago Addons",
    what: "Update addons distributed through Wago.",
    keys: [{ id: "wago", placeholder: "Paste API key" }],
  },
  {
    mark: "GH",
    name: "GitHub token",
    what: "Higher rate limits when updating GitHub-hosted addons.",
    keys: [{ id: "github", placeholder: "Paste token" }],
  },
  {
    mark: "B",
    name: "Battle.net",
    what: "Armory portraits and character data. Create a client at develop.battle.net.",
    keys: [
      { id: "battlenet_client_id", placeholder: "Client ID" },
      { id: "battlenet_client_secret", placeholder: "Client secret" },
    ],
    caveat: "May not cover WoW: Forever realms yet. We'll test with your character before relying on it.",
  },
];

/** F5d: where the Auction House screen comes from, and why it's hidden
 *  until an Auctionator file has given prices on this machine. */
function AhNote({ status }: { status: AhStatus | null }) {
  if (!status) return null;
  return (
    <div className="st-set full">
      <div className="t">Auction House prices</div>
      <div className="d">
        {status.has_prices ? (
          <>
            <span className="ok">✓</span> From Auctionator · {plural(status.items, "price", "prices")}
          </>
        ) : status.file === "unreadable" ? (
          "Auctionator's saved prices are in a format this version can't read yet, so the Auction House screen and net worth stay hidden."
        ) : status.file === "read" ? (
          "Auctionator hasn't saved any prices yet. Scan at the Auction House in-game, then log out, and the Auction House screen and net worth appear."
        ) : (
          "The Auction House screen and net worth appear once Auctionator has saved prices. Install Auctionator, scan at the Auction House in-game, then log out."
        )}
      </div>
    </div>
  );
}

export function Settings({
  install,
  onOpenGameFolder,
}: {
  install: ReturnType<typeof useInstall>;
  onOpenGameFolder: () => void;
}) {
  const { settings, error, update, reload } = useSettings();
  const { storage, refresh: refreshBackups } = useBackups();
  const info = useAppInfo();
  const secrets = useSecrets();
  const ah = useAhStatus();

  const active = install.state.kind === "ok" ? install.state.install : null;
  const flavor = active?.flavors.find((f) => f.id === active.active);
  const backup = settings?.backup;
  const hours = backup?.schedule_hours ?? 24;
  const connected = SERVICES.filter((s) => s.keys.every((k) => secrets.isSet(k.id))).length;

  return (
    <Page>
      <PageHeader
        title="Settings"
        lede="Everything stays on this PC. Integrations are optional extras."
        actions={<span className="d-dim" style={{ fontSize: 11.5 }}>Changes save automatically</span>}
      />
      {error && <p className="d-letter-bad" style={{ color: "var(--bad)", margin: "0 0 12px" }}>{error}</p>}

      <section className="st-cols">
        <div className="d-stack">
          <Panel>
            <PanelHeader title="Game" />
            <div className="st-set full">
              <div className="t">World of Warcraft folder</div>
              <div className="ctl">
                <PathField
                  path={active?.root ?? (install.state.kind === "invalid" ? "Missing" : "Not set")}
                  info={info}
                  icon
                />
                <Button onClick={onOpenGameFolder}>Change…</Button>
              </div>
              <div className="d" style={{ marginTop: 2 }}>
                {flavor ? (
                  <>
                    <span className="ok">✓</span>{" "}
                    {[
                      flavor.label,
                      flavor.version,
                      flavor.exe?.split(/[\\/]/).pop(),
                      plural(flavor.characters, "character", "characters"),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </>
                ) : install.state.kind === "invalid" ? (
                  <span className="err">{install.state.error}</span>
                ) : (
                  "Choose it on the Game folder screen."
                )}
              </div>
            </div>
            <AhNote status={ah} />
            <GameDataCache />
          </Panel>

          <Panel>
            <PanelHeader title="Backups" />
            <SwitchRow
              title="Back up when WoW closes"
              desc="The safest moment: files are fully written."
              checked={backup?.on_game_exit ?? true}
              disabled={!settings}
              onChange={(v) => update({ backup: { on_game_exit: v } })}
            />
            <SwitchRow
              title="Back up when the app starts"
              desc="If it's been more than 6 hours. Waits for game exit if WoW is running."
              checked={backup?.on_app_start ?? true}
              disabled={!settings}
              onChange={(v) => update({ backup: { on_app_start: v } })}
            />
            <SwitchRow
              title={`Back up every ${hours > 0 ? hours : 24} hours`}
              desc="While the app is open. Skipped while WoW runs; the game-exit backup covers it."
              checked={hours > 0}
              disabled={!settings}
              onChange={(v) => update({ backup: { schedule_hours: v ? 24 : 0 } })}
            />
            <SwitchRow
              title="Include the AddOns folder"
              desc="Addon files themselves, not just their settings. Larger backups."
              checked={backup?.include_addons ?? false}
              disabled={!settings}
              onChange={(v) => update({ backup: { include_addons: v } })}
            />
            <Keep storage={storage} onPruned={refreshBackups} />
            <StoreLocation
              picked={backup?.location ?? null}
              ready={settings != null}
              info={info}
              storage={storage}
              onMoved={() => {
                reload();
                refreshBackups();
              }}
            />
          </Panel>

          <Panel>
            <PanelHeader title="Safety" />
            <div className="st-set">
              <div className="t">Block writes while WoW is running</div>
              <div className="d">
                WoW overwrites WTF and SavedVariables on exit, so edits made now would be lost or
                corrupted.
              </div>
              <div className="ctl">
                <span className="st-locked">
                  <Lock size={12} aria-hidden />
                  Always on
                </span>
                <Switch checked locked label="Block writes while WoW is running" />
              </div>
            </div>
          </Panel>
        </div>

        <div className="d-stack">
          <Panel>
            <PanelHeader title="Integrations">
              <span className="d-grow" />
              <span className="d-dim">
                {connected} of {SERVICES.length} set up
              </span>
            </PanelHeader>
            <div className="st-note">
              <KeyRound size={14} aria-hidden />
              <span>
                <b>Every key is optional.</b> The app works fully offline without them. Keys are
                stored in <b>{keyStore()}</b>, never in app files or backups.
              </span>
            </div>
            {SERVICES.map((s) => (
              <ServiceRow key={s.mark} s={s} secrets={secrets} />
            ))}
          </Panel>

          <Panel>
            <PanelHeader title="About" />
            <div className="st-set full">
              <div className="t">Forever Buddy{info ? ` ${info.version}` : ""}</div>
              <div className="d">
                Not affiliated with or endorsed by Blizzard Entertainment. World of Warcraft and
                Blizzard Entertainment are trademarks or registered trademarks of Blizzard
                Entertainment, Inc.
              </div>
            </div>
          </Panel>
        </div>
      </section>
    </Page>
  );
}

function parent(p: string): string {
  return p.replace(/[\\/][^\\/]*[\\/]?$/, "");
}

/** Known folders the way the OS names them: %LOCALAPPDATA% and %APPDATA% on
 *  Windows (the parents of the app's own folders), ~ elsewhere. */
function collapse(p: string, info: AppInfo | null): string {
  if (!info) return p;
  const { local_data_dir, config_dir } = info.paths;
  const bases: [string, string][] = local_data_dir.includes("\\")
    ? [
        [parent(local_data_dir), "%LOCALAPPDATA%"],
        [parent(config_dir), "%APPDATA%"],
      ]
    : [[local_data_dir.split("/Library/")[0], "~"]];
  for (const [base, name] of bases) {
    if (base && p.toLowerCase().startsWith(base.toLowerCase())) return name + p.slice(base.length);
  }
  return p;
}

/** A path in a field, cut at the start when it doesn't fit so the folder
 *  that matters (the end) stays visible. The full path is the tooltip. */
function PathField({ path, info, icon }: { path: string; info: AppInfo | null; icon?: boolean }) {
  return (
    <span className="d-field d-mono" title={path}>
      {icon && <FolderOpen size={14} aria-hidden style={{ color: "var(--soot)", flexShrink: 0 }} />}
      <span className="st-path">
        <bdi>{collapse(path, info)}</bdi>
      </span>
    </span>
  );
}

/** "Store backups in": pick a folder, confirm, and the backups move there
 *  (backup_move_location: copied, checked, then the old folder removed). */
function StoreLocation({
  picked,
  ready,
  info,
  storage,
  onMoved,
}: {
  picked: string | null;
  ready: boolean;
  info: AppInfo | null;
  storage: StorageInfo | null;
  onMoved: () => void;
}) {
  const here = picked ? join(picked, STORE_FOLDER) : info ? join(info.paths.local_data_dir, "backups") : "…";
  // A picked folder waiting for "Move backups"; null as `to` is the default.
  const [confirm, setConfirm] = useState<{ to: string | null } | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [moving, setMoving] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  useEvent(events.moveProgress, (p) => setProgress(p));
  // The row sits low on the page: bring the confirm, the progress or the
  // result into view as each appears (design nit on #64).
  const row = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (confirm || moving || result) row.current?.scrollIntoView({ block: "nearest" });
  }, [confirm, moving, result]);

  const target = (to: string | null) =>
    to ? join(to, STORE_FOLDER) : info ? join(info.paths.local_data_dir, "backups") : "the default folder";
  const size = storage?.used_bytes ? bytes(storage.used_bytes) : null;

  const pick = async () => {
    setResult(null);
    const path = await open({ directory: true, multiple: false });
    if (typeof path === "string") setConfirm({ to: path });
  };
  const move = async (to: string | null) => {
    setConfirm(null);
    setMoving(true);
    setProgress(null);
    try {
      const r = await commands.backupMoveLocation(to);
      onMoved();
      setResult({
        ok: true,
        text: r.left_behind
          ? `Moved. The old folder couldn't be removed; delete it by hand: ${r.left_behind}`
          : r.files > 0
            ? `Moved ${bytes(r.bytes)} of backups.`
            : "Backups will be stored here from now on.",
      });
    } catch (e) {
      setResult({ ok: false, text: `Nothing was moved. ${errorText(e)}` });
    } finally {
      setMoving(false);
      setProgress(null);
    }
  };

  return (
    <div className="st-set full" ref={row}>
      <div className="t">Store backups in</div>
      <div className="ctl">
        <PathField path={here} info={info} />
        <Button onClick={pick} disabled={!ready || moving || confirm != null}>
          Change…
        </Button>
      </div>

      {confirm ? (
        <div className="st-confirm">
          <p>
            Move {size ? <b>{size}</b> : "your"} of backups to{" "}
            <b className="d-mono" title={target(confirm.to)}>
              {collapse(target(confirm.to), info)}
            </b>
            ? Each file is copied and checked first; the old folder is removed after. Backups and
            restores wait until it's done.
          </p>
          <div className="row">
            <PrimaryButton onClick={() => move(confirm.to)}>Move backups</PrimaryButton>
            <Button variant="ghost" onClick={() => setConfirm(null)}>
              Cancel
            </Button>
          </div>
        </div>
      ) : moving ? (
        <div className="st-moving" role="status">
          <Meter fraction={progress && progress.total > 0 ? progress.done / progress.total : 0} />
          <span>
            {progress
              ? `Copying ${progress.done.toLocaleString()} of ${plural(progress.total, "file", "files")}…`
              : "Getting ready…"}
          </span>
        </div>
      ) : (
        <div className="d">
          Your backups move with it.{" "}
          {picked && (
            <button className="d-link" onClick={() => setConfirm({ to: null })}>
              Move them back to the default folder
            </button>
          )}
        </div>
      )}
      {result && (
        <div className={result.ok ? "d" : "d err"} style={{ marginTop: 2 }}>
          {result.text}
        </div>
      )}
    </div>
  );
}

function SwitchRow({
  title,
  desc,
  checked,
  disabled,
  onChange,
}: {
  title: string;
  desc: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="st-set">
      <div className="t">{title}</div>
      <div className="d">{desc}</div>
      <div className="ctl">
        <Switch checked={checked} onChange={onChange} label={title} disabled={disabled} />
      </div>
    </div>
  );
}

/** F8: item icons read from the game's own files, cached on this PC. Rebuild
 *  reads every known item's icon again; Clear empties it, and icons are read
 *  again as they're shown. */
function GameDataCache() {
  const [status, setStatus] = useState<IconCacheStatus | null>(null);
  const [busy, setBusy] = useState<"rebuild" | "clear" | null>(null);
  const [result, setResult] = useState<string | null>(null);

  const refresh = () => commands.iconsCacheStatus().then(setStatus, (e) => setResult(errorText(e)));
  useEffect(() => {
    refresh();
  }, []);

  const run = async (what: "rebuild" | "clear") => {
    setBusy(what);
    setResult(null);
    try {
      if (what === "rebuild") {
        const r = await commands.iconsCacheRebuild();
        setResult(
          `Read ${plural(r.read, "icon", "icons")}.` +
            (r.failed ? ` ${plural(r.failed, "icon", "icons")} couldn't be read and show a letter instead.` : ""),
        );
      } else {
        await commands.iconsCacheClear();
        setResult("Cleared. Icons are read again as they're shown.");
      }
    } catch (e) {
      setResult(errorText(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  return (
    <div className="st-set full">
      <div className="t">Game data cache</div>
      <div className="d">
        {status?.build
          ? `Item icons read from your own game files (version ${status.build}) and kept on this PC. Never uploaded or shared. Rebuilt automatically after a game patch.`
          : "Item icons are read from your WoW install once the game folder is set. Until then, items show a letter."}
      </div>
      <div className="ctl" style={{ marginTop: 6 }}>
        <span className="d-dim">
          {status ? `${plural(status.files, "icon", "icons")} · ${bytes(status.bytes)}` : "…"}
        </span>
        <span className="d-grow" />
        <Button onClick={() => run("rebuild")} disabled={busy != null || !status?.build}>
          {busy === "rebuild" ? "Rebuilding…" : "Rebuild"}
        </Button>
        <Button variant="ghost" onClick={() => run("clear")} disabled={busy != null || !status?.files}>
          {busy === "clear" ? "Clearing…" : "Clear"}
        </Button>
      </div>
      {status?.unreadable && (
        <div className="d" style={{ marginTop: 6, color: "var(--warn)" }}>
          Couldn't read the game's art files, so items show letters instead. Nothing else is affected.
        </div>
      )}
      {result && <div className="d" style={{ marginTop: 6 }}>{result}</div>}
    </div>
  );
}

/** What's kept, the storage budget and Prune now. */
function Keep({ storage, onPruned }: { storage: StorageInfo | null; onPruned: () => void }) {
  const [pruning, setPruning] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const rows = storage ? keepRows(storage.retention_summary) : null;

  const prune = async () => {
    setPruning(true);
    setResult(null);
    try {
      const r = await commands.backupPruneNow();
      setResult(
        r.pruned.length > 0
          ? `Removed ${plural(r.pruned.length, "backup", "backups")}${r.freed_bytes ? `, freed ${bytes(r.freed_bytes)}` : ""}.`
          : "Nothing to remove: every backup is still within the rules.",
      );
      onPruned();
    } catch (e) {
      setResult(errorText(e));
    } finally {
      setPruning(false);
    }
  };

  return (
    <div className="st-set full">
      <div className="t">What's kept</div>
      {rows ? (
        <ul className="st-keep">
          {rows.map((r) => (
            <li key={r.k}>
              <span>{r.k}</span>
              <b>{r.v}</b>
            </li>
          ))}
        </ul>
      ) : (
        <div className="d" style={{ margin: "4px 0 10px" }}>{storage?.retention_summary ?? "…"}</div>
      )}
      <div className="st-budget">
        <span>Storage budget</span>
        <Meter
          fraction={storage?.used_bytes != null && storage.budget_bytes ? storage.used_bytes / storage.budget_bytes : 0}
          over={storage?.over_budget}
        />
        <span>
          <b>{storage?.used_bytes != null ? bytes(storage.used_bytes) : "…"}</b>
          {storage?.budget_bytes != null && ` of ${bytes(storage.budget_bytes)}`}
        </span>
        <Button variant="ghost" onClick={prune} disabled={pruning || !storage}>
          {pruning ? "Pruning…" : "Prune now"}
        </Button>
      </div>
      {storage?.cleanup_blocked && (
        <div className="d err" style={{ marginTop: 6 }}>Cleanup is paused: {storage.cleanup_blocked}</div>
      )}
      {result && <div className="d" style={{ marginTop: 6 }}>{result}</div>}
    </div>
  );
}

/** One integration: saved (masked, Remove) or not (paste and Save). The
 *  key itself never comes back from the credential store. */
function ServiceRow({ s, secrets }: { s: Service; secrets: ReturnType<typeof useSecrets> }) {
  const [draft, setDraft] = useState<Partial<Record<IntegrationId, string>>>({});
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const storeErr = s.keys.map((k) => secrets.errorOf(k.id)).find(Boolean) ?? null;
  const saved = s.keys.filter((k) => secrets.isSet(k.id));
  const all = saved.length === s.keys.length;
  const missing = s.keys.filter((k) => !secrets.isSet(k.id));
  const [kind, label]: [PillKind, string] = storeErr
    ? ["bad", "Can't read"]
    : all
      ? ["ok", "Saved"]
      : saved.length > 0
        ? ["warn", "Incomplete"]
        : ["none", "Not set"];

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    setErr(null);
    try {
      await f();
    } catch (e) {
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    run(async () => {
      for (const k of missing) await secrets.set(k.id, (draft[k.id] ?? "").trim());
      setDraft({});
    });
  const remove = () => run(async () => {
    for (const k of saved) await secrets.remove(k.id);
  });
  const ready = missing.every((k) => (draft[k.id] ?? "").trim() !== "");

  return (
    <div className="st-svc">
      <span className="lm" aria-hidden>
        {s.mark}
      </span>
      <span className="nm">
        {s.name} <span className="opt">Optional</span>
      </span>
      <span className="st">
        <Pill kind={kind}>
          <span className="dt" />
          {label}
        </Pill>
      </span>
      <span className="what">{s.what}</span>
      <div className="keys">
        {saved.map((k) => (
          <span key={k.id} className="d-field d-mono" title={`${k.placeholder} saved`}>
            <span className="masked">••••••••••••••••</span>
          </span>
        ))}
        {missing.map((k) => (
          <span key={k.id} className="d-field d-mono">
            <input
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={k.placeholder}
              aria-label={`${s.name} ${k.placeholder.toLowerCase()}`}
              value={draft[k.id] ?? ""}
              onChange={(e) => setDraft((d) => ({ ...d, [k.id]: e.target.value }))}
              onKeyDown={(e) => e.key === "Enter" && ready && !busy && save()}
            />
          </span>
        ))}
        {missing.length > 0 && (
          <Button onClick={save} disabled={!ready || busy}>
            Save
          </Button>
        )}
        {saved.length > 0 && (
          <Button variant="ghost" onClick={remove} disabled={busy}>
            Remove
          </Button>
        )}
      </div>
      {(err ?? storeErr) && <span className="err">{err ?? storeErr}</span>}
      {s.caveat && <span className="caveat">{s.caveat}</span>}
    </div>
  );
}
