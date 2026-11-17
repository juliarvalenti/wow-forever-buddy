// A round: what to do after each price scan.
//
//   1. A new Auctionator scan lands (opps.scanAt is newer than the round's): the round starts with
//      that scan's picks (cheap now, you hold it, swingy) and the dialog opens.
//   2. You open each pick at the AH; the addon records its ladder. A pick counts as checked when
//      its latest ladder is newer than the scan.
//   3. Your save brings those ladders in: the round moves to results and shows the verdicts.
//
// round = { scanAt, step: "check" | "results" | "done", picks: [itemId], open }

export const VERDICT = {
  gap: { label: "Good gap", order: 0 },
  todo: { label: "Search it", order: 1 },
  stale: { label: "Search again", order: 2 },
  big: { label: "Too big", order: 3 },
  yours: { label: "Yours", order: 4 },
  tight: { label: "Tight", order: 5 },
  empty: { label: "Empty", order: 6 },
};

export function isChecked(opps, itemId, scanAt) {
  const ladder = opps?.ladders?.[itemId];
  return Boolean(ladder && ladder.at >= scanAt);
}

// The round that new data calls for, or null to keep the current one.
export function nextRound(round, opps) {
  if (!opps?.scanAt) return null;
  if (opps.scanAt > round.scanAt) {
    // Things to decide on buying: not what you already hold (that never needs a fresh search).
    const picks = (opps.check || []).filter((c) => !(c.held > 0) && c.why.some((why) => why !== "searched")).map((c) => c.id);
    return { scanAt: opps.scanAt, step: "check", picks, open: true };
  }
  if (round.step === "check" && round.picks.some((id) => isChecked(opps, id, round.scanAt))) {
    return { ...round, step: "results", open: true };
  }
  return null;
}

// The round's picks as worklist rows (opps.check entries), split into checked and not yet.
export function roundPicks(round, opps) {
  const byId = new Map((opps?.check || []).map((c) => [c.id, c]));
  const picks = round.picks.map((id) => byId.get(id)).filter(Boolean);
  return {
    picks,
    checked: picks.filter((c) => isChecked(opps, c.id, round.scanAt)),
    left: picks.filter((c) => !isChecked(opps, c.id, round.scanAt)),
  };
}

export function byVerdict(a, b) {
  return (VERDICT[a.kind]?.order ?? 9) - (VERDICT[b.kind]?.order ?? 9) || (b.gap?.profit || 0) - (a.gap?.profit || 0);
}

// Auctionator's shopping-list import format: List name^"exact name";;;;;;;;;;;#;;^...
export function auctionatorList(names, listName = "FBL check") {
  const terms = names.map((name) => `"${name.replace(/"/g, "")}";;;;;;;;;;;#;;`);
  return [listName, ...terms].join("^");
}
