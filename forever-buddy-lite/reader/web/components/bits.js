// Small pieces of markup shared by several views.

import { html } from "../lib/html.js";
import { VERDICT } from "../lib/round.js";
import { count } from "../lib/format.js";

export function verdictPill(kind) {
  return html`<span class="pill pill-${kind}">${VERDICT[kind]?.label ?? kind}</span>`;
}

// A worklist row's plan, plus where to relist if you hold the item.
export function planText(row) {
  return html`${row.text}${row.sell && html`<br /><span class="up">${row.sell.text}</span>`}`;
}

// app.js handles every [data-hide] / [data-unhide] click.
export function hideButton(row) {
  return html`<button class="hide-button" type="button" data-hide="${row.id}" data-name="${row.name}" title="Never suggest this item">Hide</button>`;
}

export function hiddenNote(opps) {
  const hidden = opps.hidden || [];
  const vendor = opps.skippedVendor
    ? `${count(opps.skippedVendor)} items that vendors sell without limit left out (water, thread, vials, arrows). `
    : "";
  if (!vendor && !hidden.length) return "";
  return html`
    <p class="sub">
      ${vendor}
      ${hidden.length > 0 &&
      html`Hidden:
      ${hidden.map(
        (item, i) => html`${i > 0 && ", "}${item.name}
          <button class="hide-button" type="button" data-unhide="${item.id}" data-name="${item.name}">Show</button>`,
      )}`}
    </p>`;
}
