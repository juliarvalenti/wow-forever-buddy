// The round dialog (see lib/round.js): after a price scan, what to search; after your save, what
// the ladders say. A native <dialog>: Esc or a click on the backdrop minimizes it to the header.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { clockTime } from "../lib/format.js";
import { roundPicks, auctionatorList } from "../lib/round.js";
import { copyText } from "../lib/api.js";
import { toast } from "../lib/toast.js";
import { icon, panel, meter, callout, itemIcon } from "../lib/ui.js";
import { hideButton } from "./bits.js";
import { verdictGroups } from "./verdict-groups.js";

class RoundDialog extends Component {
  watch(s) {
    return [s.round, s.opps];
  }

  setup() {
    this.addEventListener("click", (event) => this.#onClick(event));
    // "close" doesn't bubble; listen in the capture phase.
    this.addEventListener(
      "close",
      () => {
        if (store.state.round.open) store.setRound({ open: false });
      },
      true,
    );
  }

  async #onClick(event) {
    const dialog = this.querySelector("dialog");
    if (event.target === dialog) return dialog.close(); // the backdrop
    const action = event.target.closest("[data-action]")?.dataset.action;
    if (action === "minimize") return store.setRound({ open: false });
    if (action === "finish") return store.setRound({ step: "done", open: false });
    if (action === "results") return store.setRound({ step: "results" });
    if (action === "copy") {
      const { left } = roundPicks(store.state.round, store.state.opps);
      const copied = await copyText(auctionatorList(left.map((row) => row.name)));
      if (copied) {
        const note = `Copied ${left.length}: paste in Auctionator, Shopping, Import`;
        event.target.closest("[data-action]").textContent = note;
        toast(note);
      }
      return;
    }
    if (event.target.closest("button:not(.row-link)")) return;
    const row = event.target.closest("tr[data-id]");
    if (row) {
      store.setRound({ open: false });
      store.openItem(Number(row.dataset.id));
    }
  }

  render({ round, opps }) {
    if (!round.open || round.step === "done" || !opps) return "";
    const { picks, checked, left } = roundPicks(round, opps);
    const check = round.step === "check";

    return html`
      <dialog class="dialog wide round" aria-labelledby="round-title">
        <header class="dialog-head">
          <span class="dot live" aria-hidden="true"></span>
          <h2 id="round-title" tabindex="-1" autofocus>${check ? "New price scan" : "Ladders are in"}</h2>
          <span>scan finished at ${clockTime(round.scanAt)}</span>
          <div class="actions"><button class="btn small" type="button" data-action="minimize">${icon("min", "sm")}Minimize</button></div>
        </header>
        <div class="dialog-body">
          <div class="round-progress">
            ${meter(picks.length ? checked.length / picks.length : 0, { tone: "ember", label: "Picks searched" })}
            <span>${checked.length} of ${picks.length} picks searched since this scan</span>
          </div>
          ${check ? checkStep(picks, left) : resultsStep(checked, left)}
        </div>
        <footer class="dialog-foot">
          ${check
            ? html`<span class="waiting"><span class="spinner" aria-hidden="true"></span>Waiting for your save</span>
                ${picks.length > left.length && html`<button class="btn push" type="button" data-action="results">Done searching: show results</button>`}`
            : html`${left.length > 0 && html`<button class="btn" type="button" data-action="copy">Copy ${left.length} not searched</button>`}
                <button class="btn primary push" type="button" data-action="finish">Finish round</button>`}
        </footer>
      </dialog>`;
  }

  rendered() {
    const dialog = this.querySelector("dialog");
    if (dialog && !dialog.open) dialog.showModal();
  }
}

// Still searching: what's left to open, then what's already in, grouped like the worklist.
function checkStep(picks, left) {
  const done = picks.filter((row) => !left.includes(row));
  return html`
    <ol class="steps">
      <li><span><button class="btn primary small" type="button" data-action="copy">${icon("copy", "sm")}Copy ${left.length} to search</button> then in Auctionator: Shopping, Import, paste.</span></li>
      <li><span>In that list, <b>click each item</b> so its listings open. Running the list search alone isn’t enough: only opening an item records its ladder.</span></li>
      <li><span>Save prices &amp; reload, or type /fbl r. This page updates by itself.</span></li>
    </ol>
    ${left.length > 0 &&
    panel({
      title: "Still to search",
      level: 3,
      count: left.length,
      flush: html`<ul class="rows to-search">
        ${left.map((row) => html`<li>${itemIcon(row.name)}<span class="main">${row.name}</span>${hideButton(row)}<span class="why">${row.why.join(", ")}</span></li>`)}
      </ul>`,
    })}
    ${verdictGroups(done, { searched: false })}`;
}

function resultsStep(checked, left) {
  return html`
    ${verdictGroups(checked, { searched: false })}
    ${left.length > 0 && callout(html`<b>Not searched:</b> ${left.map((row) => row.name).join(", ")}`, { icon: "clock" })}`;
}

define("round-dialog", RoundDialog);
