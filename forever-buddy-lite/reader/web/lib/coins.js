// Money as the game shows it: numbers with gold, silver and copper coins.
//
//   coins(12345)  ->  1 (gold coin) 23 (silver coin)       copper dropped once there's gold
//   coins(50000)  ->  5 (gold coin)                       a zero second denomination is left off
//   coins(405)    ->  4 (silver) 05 (copper)
//   coins(87)     ->  87 (copper)
//
// Each coin carries its letter for screen readers and copy-paste, and the whole amount is in a title.

import { html } from "./html.js";
import { money } from "./format.js";

function coin(amount, metal, letter) {
  return html`<span class="amount">${amount}</span><span class="coin coin-${metal}"><span class="letter">${letter}</span></span>`;
}

export function coins(copper, { signed = false } = {}) {
  if (copper == null) return html`<span class="coins">–</span>`;
  const negative = copper < 0;
  const c = Math.abs(Math.round(copper));
  const gold = Math.floor(c / 10000);
  const silver = Math.floor((c % 10000) / 100);
  const cop = c % 100;
  const sign = negative ? "−" : signed ? "+" : "";

  // Two denominations at most, and a zero second one is left off (5g, not 5g 0s), as the game does.
  let parts;
  if (gold) parts = html`${coin(gold, "gold", "g")}${silver > 0 && coin(silver, "silver", "s")}`;
  else if (silver) parts = html`${coin(silver, "silver", "s")}${cop > 0 && coin(String(cop).padStart(2, "0"), "copper", "c")}`;
  else parts = coin(cop, "copper", "c");

  return html`<span class="coins" title="${sign}${money(c)}">${sign}${parts}</span>`;
}
