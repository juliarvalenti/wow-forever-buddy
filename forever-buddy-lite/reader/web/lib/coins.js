// Money as the game shows it: numbers with gold, silver and copper coins (styled by .coins in
// styles/components.css).
//
//   coins(12345)       ->  1 (gold coin) 23 (silver coin)       copper dropped once there's gold
//   coins(50000)       ->  5 (gold coin)                       a zero second denomination is left off
//   coins(405)         ->  4 (silver) 05 (copper)
//   coins(87)          ->  87 (copper)
//   coins(68120000)    ->  6,812 (gold)                         gold gets thousands separators
//   coins(-500, { signed: true }) -> −5 (silver); signed: true adds "+" to gains
//
// Each coin carries its letter for screen readers and copy-paste, and the whole amount is in a title.
// For a gain/loss in colour, wrap it: html`<span class="up">${coins(x, { signed: true })}</span>`.

import { html } from "./html.js";
import { money } from "./format.js";

function part(amount, metal, letter) {
  return html`<span class="part ${letter}">${amount}<i class="coin ${metal}"><span class="letter">${letter}</span></i></span>`;
}

export function coins(copper, { signed = false } = {}) {
  if (copper == null || Number.isNaN(copper)) return html`<span class="coins muted">–</span>`;
  const negative = copper < 0;
  const c = Math.abs(Math.round(copper));
  const gold = Math.floor(c / 10000);
  const silver = Math.floor((c % 10000) / 100);
  const cop = c % 100;
  const sign = negative ? "−" : signed && c > 0 ? "+" : "";

  // Two denominations at most, and a zero second one is left off (5g, not 5g 0s), as the game does.
  let parts;
  if (gold) parts = html`${part(gold.toLocaleString("en-US"), "gold", "g")}${silver > 0 && part(silver, "silver", "s")}`;
  else if (silver) parts = html`${part(silver, "silver", "s")}${cop > 0 && part(String(cop).padStart(2, "0"), "copper", "c")}`;
  else parts = part(cop, "copper", "c");

  return html`<span class="coins" title="${sign}${money(c)}">${sign}${parts}</span>`;
}
