// One measure by day, as SVG: <day-chart kind="price"> (the day's lowest listing as a line, the
// low-to-high range as a band) or <day-chart kind="listed"> (quantity up, as bars). One measure per
// chart, never two axes. Hovering a day shows its numbers.
//
// Not tied to the store: the item view calls chart.show(item).

import { html } from "../lib/html.js";
import { money, count } from "../lib/format.js";

const WIDTH = 640;
const HEIGHT = 200;
const PAD = { left: 56, right: 12, top: 12, bottom: 26 };

class DayChart extends HTMLElement {
  show(item) {
    this.item = item;
    const kind = this.getAttribute("kind");
    const days = item.h; // [day, low, high, listed]
    const plotW = WIDTH - PAD.left - PAD.right;
    const plotH = HEIGHT - PAD.top - PAD.bottom;

    const values = kind === "price" ? days.flatMap((d) => [d[1], d[2]]) : days.map((d) => d[3]);
    const max = Math.max(...values) * 1.08 || 1;
    const min = kind === "price" ? Math.min(...values) * 0.92 : 0;
    const slot = plotW / days.length;
    const x = (i) => PAD.left + (i + 0.5) * slot;
    const y = (v) => PAD.top + plotH - ((v - min) / (max - min || 1)) * plotH;
    const label = kind === "price" ? money : (v) => count(Math.round(v));
    const ticks = [0, 0.25, 0.5, 0.75, 1].map((t) => min + (max - min) * t);

    const grid = ticks.map(
      (t) => html`
        <line x1="${PAD.left}" x2="${WIDTH - PAD.right}" y1="${y(t)}" y2="${y(t)}" stroke="var(--grid)" />
        <text x="${PAD.left - 8}" y="${y(t) + 4}" text-anchor="end">${label(t)}</text>`,
    );
    const dayLabels = days.map((d, i) => html`<text x="${x(i)}" y="${HEIGHT - 6}" text-anchor="middle">${d[0]}</text>`);
    const marks = kind === "price" ? priceMarks(days, x, y) : listedMarks(days, x, y, slot, PAD.top + plotH);
    // Invisible columns, one per day, to hover.
    const hoverTargets = days.map(
      (d, i) => html`<rect x="${PAD.left + i * slot}" y="${PAD.top}" width="${slot}" height="${plotH}" fill="transparent" data-day="${i}" />`,
    );
    const name = kind === "price" ? "Daily lowest price" : "Quantity listed";

    this.innerHTML = String(html`
      <svg viewBox="0 0 ${WIDTH} ${HEIGHT}" role="img" aria-label="${name} for ${item.n}">
        ${grid}${dayLabels}${marks}${hoverTargets}
      </svg>
      <div class="tooltip" hidden></div>`);
    this.#hover();
  }

  #hover() {
    const svg = this.querySelector("svg");
    const tip = this.querySelector(".tooltip");
    svg.addEventListener("mousemove", (event) => {
      const target = event.target.closest("rect[data-day]");
      if (!target) {
        tip.hidden = true;
        return;
      }
      const [day, low, high, listed] = this.item.h[Number(target.dataset.day)];
      tip.innerHTML = String(
        this.getAttribute("kind") === "price"
          ? html`<b>${day}</b><br />Low ${money(low)}<br />High ${money(high)}`
          : html`<b>${day}</b><br />${count(listed)} listed`,
      );
      const box = this.getBoundingClientRect();
      tip.hidden = false;
      tip.style.left = `${Math.min(event.clientX - box.left + 12, box.width - 140)}px`;
      tip.style.top = `${event.clientY - box.top - 10}px`;
    });
    svg.addEventListener("mouseleave", () => {
      tip.hidden = true;
    });
  }
}

// Line through the daily lows, a band up to the daily highs, a dot per day.
function priceMarks(days, x, y) {
  const n = days.length;
  const band =
    n > 1 &&
    html`<path fill="var(--band)" d="M${days.map((d, i) => `${x(i)},${y(d[2])}`).join("L")}L${[...days]
      .reverse()
      .map((d, j) => `${x(n - 1 - j)},${y(d[1])}`)
      .join("L")}Z" />`;
  const line =
    n > 1 &&
    html`<path fill="none" stroke="var(--series-1)" stroke-width="2" stroke-linejoin="round" d="M${days.map((d, i) => `${x(i)},${y(d[1])}`).join("L")}" />`;
  const dots = days.map(
    (d, i) => html`<circle cx="${x(i)}" cy="${y(d[1])}" r="4" fill="var(--series-1)" stroke="var(--surface-1)" stroke-width="2" />`,
  );
  return html`${band}${line}${dots}`;
}

// A bar per day, rounded at the top.
function listedMarks(days, x, y, slot, baseline) {
  const barW = Math.min(48, slot * 0.5);
  return days.map((d, i) => {
    const top = y(d[3]);
    const left = x(i) - barW / 2;
    return html`<path fill="var(--series-1)" d="M${left},${baseline}V${top + 4}q0,-4 4,-4H${left + barW - 4}q4,0 4,4V${baseline}Z" />`;
  });
}

customElements.define("day-chart", DayChart);
