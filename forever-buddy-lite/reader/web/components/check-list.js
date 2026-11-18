// The ladder worklist: today's picks and recently searched items, grouped by what to do with them
// (see verdict-groups.js), with a button that copies the unsearched ones as an Auctionator list.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { auctionatorList } from "../lib/round.js";
import { copyText } from "../lib/api.js";
import { toast } from "../lib/toast.js";
import { icon, sectionHead } from "../lib/ui.js";
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
      if (copied) {
        const note = "Copied. In Auctionator: Shopping, Import, paste.";
        this.ref("note").textContent = note;
        toast(note);
      }
    });
  }

  render({ opps }) {
    const rows = opps?.check || [];
    if (!rows.length) return "";
    const toSearch = rows.filter(needsSearch);

    return html`
      <section class="worklist-panel" aria-labelledby="ladders-title">
        ${sectionHead({
          title: "Price ladders",
          id: "ladders-title",
          note: "Every item you open at the AH records its whole ladder. Save prices & reload and the verdicts below update.",
          actions:
            toSearch.length > 0 &&
            html`<button class="btn primary" type="button" data-copy-todo>${icon("copy")}Copy ${toSearch.length} to search as an Auctionator list</button>
              <span class="muted small" data-ref="note"></span>`,
        })}
        ${verdictGroups(rows)}
        ${hiddenNote(opps)}
      </section>`;
  }
}

define("check-list", CheckList);
