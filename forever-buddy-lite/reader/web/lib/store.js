// The app's state, in one place. Components subscribe and re-render when it changes.
//
// Replace objects instead of mutating them (set/setPrefs/setRound do), so a component can tell
// whether what it shows changed by comparing references.
//
// state = {
//   items:  [enriched item, ...]       every named item with its daily history (see model.js)
//   opps:   {...} | null               the market analysis (reader-rs dashboard.rs): sections, worklist, ladders, scanAt
//   stamp:  "2026-11-16"               last day the scans cover
//   wealth: {...} | null               gold and item value over time (see api.fetchWealth)
//   lastUpdate: Date | null            when the server last pushed new data
//   live:   true | false               whether the live connection (/events) is up
//   prefs:  {...}                      filters, selection, view: remembered in this browser
//   round:  {...}                      the current price-scan round (see round.js): remembered too
// }

const PREFS_KEY = "fbl-prefs";
const ROUND_KEY = "fbl-round";

export const DEFAULT_PREFS = {
  query: "",
  kinds: [],
  trends: [],
  priceMin: "", // silver, "" = any
  priceMax: "",
  listedMin: 0,
  sort: "name",
  selectedId: null,
  filtersOpen: false, // the item finder's Filters panel
  chartRange: "24h", // the item page's time range: one of RANGES in item-view.js
  snipes: null, // the Snipes page's settings (see DEFAULT_SNIPES in snipes-view.js)
  view: "opportunities", // one of PAGES
};

// The pages in the header's navigation.
export const VIEWS = ["opportunities", "items", "snipes", "trades", "wealth"];
// Every page with a route: the navigation's, plus the style guide (linked from the footer).
export const PAGES = [...VIEWS, "design"];

// A view saved by an older version ("item" was one item's page), or anything unknown: map it.
function knownView(view) {
  if (view === "item") return "items";
  return PAGES.includes(view) ? view : "opportunities";
}

export const NO_ROUND = { scanAt: 0, step: "done", picks: [], open: false };

// Browser storage can be missing or blocked (private windows): fall back quietly.
function readStored(key, defaults) {
  try {
    const text = localStorage.getItem(key);
    return text == null ? defaults : { ...defaults, ...JSON.parse(text) };
  } catch {
    return defaults;
  }
}

function writeStored(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // not remembered; fine
  }
}

// The address bar mirrors where you are: #/opportunities, #/items/2589, #/snipes, ... Moving to
// another page or item (store.navigate) adds a browser history entry, so Back and Forward work
// and a link opens the same page in a new tab. Other pref changes only keep the address in step.
export function routeOf(prefs) {
  return prefs.view === "items" && prefs.selectedId != null ? `#/items/${prefs.selectedId}` : `#/${prefs.view}`;
}

export function parseRoute(hash) {
  const match = /^#\/(\w+)(?:\/(\d+))?$/.exec(hash || "");
  if (!match || !PAGES.includes(match[1])) return null;
  return match[2] ? { view: match[1], selectedId: Number(match[2]) } : { view: match[1] };
}

function storedPrefs() {
  const prefs = readStored(PREFS_KEY, DEFAULT_PREFS);
  // The address wins over what was remembered: a link or a reload shows the page it names.
  return { ...prefs, view: knownView(prefs.view), ...parseRoute(location.hash) };
}

class Store extends EventTarget {
  state = {
    items: [],
    opps: null,
    stamp: "",
    wealth: null,
    lastUpdate: null,
    live: true,
    prefs: storedPrefs(),
    round: readStored(ROUND_KEY, NO_ROUND),
  };

  set(patch) {
    this.state = { ...this.state, ...patch };
    this.#changed();
  }

  setPrefs(patch) {
    this.set({ prefs: { ...this.state.prefs, ...patch } });
    writeStored(PREFS_KEY, this.state.prefs);
    const route = routeOf(this.state.prefs);
    if (location.hash !== route) history.replaceState(null, "", route);
  }

  // Go to another page or item, as a step Back can undo.
  navigate(patch) {
    const route = routeOf({ ...this.state.prefs, ...patch });
    if (location.hash !== route) history.pushState(null, "", route);
    this.setPrefs(patch);
  }

  setRound(patch) {
    this.set({ round: { ...this.state.round, ...patch } });
    writeStored(ROUND_KEY, this.state.round);
  }

  // Open one item on the Items page, from anywhere. "open-item" lets that page bring it into view.
  openItem(id) {
    this.navigate({ selectedId: id, view: "items" });
    this.dispatchEvent(new CustomEvent("open-item", { detail: id }));
  }

  subscribe(listener) {
    this.addEventListener("change", listener);
    return () => this.removeEventListener("change", listener);
  }

  #changed() {
    this.dispatchEvent(new Event("change"));
  }
}

export const store = new Store();

// Back, Forward, or an edited address: show the page it names.
window.addEventListener("popstate", () => {
  const route = parseRoute(location.hash);
  if (!route) return;
  store.setPrefs(route);
  if (route.selectedId != null) store.dispatchEvent(new CustomEvent("open-item", { detail: route.selectedId }));
});
history.replaceState(null, "", routeOf(store.state.prefs));
