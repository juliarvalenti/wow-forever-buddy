// The Wealth tab: what you're worth now across characters, how it's moved, and who holds what.
// Gold comes from the addon's money log; item value from fbl-rs live's record at each save.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";
import { coins } from "../lib/coins.js";
import "./wealth-chart.js";

class WealthView extends Component {
  watch(s) {
    return [s.wealth];
  }

  render({ wealth }) {
    if (!wealth) return html`<p class="empty">Loading wealth…</p>`;
    const chars = [...wealth.characters].sort((a, b) => b.gold + b.items - (a.gold + a.items));
    const gold = chars.reduce((sum, c) => sum + c.gold, 0);
    const items = chars.reduce((sum, c) => sum + c.items, 0);
    const first = wealth.gold[0];
    const sinceFirst = first ? gold - first[1] : 0;
    const historyStarts = first ? new Date(first[0] * 1000) : null;
    const thin = wealth.gold.length < 2 && wealth.history.length < 2;

    return html`
      <header class="page-head">
        <h1>Wealth</h1>
        <p class="sub">Gold on every character, plus what their bags and bank would fetch at the latest AH prices.</p>
      </header>
      <div class="tiles">
        ${tile("Worth now", coins(gold + items), "gold + items")}
        ${tile("Gold", coins(gold), `on ${chars.length} characters`)}
        ${tile("Items", coins(items), "bags and bank at today’s prices")}
        ${tile(
          "Gold change",
          sinceFirst === 0 ? html`<span class="muted">no change yet</span>` : html`<span class="${sinceFirst > 0 ? "up" : "down"}">${coins(sinceFirst, { signed: true })}</span>`,
          historyStarts ? `since ${historyStarts.toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}` : "no history yet",
        )}
      </div>
      <section class="card">
        <h3>Over time</h3>
        ${thin
          ? html`<p class="sub">
              History starts now. From your next /reload the addon logs every change in gold (loot, sales, quests, repairs), and
              each save records what your items are worth. Come back after a session.
            </p>`
          : ""}
        <wealth-chart></wealth-chart>
      </section>
      <section class="card">
        <h3>By character</h3>
        <div class="table-scroll">
          <table>
            <thead><tr><th>Character</th><th>Level</th><th>Gold</th><th>Items</th><th>Total</th></tr></thead>
            <tbody>
              ${chars.map(
                (c) => html`
                  <tr>
                    <td><span class="item-name">${c.name}</span>${c.class && html` <span class="muted">${titleCase(c.class)}</span>`}</td>
                    <td>${c.level ?? "–"}</td>
                    <td>${coins(c.gold)}</td>
                    <td>${coins(c.items)}</td>
                    <td>${coins(c.gold + c.items)}</td>
                  </tr>`,
              )}
            </tbody>
          </table>
        </div>
      </section>`;
  }

  rendered({ wealth }) {
    const chart = this.querySelector("wealth-chart");
    if (!chart || !wealth) return;
    const total = wealth.history.map(([t, g, i]) => [t, g + i]);
    chart.show(wealth.gold, total);
  }
}

// "ROGUE" (the game's class token) -> "Rogue".
function titleCase(word) {
  return word.charAt(0).toUpperCase() + word.slice(1).toLowerCase();
}

function tile(label, value, note) {
  return html`<div class="tile"><div class="tile-label">${label}</div><div class="tile-value">${value}</div><div class="tile-note">${note}</div></div>`;
}

define("wealth-view", WealthView);
