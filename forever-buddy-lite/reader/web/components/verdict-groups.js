// Worklist rows grouped by what to do with them. Used by the Opportunities worklist and the round
// dialog's results, so both read the same way.
//
// A row is an opps.check entry: { id, name, kind, text, why, at, gap?, sell? }.

import { html } from "../lib/html.js";
import { ago, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { hideButton } from "./bits.js";

// In the order you'd act on them. Closed groups are reference, not tasks.
export const GROUPS = [
  { kinds: ["gap"], tone: "act", title: "Buy these", note: "A gap within your budget: buy the cheap levels, relist 1c under the next one.", open: true, layout: "gap" },
  { kinds: ["todo", "stale"], tone: "todo", title: "Search these at the AH", note: "Open each item so its listings show, then save.", open: true, layout: "why" },
  { kinds: ["yours"], tone: "quiet", title: "You’re the cheapest", note: "Your listing is the floor. Nothing to do.", open: true, layout: "text" },
  { kinds: ["big"], tone: "quiet", title: "Gap, but over your budget", note: "Real gaps that need more gold than you’ve set aside.", open: false, layout: "gap" },
  { kinds: ["tight"], tone: "quiet", title: "No gap", note: "The cheap price is the real price right now.", open: false, layout: "text" },
  { kinds: ["empty"], tone: "quiet", title: "Nobody selling", note: "Nothing listed when you searched.", open: false, layout: "text" },
];

export function verdictGroups(rows, { searched = true } = {}) {
  return GROUPS.map((group) => {
    const members = rows.filter((row) => group.kinds.includes(row.kind));
    if (!members.length) return "";
    return html`
      <details class="verdict-group tone-${group.tone}" ${group.open ? "open" : ""}>
        <summary>
          <span class="group-title">${group.title}</span>
          <span class="group-count">${members.length}</span>
          <span class="group-note">${group.note}</span>
        </summary>
        <div class="table-scroll">
          <table class="worklist">
            ${head(group.layout, searched)}
            <tbody>${members.map((row) => line(row, group.layout, searched))}</tbody>
          </table>
        </div>
      </details>`;
  });
}

function head(layout, searched) {
  const when = searched ? html`<th class="num">Searched</th>` : "";
  if (layout === "gap") {
    return html`<thead><tr><th>Item</th><th class="num">Buy</th><th class="num">Up to</th><th class="num">Costs</th><th class="num">Relist at</th><th class="num">Profit</th>${when}<th></th></tr></thead>`;
  }
  if (layout === "why") return html`<thead><tr><th>Item</th><th class="text">Why it’s here</th><th></th></tr></thead>`;
  return html`<thead><tr><th>Item</th><th class="text">What the ladder says</th>${when}<th></th></tr></thead>`;
}

function line(row, layout, searched) {
  const when = searched ? html`<td class="num muted">${ago(row.at)}</td>` : "";
  const name = html`<td class="item-cell"><button class="row-link item-name" type="button">${row.name}</button>${row.sell && html`<span class="sell-hint">${row.sell.text}</span>`}</td>`;
  const hide = html`<td class="row-action">${hideButton(row)}</td>`;

  if (layout === "gap" && row.gap) {
    const g = row.gap;
    return html`
      <tr data-id="${row.id}" class="clickable">
        ${name}
        <td class="num">${count(g.qty)}</td>
        <td class="num">${coins(g.upTo)}</td>
        <td class="num">${coins(g.cost)}</td>
        <td class="num">${coins(g.relist)}</td>
        <td class="num profit">${coins(g.profit, { signed: true })}</td>
        ${when}${hide}
      </tr>`;
  }
  if (layout === "why") {
    return html`<tr data-id="${row.id}" class="clickable">${name}<td class="text muted">${row.why.join(", ")}</td>${hide}</tr>`;
  }
  return html`<tr data-id="${row.id}" class="clickable">${name}<td class="text">${row.text}</td>${when}${hide}</tr>`;
}
