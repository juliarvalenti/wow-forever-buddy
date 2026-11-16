// The row above the main view: view tabs, the current round's progress, and whether we're live.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { clockTime } from "../lib/format.js";
import { roundPicks } from "../lib/round.js";

class TopBar extends Component {
  watch(s) {
    return [s.prefs.view, s.prefs.selectedId, s.items, s.round, s.opps, s.stamp, s.lastUpdate];
  }

  setup() {
    this.addEventListener("click", (event) => {
      const tab = event.target.closest("[data-view]");
      if (tab) store.setPrefs({ view: tab.dataset.view });
      if (event.target.closest("[data-open-round]")) store.setRound({ open: true });
    });
  }

  render({ prefs, items, round, opps, stamp, lastUpdate }) {
    const selected = items.find((it) => it.id === prefs.selectedId);
    const inRound = round.step !== "done" && round.scanAt;
    const { picks, checked } = roundPicks(round, opps);
    const updated = lastUpdate && ` · updated ${lastUpdate.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;

    return html`
      <nav class="top-bar" aria-label="Views">
        <button class="chip" type="button" data-view="opportunities" aria-pressed="${prefs.view === "opportunities"}">Opportunities</button>
        ${selected && html`<button class="chip" type="button" data-view="item" aria-pressed="${prefs.view === "item"}">${selected.n}</button>`}
        ${inRound && html`<button class="btn" type="button" data-open-round>Round ${clockTime(round.scanAt)} · ${checked.length}/${picks.length} checked</button>`}
        <span class="status"><span class="live-dot" aria-hidden="true"></span>live · scans through ${stamp}${updated}</span>
      </nav>`;
  }
}

define("top-bar", TopBar);
