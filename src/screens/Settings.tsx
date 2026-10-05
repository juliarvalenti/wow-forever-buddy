import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, KeyRound, Lock } from "lucide-react";
import type { IntegrationId, StorageInfo } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import {
  Button,
  Meter,
  Page,
  PageHeader,
  Panel,
  PanelHeader,
  Pill,
  type PillKind,
  Switch,
} from "@/components/d";
import { useBackups } from "@/hooks/useBackups";
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

export function Settings({
  install,
  onOpenGameFolder,
}: {
  install: ReturnType<typeof useInstall>;
  onOpenGameFolder: () => void;
}) {
  const { settings, error, update } = useSettings();
  const { storage, refresh: refreshBackups } = useBackups();
  const info = useAppInfo();
  const secrets = useSecrets();

  const active = install.state.kind === "ok" ? install.state.install : null;
  const flavor = active?.flavors.find((f) => f.id === active.active);
  const backup = settings?.backup;
  const hours = backup?.schedule_hours ?? 24;
  const picked = backup?.location ?? null;
  const location = picked
    ? join(picked, STORE_FOLDER)
    : info
      ? join(info.paths.local_data_dir, "backups")
      : "…";

  const chooseLocation = async () => {
    const path = await open({ directory: true, multiple: false });
    if (typeof path === "string") await update({ backup: { location: path } });
  };

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
                <span className="d-field d-mono">
                  <FolderOpen size={14} aria-hidden style={{ color: "var(--soot)", flexShrink: 0 }} />
                  <span>{active?.root ?? (install.state.kind === "invalid" ? "Missing" : "Not set")}</span>
                </span>
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
            <div className="st-set full">
              <div className="t">Store backups in</div>
              <div className="ctl">
                <span className="d-field d-mono" title={location}>
                  <span>{location}</span>
                </span>
                <Button onClick={chooseLocation} disabled={!settings}>
                  Change…
                </Button>
              </div>
              <div className="d">
                {picked
                  ? "Backups already taken stay in the old folder. Switch back to see them again. "
                  : "Pick another drive to keep backups off this one. "}
                {picked && (
                  <button className="d-link" onClick={() => update({ backup: { location: null } })}>
                    Use the default folder
                  </button>
                )}
              </div>
            </div>
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
              <b>Every key is optional.</b> The app works fully offline without them. Keys are stored
              in <b>{keyStore()}</b>, never in app files or backups.
            </span>
          </div>
          {SERVICES.map((s) => (
            <ServiceRow key={s.mark} s={s} secrets={secrets} />
          ))}
        </Panel>
      </section>
    </Page>
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
