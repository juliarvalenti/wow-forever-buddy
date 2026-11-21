// "Market indexes" at the top of the Opportunities page: how whole markets move, from your saved
// scans (indexes.rs). Each index is a basket of busy trade goods weighted by market size, 100 at the
// start of the week. A tile per index (value, 24h change, sparkline); pick one to see its chart and
// the items that moved it most. The axis isn't anchored at zero here: an index is a ratio to its own
// start (100 = unchanged), so the dashed line at 100 is the reference, not zero.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import { html, raw } from "../lib/html.js";
import { fetchIndexes } from "../lib/api.js";
import { panel, loadingState, errorState } from "../lib/ui.js";

const W = 720;
const H = 200;
const PAD = { left: 44, right: 12, top: 12, bottom: 24 };

const pct = (f) => `${f > 0 ? "+" : ""}${(f * 100).toFixed(1)}%`;
const tone = (f) => (f > 0.0005 ? "up" : f < -0.0005 ? "down" : "muted");
const when = (t) => new Date(t * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });

// A tiny line, no axes, for a tile.
function sparkline(series) {
  if (series.length < 2) return "";
  const vs = series.map((p) => p[1]);
  const lo = Math.min(...vs, 100);
  const hi = Math.max(...vs, 100);
  const t0 = series[0][0];
  const span = series.at(-1)[0] - t0 || 1;
  const x = (t) => ((t - t0) / span) * 120;
  const y = (v) => 30 - ((v - lo) / (hi - lo || 1)) * 28 - 1;
  const d = series.map(([t, v], i) => `${i ? "L" : "M"}${x(t).toFixed(1)},${y(v).toFixed(1)}`).join("");
  return raw(`<svg class="index-spark" viewBox="0 0 120 30" preserveAspectRatio="none" aria-hidden="true">
    <line x1="0" x2="120" y1="${y(100).toFixed(1)}" y2="${y(100).toFixed(1)}" class="index-base" />
    <path d="${d}" class="index-line" /></svg>`);
}

class IndexBoard extends Component {
  #data = null;
  #error = null;
  #loadedFor = undefined;
  #open = "WoW 50";

  watch(s) {
    return [s.lastUpdate];
  }

  setup() {
    this.addEventListener("click", (event) => {
      const pick = event.target.closest("[data-index]");
      if (pick) {
        this.#open = pick.dataset.index;
        this.refresh(true);
        return;
      }
      const item = event.target.closest("[data-open-item]");
      if (item) store.openItem(Number(item.dataset.openItem));
    });
    this.addEventListener("mousemove", (event) => this.#hover(event));
    this.addEventListener("mouseleave", () => this.#hover(null));
  }

  render() {
    const base = { title: "Market indexes", id: "indexes-title" };
    if (this.#error) return panel({ ...base, body: errorState({ title: "Couldn’t load indexes", text: this.#error, compact: true }) });
    if (!this.#data) return panel({ ...base, body: loadingState("Loading…", { compact: true }) });
    const { indexes, latest, scans } = this.#data;
    if (!indexes.length) {
      return panel({ ...base, body: html`<p class="note">Indexes need a few saved scans with new prices. Scan the AH a couple of times.</p>` });
    }
    const open = indexes.find((i) => i.name === this.#open) || indexes[0];
    return panel({
      ...base,
      note: html`As of the last AH scan, ${when(latest)} · ${scans} scans this week`,
      body: html`
        <div class="index-tiles" role="list">
          ${indexes.map(
            (i) => html`
              <button type="button" class="index-tile ${i.name === open.name ? "on" : ""}" data-index="${i.name}" role="listitem"
                aria-pressed="${i.name === open.name}">
                <span class="index-name">${i.name}</span>
                <span class="index-value">${i.value.toFixed(1)}</span>
                <span class="index-change ${tone(i.change24h)}">${pct(i.change24h)} <span class="muted">24h</span></span>
                ${sparkline(i.series)}
              </button>`,
          )}
        </div>
        <div class="index-detail">
          <div class="index-detail-head">
            <strong>${open.name}</strong>
            <span class="muted">${open.members} items · week ${html`<span class="${tone(open.changeAll)}">${pct(open.changeAll)}</span>`}</span>
          </div>
          <div class="index-chart" data-chart="1">${this.#chart(open)}<div class="tip" hidden></div></div>
          <div class="index-movers">
            <span class="muted">Moved it most, 24h:</span>
            ${open.movers.map(
              (m) => html`<button type="button" class="row-link" data-open-item="${m.id}">${m.name}
                <span class="${tone(m.change24h)}">${pct(m.change24h)}</span></button>`,
            )}
          </div>
        </div>`,
    });
  }

  #chart(index) {
    const s = index.series;
    const vs = s.map((p) => p[1]);
    const lo = Math.floor(Math.min(...vs, 100) / 5) * 5 - 5;
    const hi = Math.ceil(Math.max(...vs, 100) / 5) * 5 + 5;
    const t0 = s[0][0];
    const span = s.at(-1)[0] - t0 || 1;
    const pw = W - PAD.left - PAD.right;
    const ph = H - PAD.top - PAD.bottom;
    const x = (t) => PAD.left + ((t - t0) / span) * pw;
    const y = (v) => PAD.top + ph - ((v - lo) / (hi - lo)) * ph;
    this.scale = { t0, span, pw, series: s, x, y };
    const ticks = [lo, (lo + hi) / 2, hi].map((v) => Math.round(v));
    const days = [0, 0.25, 0.5, 0.75, 1].map((f) => t0 + span * f);
    const d = s.map(([t, v], i) => `${i ? "L" : "M"}${x(t).toFixed(1)},${y(v).toFixed(1)}`).join("");
    return raw(`<svg viewBox="0 0 ${W} ${H}" role="img" aria-label="${index.name} index over the week">
      ${ticks.map((v) => `<line x1="${PAD.left}" x2="${W - PAD.right}" y1="${y(v)}" y2="${y(v)}" class="grid" /><text x="${PAD.left - 6}" y="${y(v) + 4}" text-anchor="end">${v}</text>`).join("")}
      <line x1="${PAD.left}" x2="${W - PAD.right}" y1="${y(100)}" y2="${y(100)}" class="index-base" />
      ${days.map((t, i) => `<text x="${x(t)}" y="${H - 6}" text-anchor="${i === 0 ? "start" : i === days.length - 1 ? "end" : "middle"}">${new Date(t * 1000).toLocaleDateString([], { month: "short", day: "numeric" })}</text>`).join("")}
      <path d="${d}" class="index-line big" />
      <line class="crosshair" x1="0" x2="0" y1="${PAD.top}" y2="${PAD.top + ph}" visibility="hidden" />
      <circle class="crossdot" r="4" cx="0" cy="0" visibility="hidden" />
    </svg>`);
  }

  // The scan nearest the pointer on the big chart.
  #hover(event) {
    const box = this.querySelector("[data-chart] svg");
    const tip = this.querySelector("[data-chart] .tip");
    const cross = box?.querySelector(".crosshair");
    const dot = box?.querySelector(".crossdot");
    if (!box || !tip || !this.scale) return;
    const r = box.getBoundingClientRect();
    const inside = event && event.clientX >= r.left && event.clientX <= r.right && event.clientY >= r.top && event.clientY <= r.bottom;
    if (!inside) {
      tip.hidden = true;
      cross.setAttribute("visibility", "hidden");
      dot.setAttribute("visibility", "hidden");
      return;
    }
    const { t0, span, pw, series, x, y } = this.scale;
    const t = t0 + ((((event.clientX - r.left) / r.width) * W - PAD.left) / pw) * span;
    let best = series[0];
    for (const p of series) if (Math.abs(p[0] - t) < Math.abs(best[0] - t)) best = p;
    cross.setAttribute("x1", x(best[0]));
    cross.setAttribute("x2", x(best[0]));
    cross.setAttribute("visibility", "visible");
    dot.setAttribute("cx", x(best[0]));
    dot.setAttribute("cy", y(best[1]));
    dot.setAttribute("visibility", "visible");
    tip.innerHTML = String(html`<b>${when(best[0])}</b><br />${best[1].toFixed(1)} <span class="muted">(${pct(best[1] / 100 - 1)} on the week)</span>`);
    const host = this.querySelector("[data-chart]").getBoundingClientRect();
    tip.hidden = false;
    tip.style.left = `${Math.min(event.clientX - host.left + 12, host.width - 200)}px`;
    tip.style.top = `${event.clientY - host.top - 10}px`;
  }

  async rendered({ lastUpdate }) {
    if (this.#loadedFor === lastUpdate && (this.#data || this.#error)) return;
    this.#loadedFor = lastUpdate;
    try {
      this.#data = await fetchIndexes();
      this.#error = null;
    } catch (error) {
      this.#error = error.message;
    }
    this.refresh(true);
  }
}

define("index-board", IndexBoard);
