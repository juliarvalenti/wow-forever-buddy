// Entry point: loads the data from live.mjs, keeps it current, and wires the few app-wide actions.

import "./components/item-finder.js";
import "./components/top-bar.js";
import "./components/main-view.js";
import "./components/opportunities-view.js";
import "./components/check-list.js";
import "./components/item-view.js";
import "./components/round-dialog.js";

import { store } from "./lib/store.js";
import { fetchData, onServerUpdate, setHidden } from "./lib/api.js";
import { enrichItem } from "./lib/model.js";
import { nextRound } from "./lib/round.js";

const PAGE_TITLE = document.title;
const DEFAULT_ITEM = "Mageweave Cloth"; // something to show before you pick

async function loadData({ pushed }) {
  const data = await fetchData();
  const items = data.items.map(enrichItem);
  store.set({ items, opps: data.opps, stamp: data.stamp, lastUpdate: pushed ? new Date() : store.state.lastUpdate });

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
  document.querySelector("main-view").textContent = `Couldn't load the data (${error.message}). Is live.mjs running?`;
});
onServerUpdate(() => loadData({ pushed: true }).catch(() => {}));
