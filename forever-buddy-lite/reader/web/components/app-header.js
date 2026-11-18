// The app's frame, as in the Forever Buddy app: a stitched leather sidebar with the wordmark, the
// main navigation (a red ribbon marks where you are), and a status well at its foot: whether we're
// live, how fresh the prices are, and the current price-scan round (click to reopen its dialog).
// Under 1000px wide it becomes a leather bar across the top (styles/base.css).

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { icon } from "../lib/icons.js";
import { clockTime } from "../lib/format.js";
import { roundPicks } from "../lib/round.js";

const NAV = [
  {
    group: "Market",
    pages: [
      { view: "opportunities", label: "Opportunities", icon: "trend" },
      { view: "items", label: "Items", icon: "scale" },
      { view: "snipes", label: "Snipes", icon: "target" },
    ],
  },
  {
    group: "Your gold",
    pages: [
      { view: "trades", label: "Trades", icon: "scroll" },
      { view: "wealth", label: "Wealth", icon: "coins" },
    ],
  },
];

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
    const gaps = (opps?.check || []).filter((row) => row.kind === "gap").length;
    const updated = lastUpdate ? `, updated ${lastUpdate.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}` : "";
    const statusTitle = live
      ? `Connected: updates by itself after each scan or save. Prices through ${stamp || "…"}${updated}.`
      : "Not connected to fbl-rs live. Retrying…";

    const link = (page) => html`
      <a class="nav-link" href="#/${page.view}" data-view="${page.view}" ${prefs.view === page.view ? html`aria-current="page"` : ""}>
        ${icon(page.icon)}<span class="lbl">${page.label}</span>
        ${page.view === "opportunities" && gaps > 0 && html`<span class="n badge" title="${gaps} good gaps to buy">${gaps}</span>`}
      </a>`;

    return html`
      <div class="side">
        <a class="brand" href="#/opportunities" data-view="opportunities">
          <span class="brand-mark" aria-hidden="true">${icon("shield")}</span>
          <span class="brand-text">
            <span class="brand-name">ForeverBuddy Lite</span>
            <span class="brand-sub">for WoW: Forever</span>
          </span>
        </a>
        <nav class="nav" aria-label="Main">
          ${NAV.map((section) => html`<div class="nav-group" aria-hidden="true">${section.group}</div>${section.pages.map(link)}`)}
        </nav>
        <div class="side-status">
          <div class="status-row ${live ? "" : "offline"}" title="${statusTitle}">
            <span class="dot ${live ? "live" : "bad"}" aria-hidden="true"></span>
            <span><b>${live ? "Live" : "Offline"}</b></span>
          </div>
          ${stamp &&
          html`<div class="status-row status-stamp" title="${statusTitle}">
            <span class="dot ok" aria-hidden="true"></span><span>Prices through ${stamp}</span>
          </div>`}
          ${inRound &&
          html`<button class="round-indicator" type="button" data-open-round
            title="Price scan of ${clockTime(round.scanAt)}: ${checked.length} of ${picks.length} picks searched. Open the round.">
            <span class="dot live" aria-hidden="true"></span>
            <span class="round-label">Scan ${clockTime(round.scanAt)}</span>
            <span class="round-count">${checked.length}/${picks.length}</span>
          </button>`}
        </div>
      </div>`;
  }
}

define("app-header", AppHeader);
