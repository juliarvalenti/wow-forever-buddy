// Wealth over time, as SVG: total gold on all characters as steps (it changes in jumps: a sale, a
// quest, a repair), and gold plus what bags and bank would fetch, as points joined by a line (it's
// recorded at each save). Both in copper on one axis that starts at zero. Hovering shows the values
// at that moment.
//
// It sits on parchment (a .record: wealth is a record of what happened), so the chart tokens are the
// ledger's inks: the total in black ink (--chart-1), gold in amber ink (--chart-2).
//
// Not tied to the store: the wealth view calls chart.show(gold, history).

import { html, raw } from "../lib/html.js";
import { money } from "../lib/format.js";
import { coins } from "../lib/coins.js";
import { legend } from "../lib/ui.js";

// The drawing is as wide as the chart is on screen (at least MIN_WIDTH), so its type stays at the
// system's sizes on a phone instead of shrinking with the viewBox.
const MIN_WIDTH = 320;
const DEFAULT_WIDTH = 760;
const HEIGHT = 240;
const PAD = { left: 64, right: 16, top: 14, bottom: 28 };

// The value of a step series at time t: the last point at or before it.
function stepAt(points, t) {
  let value = null;
  for (const [pt, v] of points) {
    if (pt > t) break;
    value = v;
  }
  return value;
}

function timeLabel(seconds, spanSeconds) {
  const d = new Date(seconds * 1000);
  if (spanSeconds < 2 * 86400) return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

class WealthChart extends HTMLElement {
  connectedCallback() {
    this.classList.add("chart");
  }

  // gold: [[t, copper]]; total: [[t, copper]] (gold + items).
  show(gold, total) {
    this.w = Math.max(MIN_WIDTH, Math.round(this.clientWidth) || DEFAULT_WIDTH);
    const now = Date.now() / 1000;
    const times = [...gold, ...total].map((p) => p[0]);
    if (!times.length) {
      this.innerHTML = "";
      return;
    }
    let start = Math.min(...times);
    let end = Math.max(now, ...times);
    if (end - start < 3600) start = end - 3600; // at least an hour wide
    const span = end - start;

    // Gold steps carry on to now.
    const goldLine = gold.length ? [...gold, [end, gold.at(-1)[1]]] : [];
    const values = [...goldLine, ...total].map((p) => p[1]);
    // The axis starts at zero gold: cut off above zero, small moves would look big.
    const min = 0;
    const max = Math.max(...values) * 1.08 || 1;

    const plotW = this.w - PAD.left - PAD.right;
    const plotH = HEIGHT - PAD.top - PAD.bottom;
    const x = (t) => PAD.left + ((t - start) / span) * plotW;
    const y = (v) => PAD.top + plotH - ((v - min) / (max - min)) * plotH;

    const yTicks = [0, 0.25, 0.5, 0.75, 1].map((f) => min + (max - min) * f);
    const xTicks = (this.w < 520 ? [0, 0.5, 1] : [0, 0.25, 0.5, 0.75, 1]).map((f) => start + span * f);

    const stepPath = goldLine.map(([t, v], i) => (i === 0 ? `M${x(t)},${y(v)}` : `H${x(t)}V${y(v)}`)).join("");
    const totalPath = total.map(([t, v], i) => `${i ? "L" : "M"}${x(t)},${y(v)}`).join("");

    this.innerHTML = String(html`
      ${legend([
        { label: "Gold + items at that save’s prices", color: "var(--chart-1)" },
        { label: "Gold on all characters", color: "var(--chart-2)" },
      ])}
      <svg viewBox="0 0 ${this.w} ${HEIGHT}" role="img" aria-label="Gold, and gold plus item value, over time">
        <g class="grid">
          ${yTicks.map((v, i) => html`<line class="${i === 0 ? "zero" : ""}" x1="${PAD.left}" x2="${this.w - PAD.right}" y1="${y(v)}" y2="${y(v)}" />`)}
        </g>
        <g class="axis">
          ${yTicks.map((v) => html`<text x="${PAD.left - 8}" y="${y(v) + 4}" text-anchor="end">${money(v)}</text>`)}
          ${xTicks.map((t, i) => html`<text x="${x(t)}" y="${HEIGHT - 8}" text-anchor="${i === 0 ? "start" : i === xTicks.length - 1 ? "end" : "middle"}">${timeLabel(t, span)}</text>`)}
        </g>
        ${stepPath && raw(`<path class="line s2" d="${stepPath}" />`)}
        ${totalPath && raw(`<path class="line s1 total" d="${totalPath}" />`)}
        ${total.map(([t, v]) => html`<circle class="pt f1" cx="${x(t)}" cy="${y(v)}" r="3.5" />`)}
        <line class="cross" x1="0" x2="0" y1="${PAD.top}" y2="${PAD.top + plotH}" visibility="hidden" />
        <rect x="${PAD.left}" y="${PAD.top}" width="${plotW}" height="${plotH}" fill="transparent" />
      </svg>
      <div class="tip" hidden></div>`);

    const svg = this.querySelector("svg");
    const tip = this.querySelector(".tip");
    const cross = this.querySelector(".cross");
    const key = (n) => raw(`<i class="key" style="--key:var(--chart-${n})"></i>`);
    svg.addEventListener("mousemove", (event) => {
      const box = svg.getBoundingClientRect();
      const px = ((event.clientX - box.left) / box.width) * this.w;
      if (px < PAD.left || px > this.w - PAD.right) {
        tip.hidden = true;
        cross.setAttribute("visibility", "hidden");
        return;
      }
      const t = start + ((px - PAD.left) / plotW) * span;
      const g = stepAt(goldLine, t);
      const w = stepAt(total, t);
      cross.setAttribute("x1", px);
      cross.setAttribute("x2", px);
      cross.setAttribute("visibility", "visible");
      tip.innerHTML = String(html`
        <div class="when">${new Date(t * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}</div>
        <div class="row"><span>${key(2)}Gold</span>${g == null ? "–" : coins(g)}</div>
        <div class="row total"><span>${key(1)}Gold + items</span>${w == null ? "–" : coins(w)}</div>`);
      const host = this.getBoundingClientRect();
      tip.hidden = false;
      tip.style.left = `${Math.max(0, Math.min(event.clientX - host.left + 12, host.width - tip.offsetWidth))}px`;
      tip.style.top = `${event.clientY - host.top - tip.offsetHeight - 8}px`;
    });
    svg.addEventListener("mouseleave", () => {
      tip.hidden = true;
      cross.setAttribute("visibility", "hidden");
    });
  }
}

customElements.define("wealth-chart", WealthChart);
