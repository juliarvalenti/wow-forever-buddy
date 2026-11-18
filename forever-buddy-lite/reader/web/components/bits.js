// Small pieces of markup shared by several views (the generic ones are in lib/ui.js).

import { html } from "../lib/html.js";
import { VERDICT } from "../lib/round.js";
import { count } from "../lib/format.js";
import { pill } from "../lib/ui.js";

// A ladder verdict as a pill: what to do reads in its colour.
const VERDICT_TONE = { gap: "ok", todo: "warn", stale: "warn", big: "bronze", yours: "info", tight: "quiet", empty: "quiet" };

export function verdictPill(kind) {
  return pill(VERDICT[kind]?.label ?? kind, VERDICT_TONE[kind] || "");
}

// app.js handles every [data-hide] / [data-unhide] click.
export function hideButton(row) {
  return html`<button class="btn quiet small danger" type="button" data-hide="${row.id}" data-name="${row.name}" title="Never suggest this item">Hide</button>`;
}

export function hiddenNote(opps) {
  const hidden = opps.hidden || [];
  const vendor = opps.skippedVendor
    ? `${count(opps.skippedVendor)} items that vendors sell without limit left out (water, thread, vials, arrows). `
    : "";
  if (!vendor && !hidden.length) return "";
  return html`
    <p class="hidden-note">
      ${vendor}
      ${hidden.length > 0 &&
      html`Hidden:
      ${hidden.map(
        (item, i) => html`${i > 0 && ", "}${item.name}
          <button class="btn quiet small" type="button" data-unhide="${item.id}" data-name="${item.name}">Show</button>`,
      )}`}
    </p>`;
}
