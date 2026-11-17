// The app header: name, the main navigation (Opportunities, Items, Wealth), and two quiet indicators
// on the right: the current price-scan round (click to reopen its dialog) and whether we're live.

import { Component, define } from "../lib/component.js";
import { store, VIEWS } from "../lib/store.js";
import { html } from "../lib/html.js";
import { clockTime } from "../lib/format.js";
import { roundPicks } from "../lib/round.js";

const LABELS = { opportunities: "Opportunities", items: "Items", snipes: "Snipes", trades: "Trades", wealth: "Wealth" };

class AppHeader extends Component {
  watch(s) {
    return [s.prefs.view, s.round, s.opps, s.stamp, s.lastUpdate, s.live];
  }

  setup() {
    this.addEventListener("click", (event) => {
      const link = event.target.closest("[data-view]");
      // A plain click moves within the page; Ctrl/middle-click opens the link in a new tab as usual.
      if (link && event.button === 0 && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
        event.preventDefault();
        store.navigate({ view: link.dataset.view });
      }
      if (event.target.closest("[data-open-round]")) store.setRound({ open: true });
    });
  }

  render({ prefs, round, opps, stamp, lastUpdate, live }) {
    const inRound = round.step !== "done" && round.scanAt;
    const { picks, checked } = roundPicks(round, opps);
    const updated = lastUpdate ? `, updated ${lastUpdate.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}` : "";
    const statusTitle = live
      ? `Connected: updates by itself after each scan or save. Prices through ${stamp || "…"}${updated}.`
      : "Not connected to fbl-rs live. Retrying…";

    return html`
      <header class="app-header">
        <div class="shell header-inner">
          <a class="brand" href="#/opportunities" data-view="opportunities">
            <span class="brand-mark" aria-hidden="true"></span>ForeverBuddy <span class="brand-lite">Lite</span>
          </a>
          <nav class="nav" aria-label="Main">
            ${VIEWS.map(
              (view) => html`<a class="nav-link" href="#/${view}" data-view="${view}" ${prefs.view === view ? html`aria-current="page"` : ""}>${LABELS[view]}</a>`,
            )}
          </nav>
          <div class="header-meta">
            ${inRound &&
            html`<button class="round-indicator" type="button" data-open-round
              title="Price scan of ${clockTime(round.scanAt)}: ${checked.length} of ${picks.length} picks searched. Open the round.">
              <span class="round-dot" aria-hidden="true"></span>
              <span class="round-label">Scan ${clockTime(round.scanAt)}</span>
              <span class="round-count">${checked.length}/${picks.length}</span>
            </button>`}
            <span class="status ${live ? "" : "offline"}" title="${statusTitle}">
              <span class="live-dot" aria-hidden="true"></span>
              <span class="status-text">${live ? "Live" : "Offline"}</span>
              ${stamp && html`<span class="status-stamp">· prices through ${stamp}</span>`}
            </span>
          </div>
        </div>
      </header>`;
  }
}

define("app-header", AppHeader);
