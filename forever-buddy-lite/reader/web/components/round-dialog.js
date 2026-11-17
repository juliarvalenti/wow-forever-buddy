// The round dialog (see lib/round.js): after a price scan, what to search; after your save, what
// the ladders say. A native <dialog>: Esc or a click on the backdrop minimizes it to the header.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html } from "../lib/html.js";
import { clockTime } from "../lib/format.js";
import { roundPicks, auctionatorList } from "../lib/round.js";
import { copyText } from "../lib/api.js";
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
      if (copied) event.target.textContent = `Copied ${left.length}: paste in Auctionator, Shopping, Import`;
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
    const progress = picks.length ? (checked.length / picks.length) * 100 : 0;

    return html`
      <dialog class="round" aria-labelledby="round-title">
        <header class="round-head">
          <h2 id="round-title" tabindex="-1" autofocus>${round.step === "check" ? "New price scan" : "Ladders are in"}</h2>
          <span class="muted">scan finished at ${clockTime(round.scanAt)}</span>
          <button class="btn" type="button" data-action="minimize">Minimize</button>
        </header>
        <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="${picks.length}" aria-valuenow="${checked.length}">
          <div style="width:${progress}%"></div>
        </div>
        <p class="muted">${checked.length} of ${picks.length} picks searched since this scan</p>
        ${round.step === "check" ? checkStep(round, opps, picks, left) : resultsStep(checked, left)}
      </dialog>`;
  }

  rendered() {
    const dialog = this.querySelector("dialog");
    if (dialog && !dialog.open) dialog.showModal();
  }
}

// Still searching: what's left to open, then what's already in, grouped like the worklist.
function checkStep(round, opps, picks, left) {
  const done = picks.filter((row) => !left.includes(row));
  return html`
    <ol class="steps">
      <li><button class="btn primary" type="button" data-action="copy">Copy ${left.length} to search</button> then in Auctionator: Shopping, Import, paste.</li>
      <li>In that list, <b>click each item</b> so its listings open. Running the list search alone isn’t enough: only opening an item records its ladder.</li>
      <li>Save prices &amp; reload, or type /fbl r. This page updates by itself.</li>
    </ol>
    <p class="waiting"><span class="pulse" aria-hidden="true"></span>Waiting for your save</p>
    ${left.length > 0 &&
    html`<ul class="to-search">
      ${left.map((row) => html`<li><span class="item-name">${row.name}</span><span class="muted">${row.why.join(", ")}</span>${hideButton(row)}</li>`)}
    </ul>`}
    ${verdictGroups(done, { searched: false })}
    ${done.length > 0 && html`<div class="tools"><button class="btn" type="button" data-action="results">Done searching: show results</button></div>`}`;
}

function resultsStep(checked, left) {
  return html`
    ${verdictGroups(checked, { searched: false })}
    ${left.length > 0 && html`<p class="muted">Not searched: ${left.map((row) => row.name).join(", ")}</p>`}
    <div class="tools">
      ${left.length > 0 && html`<button class="btn" type="button" data-action="copy">Copy ${left.length} not searched</button>`}
      <button class="btn primary" type="button" data-action="finish">Finish round</button>
    </div>`;
}

define("round-dialog", RoundDialog);
