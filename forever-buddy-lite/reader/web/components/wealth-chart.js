// Wealth over time, as SVG: total gold on all characters as steps (it changes in jumps: a sale, a
// quest, a repair), and gold plus what bags and bank would fetch, as points joined by a line (it's
// recorded at each save). Both in copper on one axis. Hovering shows the values at that moment.
//
// Not tied to the store: the wealth view calls chart.show(gold, history).

import { html, raw } from "../lib/html.js";
import { money } from "../lib/format.js";

const WIDTH = 760;
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
  // gold: [[t, copper]]; total: [[t, copper]] (gold + items).
  show(gold, total) {
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
    const low = Math.min(...values);
    const high = Math.max(...values);
    const pad = Math.max((high - low) * 0.1, 500); // never a flat line pressed to the edge
    const min = Math.max(0, low - pad);
    const max = high + pad;

    const plotW = WIDTH - PAD.left - PAD.right;
    const plotH = HEIGHT - PAD.top - PAD.bottom;
    const x = (t) => PAD.left + ((t - start) / span) * plotW;
    const y = (v) => PAD.top + plotH - ((v - min) / (max - min)) * plotH;

    const yTicks = [0, 0.25, 0.5, 0.75, 1].map((f) => min + (max - min) * f);
    const xTicks = [0, 0.25, 0.5, 0.75, 1].map((f) => start + span * f);

    const stepPath = goldLine
      .map(([t, v], i) => (i === 0 ? `M${x(t)},${y(v)}` : `H${x(t)}V${y(v)}`))
      .join("");
    const totalPath = total.map(([t, v], i) => `${i ? "L" : "M"}${x(t)},${y(v)}`).join("");

    this.innerHTML = String(html`
      <div class="chart-legend">
        <span><i class="swatch swatch-gold"></i>Gold on all characters</span>
        <span><i class="swatch swatch-series"></i>Gold + items at that save’s prices</span>
      </div>
      <svg viewBox="0 0 ${WIDTH} ${HEIGHT}" role="img" aria-label="Gold, and gold plus item value, over time">
        ${yTicks.map(
          (v) => html`
            <line x1="${PAD.left}" x2="${WIDTH - PAD.right}" y1="${y(v)}" y2="${y(v)}" stroke="var(--rule-soft)" />
            <text x="${PAD.left - 8}" y="${y(v) + 4}" text-anchor="end">${money(v)}</text>`,
        )}
        ${xTicks.map((t) => html`<text x="${x(t)}" y="${HEIGHT - 8}" text-anchor="middle">${timeLabel(t, span)}</text>`)}
        ${stepPath && raw(`<path d="${stepPath}" fill="none" stroke="var(--gold)" stroke-width="2" stroke-linejoin="round" />`)}
        ${totalPath && raw(`<path d="${totalPath}" fill="none" stroke="var(--series)" stroke-width="2" stroke-linejoin="round" />`)}
        ${total.map(([t, v]) => html`<circle cx="${x(t)}" cy="${y(v)}" r="3.5" fill="var(--series)" stroke="var(--panel)" stroke-width="2" />`)}
        <line class="crosshair" x1="0" x2="0" y1="${PAD.top}" y2="${PAD.top + plotH}" stroke="var(--ink-3)" stroke-dasharray="3 3" visibility="hidden" />
        <rect class="hover-area" x="${PAD.left}" y="${PAD.top}" width="${plotW}" height="${plotH}" fill="transparent" />
      </svg>
      <div class="tooltip" hidden></div>`);

    const svg = this.querySelector("svg");
    const tip = this.querySelector(".tooltip");
    const cross = this.querySelector(".crosshair");
    svg.addEventListener("mousemove", (event) => {
      const box = svg.getBoundingClientRect();
      const px = ((event.clientX - box.left) / box.width) * WIDTH;
      if (px < PAD.left || px > WIDTH - PAD.right) {
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
        <b>${new Date(t * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}</b><br />
        Gold ${g == null ? "–" : money(g)}<br />
        Gold + items ${w == null ? "–" : money(w)}`);
      const host = this.getBoundingClientRect();
      tip.hidden = false;
      tip.style.left = `${Math.min(event.clientX - host.left + 12, host.width - 180)}px`;
      tip.style.top = `${event.clientY - host.top - 10}px`;
    });
    svg.addEventListener("mouseleave", () => {
      tip.hidden = true;
      cross.setAttribute("visibility", "hidden");
    });
  }
}

customElements.define("wealth-chart", WealthChart);
