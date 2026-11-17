// Entry point: loads the data from the live server (fbl-rs live), keeps it current, and wires the few app-wide actions.

import "./components/app-header.js";
import "./components/main-view.js";
import "./components/opportunities-view.js";
import "./components/check-list.js";
import "./components/items-view.js";
import "./components/round-dialog.js";
import "./components/wealth-view.js";

import { store } from "./lib/store.js";
import { fetchData, fetchWealth, onServerUpdate, setHidden } from "./lib/api.js";
import { enrichItem } from "./lib/model.js";
import { nextRound } from "./lib/round.js";

const PAGE_TITLE = document.title;
const DEFAULT_ITEM = "Mageweave Cloth"; // something to show before you pick

async function loadData({ pushed }) {
  const [data, wealth] = await Promise.all([fetchData(), fetchWealth().catch(() => null)]);
  const items = data.items.map(enrichItem);
  store.set({ items, opps: data.opps, stamp: data.stamp, wealth, lastUpdate: pushed ? new Date() : store.state.lastUpdate });

  if (store.state.prefs.selectedId == null) {
    const first = items.find((it) => it.n === DEFAULT_ITEM) || items[0];
    if (first) store.setPrefs({ selectedId: first.id });
  }

  // A new scan starts a round; ladders from your save move it to results.
  const round = nextRound(store.state.round, data.opps);
  if (round) {
    store.setRound(round);
    if (pushed) document.title = `● ${round.step === "check" ? "New price scan" : "Ladders in"} · ${PAGE_TITLE}`;
  }
}

// Hide / Show buttons, wherever they are: tell the server; it rebuilds and pushes an update.
document.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-hide], [data-unhide]");
  if (!button) return;
  const hide = button.hasAttribute("data-hide");
  const id = Number(hide ? button.dataset.hide : button.dataset.unhide);
  button.disabled = true;
  button.textContent = hide ? "Hiding…" : "Showing…";
  if (hide) store.setRound({ picks: store.state.round.picks.filter((pick) => pick !== id) });
  await setHidden(id, button.dataset.name, hide);
});

// The tab title flags news while you're in the game; looking at the page clears it.
window.addEventListener("focus", () => {
  document.title = PAGE_TITLE;
});

loadData({ pushed: false }).catch((error) => {
  const view = document.querySelector("main-view");
  view.textContent = `Couldn't load the data (${error.message}). Is fbl-rs live running?`;
  view.classList.add("empty");
});
// The header says Offline only if the connection stays down a few seconds (it reconnects by itself).
let offlineTimer;
onServerUpdate(
  () => loadData({ pushed: true }).catch(() => {}),
  (live) => {
    clearTimeout(offlineTimer);
    if (live) {
      if (!store.state.live) store.set({ live: true });
    } else offlineTimer = setTimeout(() => store.set({ live: false }), 3000);
  },
);
