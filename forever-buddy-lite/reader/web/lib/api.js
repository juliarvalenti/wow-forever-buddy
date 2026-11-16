// Talking to live.mjs: the data, live updates, hiding items.

export async function fetchData() {
  const response = await fetch("/data.json", { cache: "no-store" });
  if (!response.ok) throw new Error(`data.json: ${response.status}`);
  return response.json(); // { items, opps, stamp }
}

// Calls onUpdate whenever a scan or save reaches disk. The browser reconnects by itself.
export function onServerUpdate(onUpdate) {
  const events = new EventSource("/events");
  events.addEventListener("update", onUpdate);
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
