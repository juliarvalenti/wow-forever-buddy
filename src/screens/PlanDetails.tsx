import type { ReactNode } from "react";
import type { RestorePlan } from "@/lib/bindings";
import { Callout } from "@/components/d";
import { plural } from "@/lib/format";

/** "WTF/Account/ACCOUNT1/Ashenvale/Thrandor/x.wtf" → "…\Ashenvale\Thrandor\x.wtf":
 *  Windows separators, and the account prefix every path shares left out.
 *  The full path stays in the title tooltip. */
export function shortPath(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  const i = parts.findIndex((s, n) => s === "Account" && parts[n - 1] === "WTF");
  return i >= 0 && parts.length > i + 2 ? `…\\${parts.slice(i + 2).join("\\")}` : parts.join("\\");
}

/** What a restore (or a recovery) will write and remove, for its confirm
 *  step. Deletions are always listed in full. `removedBecause` finishes the
 *  sentence "N files will be removed, because …" for one or many files. */
export function PlanDetails({
  plan,
  lead,
  removedBecause = (one) => (one ? "it isn't in this snapshot" : "they aren't in this snapshot"),
}: {
  plan: RestorePlan;
  /** The opening sentence; defaults to the backend's summary. */
  lead?: ReactNode;
  removedBecause?: (one: boolean) => string;
}) {
  return (
    <>
      <p>{lead ?? <b>{plan.summary}</b>}</p>
      {plan.write.length > 0 && (
        <ul className="d-files d-mono">
          {plan.write.map((f) =>
            f.files.length === 1 ? (
              <li key={f.folder} title={`${f.folder}/${f.files[0]}`}>
                {shortPath(`${f.folder}/${f.files[0]}`)}
              </li>
            ) : (
              <li key={f.folder} title={f.folder}>
                {shortPath(f.folder)}\ <span className="folder">({plural(f.files.length, "file", "files")})</span>
              </li>
            ),
          )}
        </ul>
      )}
      {plan.delete.length > 0 && (
        <>
          <p>
            <b>{plural(plan.delete.length, "file", "files")} will be removed</b>, because{" "}
            {removedBecause(plan.delete.length === 1)}:
          </p>
          <ul className="d-files d-mono">
            {plan.delete.map((f) => (
              <li key={f} title={f}>
                {shortPath(f)}
              </li>
            ))}
          </ul>
        </>
      )}
      {plan.not_backed_up.length > 0 && (
        <p className="d-muted">
          Not in this backup (couldn't be read at the time), so left as is:{" "}
          <span className="d-mono">{plan.not_backed_up.join(", ")}</span>
        </p>
      )}
      {plan.read_only.length > 0 && (
        <Callout tone="bad">
          <span>
            These files are marked read-only, so nothing will be restored until you clear the
            flag: <span className="d-mono">{plan.read_only.join(", ")}</span>
          </span>
        </Callout>
      )}
      {plan.unchanged > 0 && (
        <p className="d-muted">{plural(plan.unchanged, "file already matches", "files already match")}.</p>
      )}
    </>
  );
}

/** Nothing to confirm or nothing allowed: read-only files, or no changes. */
export function planBlocked(plan: RestorePlan | null): boolean {
  return !plan || plan.read_only.length > 0 || plan.write_count + plan.delete.length === 0;
}
