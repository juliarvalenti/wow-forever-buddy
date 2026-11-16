// "Check these ladders": today's picks and recently searched items, each with what its latest
// ladder says (market.mjs ladderVerdict), and a button that copies the unsearched ones as an
// Auctionator shopping list.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { ago } from "../lib/format.js";
import { auctionatorList } from "../lib/round.js";
import { copyText } from "../lib/api.js";
import { verdictPill, planText, hideButton, hiddenNote } from "./bits.js";

const needsSearch = (row) => row.kind === "todo" || row.kind === "stale";

class CheckList extends Component {
  watch(s) {
    return [s.opps];
  }

  setup() {
    this.addEventListener("click", async (event) => {
      if (!event.target.closest("[data-copy-todo]")) return;
      const names = (store.state.opps?.check || []).filter(needsSearch).map((row) => row.name);
      const copied = await copyText(auctionatorList(names));
      if (copied) this.ref("note").textContent = "Copied. In Auctionator: Shopping, Import, paste.";
    });
  }

  render({ opps }) {
    const rows = opps?.check || [];
    if (!rows.length) return "";
    const toSearch = rows.filter(needsSearch);
    const good = rows.filter((row) => row.kind === "gap");

    return html`
      <section class="card check-list">
        <h3>Check these ladders <span class="count">${good.length} good · ${toSearch.length} to search</span></h3>
        <p class="why">
          Search each at the AH (open it in Auctionator or the Buy tab), then Save prices &amp; reload: every search records the
          item’s whole ladder and the verdict updates here. Good gap = buy the cheap levels, relist 1c under the next one, after
          the 5% cut and a deposit, within your budget.
        </p>
        <div class="tools">
          <button class="btn" type="button" data-copy-todo ${toSearch.length ? "" : "disabled"}>
            Copy the ${toSearch.length} to search as an Auctionator list
          </button>
          <span class="muted" data-ref="note"></span>
        </div>
        <table>
          <thead>
            <tr><th>Verdict</th><th>Item</th><th>Why</th><th>Searched</th><th>Plan</th><th></th></tr>
          </thead>
          <tbody>
            ${rows.map(
              (row) => html`
                <tr data-id="${row.id}" class="clickable">
                  <td>${verdictPill(row.kind)}</td>
                  <td>${row.name}</td>
                  <td class="muted">${row.why.join(", ")}</td>
                  <td class="muted">${ago(row.at)}</td>
                  <td class="plan">${planText(row)}</td>
                  <td>${hideButton(row)}</td>
                </tr>`,
            )}
          </tbody>
        </table>
        ${hiddenNote(opps)}
      </section>`;
  }
}

define("check-list", CheckList);
