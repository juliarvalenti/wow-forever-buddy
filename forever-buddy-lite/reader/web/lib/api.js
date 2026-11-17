// Talking to the live server (fbl-rs live): the data, live updates, hiding items.

export async function fetchData() {
  const response = await fetch("/data.json", { cache: "no-store" });
  if (!response.ok) throw new Error(`data.json: ${response.status}`);
  return response.json(); // { items, opps, stamp }
}

// Gold and item value over time: { characters, gold: [[t, copper]], history: [[t, gold, items]] }.
// One item's cheapest price and quantity at every archived scan: { id, scans, points: [[t, cheapest, listed]] }.
export async function fetchHistory(id) {
  const response = await fetch(`/history.json?id=${id}`, { cache: "no-store" });
  if (!response.ok) throw new Error(`history.json: ${response.status}`);
  return response.json();
}

// A Point Blank Sniper list for these settings (prices in copper): { name, list, items }.
export async function fetchSnipes(settings) {
  const query = new URLSearchParams(Object.entries(settings).map(([k, v]) => [k, String(v)]));
  const response = await fetch(`/snipes.json?${query}`, { cache: "no-store" });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

// The trade ledger: { totals, items, recent } (copper).
export async function fetchJournal() {
  const response = await fetch("/journal.json", { cache: "no-store" });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

export async function fetchWealth() {
  const response = await fetch("/wealth.json", { cache: "no-store" });
  if (!response.ok) throw new Error(`wealth.json: ${response.status}`);
  return response.json();
}

// Calls onUpdate whenever a scan or save reaches disk. The browser reconnects by itself.
// onLive(true | false) reports the connection going up or down (the header shows it).
export function onServerUpdate(onUpdate, onLive = () => {}) {
  const events = new EventSource("/events");
  events.addEventListener("update", onUpdate);
  events.addEventListener("open", () => onLive(true));
  events.addEventListener("error", () => onLive(events.readyState === EventSource.OPEN));
  return () => events.close();
}

// Never suggest an item again (on = true), or bring it back. The server rebuilds and pushes an update.
export async function setHidden(id, name, on) {
  await fetch("/hide", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, name, on }),
  });
}

export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // No clipboard access: let the user copy it by hand.
    window.prompt("Copy this:", text);
    return false;
  }
}
