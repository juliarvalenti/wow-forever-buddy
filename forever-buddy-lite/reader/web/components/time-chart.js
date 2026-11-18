// One item over time, on a real time axis: <time-chart kind="price"> (the cheapest listing at each
// scan, as steps, over a band for each day's low-to-high range from Auctionator) or
// <time-chart kind="listed"> (how many were up at each scan). Hovering shows the nearest scan.
// Colours and type come from the chart tokens (.chart in styles/components.css): price is
// --chart-1, quantity --chart-2.
//
// Not tied to the store: the item view calls chart.show({ points, days, start, end }).
//   points  [[unix seconds, cheapest, listed], ...] from /history.json, oldest first
//   days    [{ start, end, low, high, listed }] from Auctionator, unix seconds
//   start, end  the time range shown

import { html, raw } from "../lib/html.js";
import { money, count } from "../lib/format.js";
import { coins } from "../lib/coins.js";

// The drawing is as wide as the chart is on screen (at least MIN_WIDTH), so its type stays at the
// system's sizes on a phone instead of shrinking with the viewBox.
const MIN_WIDTH = 320;
const DEFAULT_WIDTH = 720;
const HEIGHT = 220;
const PAD = { left: 60, right: 14, top: 12, bottom: 28 };
const HOUR = 3600;
const DAY = 86400;

function timeLabel(seconds, span) {
  const d = new Date(seconds * 1000);
  if (span <= 2 * DAY) return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

function whenLabel(seconds) {
  return new Date(seconds * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

class TimeChart extends HTMLElement {
  connectedCallback() {
    this.classList.add("chart");
  }

  show({ points, days, start, end }) {
    this.w = Math.max(MIN_WIDTH, Math.round(this.clientWidth) || DEFAULT_WIDTH);
    const kind = this.getAttribute("kind");
    const price = kind === "price";
    const series1 = price ? "1" : "2";
    const span = Math.max(end - start, HOUR);
    const visible = points.filter((p) => p[0] >= start && p[0] <= end);
    // The scan just before the range still says what the price was when it starts.
    const before = points.filter((p) => p[0] < start).at(-1);
    const shownDays = days.filter((d) => d.end > start && d.start < end);

    // Without scans in range, fall back to one point per day (the day's low, or its quantity).
    const usingDays = visible.length === 0;
    const series = usingDays
      ? shownDays.map((d) => [Math.min(Math.max((d.start + d.end) / 2, start), end), d.low, d.listed])
      : [...(before ? [[start, before[1], before[2]]] : []), ...visible];
    this.series = series;
    this.kind = kind;

    if (!series.length) {
      this.innerHTML = String(html`<p class="note">Nothing in this range.</p>`);
      return;
    }

    const valueOf = (p) => (price ? p[1] : p[2]);
    const values = series.map(valueOf);
    if (price) for (const d of shownDays) values.push(d.low, d.high);
    // Both axes start at zero: a money (or quantity) axis cut off above zero makes small moves look big.
    const low = 0;
    const high = Math.max(...values) * 1.08 || 1;

    const plotW = this.w - PAD.left - PAD.right;
    const plotH = HEIGHT - PAD.top - PAD.bottom;
    const x = (t) => PAD.left + ((t - start) / span) * plotW;
    const y = (v) => PAD.top + plotH - ((v - low) / (high - low || 1)) * plotH;
    this.scale = { x, start, span, plotW };

    const yTicks = [0, 0.25, 0.5, 0.75, 1].map((f) => low + (high - low) * f);
    const xTicks = (this.w < 520 ? [0, 0.5, 1] : [0, 0.25, 0.5, 0.75, 1]).map((f) => start + span * f);
    const label = price ? money : (v) => count(Math.round(v));

    // Each day's low-to-high, clipped to the range.
    const bands = price
      ? shownDays.map((d) => {
          const left = x(Math.max(d.start, start));
          const right = x(Math.min(d.end, end));
          return html`<rect class="band" x="${left}" y="${y(d.high)}" width="${Math.max(1, right - left)}" height="${Math.max(1, y(d.low) - y(d.high))}" />`;
        })
      : [];
    // Steps: a price holds until the next scan; the last one carries on to the end of the range.
    const stepEnd = usingDays ? series.at(-1)[0] : Math.min(end, Date.now() / 1000);
    const path = series.map((p, i) => (i === 0 ? `M${x(p[0])},${y(valueOf(p))}` : `H${x(p[0])}V${y(valueOf(p))}`)).join("") + `H${x(stepEnd)}`;
    const dots = (usingDays ? series : visible).map(
      (p) => html`<circle class="pt f${series1}" cx="${x(p[0])}" cy="${y(valueOf(p))}" r="${series.length > 60 ? 2 : 3.5}" />`,
    );

    const name = price ? "Cheapest listing at each scan" : "Quantity listed at each scan";
    this.innerHTML = String(html`
      ${usingDays ? html`<p class="note">No scans in this range: one point per day from Auctionator.</p>` : ""}
      <svg viewBox="0 0 ${this.w} ${HEIGHT}" role="img" aria-label="${name}">
        <g class="grid">
          ${yTicks.map((v, i) => html`<line class="${i === 0 ? "zero" : ""}" x1="${PAD.left}" x2="${this.w - PAD.right}" y1="${y(v)}" y2="${y(v)}" />`)}
        </g>
        <g class="axis">
          ${yTicks.map((v) => html`<text x="${PAD.left - 8}" y="${y(v) + 4}" text-anchor="end">${label(v)}</text>`)}
          ${xTicks.map((t, i) => html`<text x="${x(t)}" y="${HEIGHT - 8}" text-anchor="${i === 0 ? "start" : i === xTicks.length - 1 ? "end" : "middle"}">${timeLabel(t, span)}</text>`)}
        </g>
        ${bands}
        ${raw(`<path class="line s${series1}" d="${path}" />`)}
        ${dots}
        <line class="cross" x1="0" x2="0" y1="${PAD.top}" y2="${PAD.top + plotH}" visibility="hidden" />
        <rect x="${PAD.left}" y="${PAD.top}" width="${plotW}" height="${plotH}" fill="transparent" />
      </svg>
      <div class="tip" hidden></div>`);
    this.#hover();
  }

  // The scan nearest the pointer.
  #hover() {
    const svg = this.querySelector("svg");
    const tip = this.querySelector(".tip");
    const cross = this.querySelector(".cross");
    const price = this.kind === "price";
    svg.addEventListener("mousemove", (event) => {
      const box = svg.getBoundingClientRect();
      const px = ((event.clientX - box.left) / box.width) * this.w;
      const { x, start, span, plotW } = this.scale;
      const t = start + ((px - PAD.left) / plotW) * span;
      let best = null;
      for (const p of this.series) if (!best || Math.abs(p[0] - t) < Math.abs(best[0] - t)) best = p;
      if (!best || px < PAD.left || px > this.w - PAD.right) {
        tip.hidden = true;
        cross.setAttribute("visibility", "hidden");
        return;
      }
      cross.setAttribute("x1", x(best[0]));
      cross.setAttribute("x2", x(best[0]));
      cross.setAttribute("visibility", "visible");
      const key = (n) => `<i class="key" style="--key:var(--chart-${n})"></i>`;
      tip.innerHTML = String(html`
        <div class="when">${whenLabel(best[0])}</div>
        <div class="row"><span>${price ? raw(key(1)) : ""}Cheapest</span>${coins(best[1])}</div>
        <div class="row"><span>${price ? "" : raw(key(2))}Listed</span>${count(best[2])}</div>`);
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

customElements.define("time-chart", TimeChart);
