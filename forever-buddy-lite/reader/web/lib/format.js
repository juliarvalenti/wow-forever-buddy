// Display formatting: gold, percentages, times.

// Copper to "1g 23s", "4s 05c" or "87c". Prices on the AH are whole copper.
export function money(copper) {
  if (copper == null) return "–";
  const c = Math.round(copper);
  const gold = Math.floor(c / 10000);
  const silver = Math.floor((c % 10000) / 100);
  const cop = c % 100;
  if (gold) return `${gold}g ${silver}s`;
  if (silver) return `${silver}s ${String(cop).padStart(2, "0")}c`;
  return `${cop}c`;
}

// 0.123 -> "+12%"
export function percent(fraction) {
  return `${fraction > 0 ? "+" : ""}${Math.round(fraction * 100)}%`;
}

// Unix seconds -> "5m ago" / "2h ago".
export function ago(unixSeconds) {
  if (!unixSeconds) return "–";
  const minutes = Math.max(0, Math.round((Date.now() / 1000 - unixSeconds) / 60));
  return minutes < 60 ? `${minutes}m ago` : `${Math.round(minutes / 60)}h ago`;
}

// Unix seconds -> "1:19 PM" in the viewer's locale.
export function clockTime(unixSeconds) {
  return new Date(unixSeconds * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

export function count(n) {
  return Number(n).toLocaleString();
}
