import type { RestorePlan } from "@/lib/bindings";
import { Callout } from "@/components/d";
import { plural } from "@/lib/format";

/** What a restore (or a recovery) will write and remove, for its confirm
 *  step. Deletions are always listed in full. `removedBecause` finishes the
 *  sentence "N files will be removed, because …" for one or many files. */
export function PlanDetails({
  plan,
  removedBecause = (one) => (one ? "it isn't in this snapshot" : "they aren't in this snapshot"),
}: {
  plan: RestorePlan;
  removedBecause?: (one: boolean) => string;
}) {
  return (
    <>
      <p>
        <b>{plan.summary}</b>
      </p>
      {plan.write.length > 0 && (
        <ul className="d-files">
          {plan.write.map((f) => (
            <li key={f.folder}>
              <span className="d-mono">{f.folder}/</span>{" "}
              <span className="folder">({plural(f.files.length, "file", "files")})</span>
            </li>
          ))}
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
              <li key={f}>{f}</li>
            ))}
          </ul>
        </>
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
