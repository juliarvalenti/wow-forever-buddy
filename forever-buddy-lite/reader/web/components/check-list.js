// The ladder worklist: today's picks and recently searched items, grouped by what to do with them
// (see verdict-groups.js), with a button that copies the unsearched ones as an Auctionator list.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { auctionatorList } from "../lib/round.js";
import { copyText } from "../lib/api.js";
import { hiddenNote } from "./bits.js";
import { verdictGroups } from "./verdict-groups.js";

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

    return html`
      <section class="worklist-panel">
        <header class="section-head">
          <h2>Price ladders</h2>
          <p class="section-note">
            Every item you open at the AH records its whole ladder. Save prices &amp; reload and the verdicts below update.
          </p>
          ${toSearch.length > 0 &&
          html`<div class="tools">
            <button class="btn primary" type="button" data-copy-todo>Copy ${toSearch.length} to search as an Auctionator list</button>
            <span class="muted" data-ref="note"></span>
          </div>`}
        </header>
        ${verdictGroups(rows)}
        ${hiddenNote(opps)}
      </section>`;
  }
}

define("check-list", CheckList);
