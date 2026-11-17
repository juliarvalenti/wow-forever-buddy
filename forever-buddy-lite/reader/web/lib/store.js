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
  view: "opportunities", // one of VIEWS
};

// The pages in the header's navigation.
export const VIEWS = ["opportunities", "items", "wealth"];

// A view saved by an older version ("item" was one item's page), or anything unknown: map it.
function knownView(view) {
  if (view === "item") return "items";
  return VIEWS.includes(view) ? view : "opportunities";
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

function storedPrefs() {
  const prefs = readStored(PREFS_KEY, DEFAULT_PREFS);
  return { ...prefs, view: knownView(prefs.view) };
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
  }

  setRound(patch) {
    this.set({ round: { ...this.state.round, ...patch } });
    writeStored(ROUND_KEY, this.state.round);
  }

  // Open one item on the Items page, from anywhere. "open-item" lets that page bring it into view.
  openItem(id) {
    this.setPrefs({ selectedId: id, view: "items" });
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
