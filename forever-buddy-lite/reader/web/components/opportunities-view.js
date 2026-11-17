// The Opportunities tab: the ladder worklist first, then one card per market section.
// Clicking a row opens that item.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { percent, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";

const ROWS_PER_SECTION = 15;

const vsUsual = (r) => percent(r.now / r.usual - 1);
const listed = (r) => count(r.qty);

// What each market section means, and the columns that show it.
const SECTIONS = [
  {
    key: "buy",
    title: "Buy",
    why: "A stray cheap listing on a busy market: under usual and under today’s own high. Profit is one item resold near usual, after the 5% cut and a 1s deposit.",
    columns: [["Now", (r) => coins(r.now)], ["Usual", (r) => coins(r.usual)], ["vs usual", vsUsual], ["Profit each", (r) => coins(r.profit)], ["Listed", listed]],
  },
  {
    key: "dips",
    title: "Dip bets",
    why: "The whole market is down today. Not a sure snipe: a bet that it comes back to usual.",
    columns: [["Now", (r) => coins(r.now)], ["Usual", (r) => coins(r.usual)], ["vs usual", vsUsual], ["If it recovers", (r) => coins(r.profit)], ["Listed", listed]],
  },
  {
    key: "sell",
    title: "Sell what you hold",
    why: "Things on your characters (bags, bank, mail) that sell above their usual today.",
    columns: [["You have", (r) => r.held], ["Now", (r) => coins(r.now)], ["Usual", (r) => coins(r.usual)], ["vs usual", vsUsual], ["After cut", (r) => coins(r.worth)]],
  },
  {
    key: "squeeze",
    title: "Squeeze",
    why: "Supply falling while the price rises: sellers can ask more.",
    columns: [["Price", (r) => percent(r.dp)], ["Supply", (r) => percent(r.dq)], ["Now", (r) => coins(r.now)], ["You have", (r) => r.held || ""]],
  },
  {
    key: "flood",
    title: "Flood",
    why: "Supply rising while the price falls: do not hold these; buy later.",
    columns: [["Price", (r) => percent(r.dp)], ["Supply", (r) => percent(r.dq)], ["Now", (r) => coins(r.now)], ["You have", (r) => r.held || ""]],
  },
  {
    key: "swingy",
    title: "Swingy",
    why: "The widest range today on busy markets: buy near the low, list near the high.",
    columns: [["Today low", (r) => coins(r.low)], ["Today high", (r) => coins(r.high)], ["Now", (r) => coins(r.now)], ["Listed", listed]],
  },
];

class OpportunitiesView extends Component {
  watch(s) {
    return [s.opps];
  }

  setup() {
    this.addEventListener("click", (event) => {
      if (event.target.closest("button:not(.row-link)")) return; // buttons in a row do their own thing
      const row = event.target.closest("tr[data-id]");
      if (row) store.openItem(Number(row.dataset.id));
    });
  }

  render({ opps }) {
    if (!opps) return html`<p class="empty">Loading…</p>`;
    return html`
      <header class="page-head">
        <h1>Opportunities</h1>
        <p class="sub">${count(opps.busy)} busy items (${opps.minListed}+ listed) of ${count(opps.items)}; buys up to ${coins(opps.budget)}.</p>
      </header>
      <check-list></check-list>
      <section class="market" aria-labelledby="market-title">
        <header class="section-head">
          <h2 id="market-title">Market today</h2>
          <p class="section-note">From the latest price scan. Click a row to open the item.</p>
        </header>
        ${SECTIONS.map((section) => sectionCard(section, opps[section.key] || []))}
      </section>`;
  }
}

function sectionCard({ title, why, columns }, rows) {
  const shown = rows.slice(0, ROWS_PER_SECTION);
  return html`
    <section class="card">
      <h3>${title} <span class="count">${rows.length}</span></h3>
      <p class="why">${why}</p>
      ${shown.length
        ? html`
            <div class="table-scroll"><table>
              <thead>
                <tr><th>Item</th>${columns.map(([label]) => html`<th>${label}</th>`)}</tr>
              </thead>
              <tbody>
                ${shown.map(
                  (r) => html`<tr data-id="${r.id}" class="clickable"><td><button class="row-link" type="button">${r.name}</button></td>${columns.map(([, value]) => html`<td>${value(r)}</td>`)}</tr>`,
                )}
              </tbody>
            </table></div>`
        : html`<p class="muted">Nothing right now.</p>`}
    </section>`;
}

define("opportunities-view", OpportunitiesView);
