// Talking to the live server (fbl-rs live): the data, live updates, hiding items.

export async function fetchData() {
  const response = await fetch("/data.json", { cache: "no-store" });
  if (!response.ok) throw new Error(`data.json: ${response.status}`);
  return response.json(); // { items, opps, stamp }
}

// Gold and item value over time: { characters, gold: [[t, copper]], history: [[t, gold, items]] }.
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
