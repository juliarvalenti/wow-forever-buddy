// The living style guide (#/design, linked from the footer): every token and component of the
// design system, in its states, drawn by the same CSS and helpers the pages use. If something here
// looks wrong, every page that uses it looks wrong too. See DESIGN.md for how to build a page.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";
import { coins } from "../lib/coins.js";
import { ICONS } from "../lib/icons.js";
import { toast } from "../lib/toast.js";
import {
  icon, pageHead, sectionHead, panel, tiles, tile, table, pill, seg, legend, callout,
  emptyState, loadingState, errorState, itemIcon, meter, signed, character,
} from "../lib/ui.js";
import "./time-chart.js";
import "./wealth-chart.js";
import "./fb-tabs.js";

const SECTIONS = [
  ["materials", "Materials"],
  ["colour", "Colour"],
  ["type", "Type"],
  ["space", "Space and shape"],
  ["buttons", "Buttons"],
  ["forms", "Forms"],
  ["labels", "Labels"],
  ["panels", "Panels and tiles"],
  ["tables", "Tables"],
  ["messages", "Messages and states"],
  ["overlays", "Overlays"],
  ["data", "Money and charts"],
  ["icons", "Icons"],
];

// Token swatches: [token, what it's for]
const PALETTE = [
  ["Stone", [["--c-chalk-hi", "strong text"], ["--c-chalk", "text"], ["--c-ash", "muted text"], ["--c-soot", "faint text"], ["--c-stone-hi", "raised"], ["--c-stone-a", "slab top"], ["--c-stone-b", "slab foot"], ["--c-floor", "the hall"], ["--c-edge", "the 1px black edge"]]],
  ["Leather", [["--c-hide", "sidebar"], ["--c-hide-2", "sidebar shade"], ["--c-strap-a", "strap top"], ["--c-strap-b", "strap foot"], ["--c-hide-head", "strap title"], ["--c-hide-text", "on leather"], ["--c-hide-mute", "muted on leather"]]],
  ["Parchment", [["--c-paper", "a record"], ["--c-paper-2", "its shade"], ["--c-ink", "ink"], ["--c-ink-2", "faded ink"], ["--c-ink-3", "faint ink"]]],
  ["Accents", [["--c-bronze-hi", "bronze: the one primary action"], ["--c-bronze", "bronze"], ["--c-ember", "ember: live, pending, selected"], ["--c-ember-2", "ember glow, focus"], ["--c-ribbon", "ribbon: you are here"], ["--c-wax", "wax seal"]]],
  ["Meaning", [["--c-ok", "up, gain, done"], ["--c-bad", "down, loss, error"], ["--c-warn", "check this"], ["--c-info", "a note"], ["--c-ok-ink", "up, on paper"], ["--c-bad-ink", "down, on paper"]]],
  ["Coins (only coins are gold)", [["--c-gold", "gold coin"], ["--c-silver", "silver coin"], ["--c-copper", "copper coin"]]],
  ["Charts", [["--chart-1", "the main series"], ["--chart-2", "the second"], ["--chart-3", "third"], ["--chart-4", "fourth"], ["--chart-5", "fifth"], ["--chart-grid", "grid lines"], ["--chart-axis", "axis labels"]]],
  ["Classes", ["warrior", "paladin", "hunter", "rogue", "priest", "shaman", "mage", "warlock", "druid"].map((c) => [`--class-${c}`, c])],
];

const ROLES = [
  ["Text", [["--text-strong", "titles, figures"], ["--text", "body"], ["--text-muted", "secondary"], ["--text-faint", "notes, column heads"], ["--text-link", "links"], ["--text-display", "page titles"]]],
  ["Lines", [["--line", "a rule between rows"], ["--line-strong", "under a table head"], ["--line-stitch", "stitching"]]],
  ["Meaning", [["--accent", "live, selected, focus"], ["--up", "gain"], ["--down", "loss"], ["--warn", "warning"], ["--info", "information"]]],
];

const TYPE = [
  ["--fs-display", "Page title", "display"],
  ["--fs-3xl", "Page title on a phone", "display"],
  ["--fs-2xl", "6,812 tile value", "strong"],
  ["--fs-xl", "Record title", "record"],
  ["--fs-l", "Panel title", "strong"],
  ["--fs-m", "Body text: prices from your own scans.", ""],
  ["--fs-s", "Tables and ledes", ""],
  ["--fs-xs", "Labels, notes, column heads", "muted"],
  ["--fs-2xs", "Pills and chart axes", "muted"],
];

const SPACE = ["--space-1", "--space-2", "--space-3", "--space-4", "--space-5", "--space-6", "--space-7", "--space-8", "--space-9", "--space-10", "--space-11", "--space-12"];
const SHADOWS = ["--shadow-slab", "--shadow-raised", "--shadow-bronze", "--shadow-well", "--shadow-strap", "--shadow-popover", "--shadow-paper", "--shadow-glass"];

const swatch = ([token, use]) => html`
  <div class="ds-swatch">
    <div class="chip-color" style="background: var(${token})"></div>
    <code>${token}</code>
    <span class="use">${use}</span>
  </div>`;

const demo = (label, content) => html`<div class="ds-demo"><div class="ds-label">${label}</div>${content}</div>`;

// Example data for the tables and charts (made up, so the guide works without a scan).
const SAMPLE = [
  { id: 1, name: "Arcanite Bar", now: 364000, usual: 411000, listed: 84 },
  { id: 2, name: "Runecloth", now: 11200, usual: 10800, listed: 3120 },
  { id: 3, name: "Black Lotus", now: 820000, usual: 750000, listed: 4 },
  { id: 4, name: "Major Healing Potion", now: 18500, usual: 18900, listed: 211 },
  { id: 5, name: "Flask of the Titans", now: 640000, usual: 620000, listed: 12 },
];

class DesignView extends Component {
  watch() {
    return []; // static: drawn once
  }

  setup() {
    this.addEventListener("click", (event) => {
      const jump = event.target.closest("[data-jump]");
      if (jump) document.getElementById(`ds-${jump.dataset.jump}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
      const segButton = event.target.closest(".seg [aria-pressed]");
      if (segButton && this.contains(segButton) && segButton.closest("[data-demo]")) {
        for (const b of segButton.parentElement.querySelectorAll("button")) b.setAttribute("aria-pressed", String(b === segButton));
      }
      const chip = event.target.closest(".chip");
      if (chip && chip.closest("[data-demo]")) chip.setAttribute("aria-pressed", String(chip.getAttribute("aria-pressed") !== "true"));
      const toastButton = event.target.closest("[data-toast]");
      if (toastButton) toast(toastButton.dataset.msg, { tone: toastButton.dataset.toast });
      if (event.target.closest("[data-open-dialog]")) this.querySelector("dialog.ds-dialog").showModal();
      if (event.target.closest("[data-close-dialog]")) this.querySelector("dialog.ds-dialog").close();
      const dialog = this.querySelector("dialog.ds-dialog");
      if (event.target === dialog) dialog.close(); // the backdrop
    });
  }

  render() {
    return html`
      ${pageHead({
        title: "Design system",
        lede: html`The Forever Buddy app’s design language “D”, for the Lite dashboard: stone and leather for the frame and live data,
          parchment for records, bronze for the one primary action, ember for live, gold for coins only.
          Every piece below is the real CSS; <code class="mono">DESIGN.md</code> says how to build a page with it.`,
        actions: html`<a class="btn" href="#/opportunities">${icon("left", "sm")}Back to the market</a>`,
      })}
      <nav class="ds-nav" aria-label="Sections of the style guide">
        <div class="chips">${SECTIONS.map(([id, label]) => html`<button class="chip" type="button" data-jump="${id}">${label}</button>`)}</div>
      </nav>
      ${this.#materials()} ${this.#colour()} ${this.#type()} ${this.#space()} ${this.#buttons()} ${this.#forms()} ${this.#labels()}
      ${this.#panels()} ${this.#tables()} ${this.#messages()} ${this.#overlays()} ${this.#data()} ${this.#icons()}`;
  }

  rendered() {
    // The charts: two days of a made-up item, and a week of made-up wealth. Both axes start at zero.
    const end = Date.now() / 1000;
    const start = end - 2 * 86400;
    const points = [];
    for (let t = start + 1800, i = 0; t < end; t += 5400, i++) {
      points.push([t, Math.round(52000 + Math.sin(i / 3) * 6000 + (i % 7) * 400), Math.round(900 + Math.cos(i / 4) * 260)]);
    }
    const days = [0, 1, 2].map((d) => {
      const s = Math.floor(start / 86400) * 86400 + d * 86400;
      return { start: s, end: s + 86400, low: 44000 + d * 1500, high: 61000 - d * 900, listed: 1000 };
    });
    for (const chart of this.querySelectorAll("time-chart")) chart.show({ points, days, start, end });
    const gold = [];
    const total = [];
    for (let i = 0; i < 9; i++) {
      const t = end - 7 * 86400 + i * 0.8 * 86400;
      gold.push([t, 4200000 + i * 210000 + (i % 3) * 90000]);
      total.push([t, 6100000 + i * 260000 - (i % 2) * 120000]);
    }
    this.querySelector("wealth-chart")?.show(gold, total);
  }

  #section(id, title, note, body) {
    return html`<section class="ds-section" id="ds-${id}" aria-labelledby="ds-${id}-title">${sectionHead({ title, note, id: `ds-${id}-title` })}${body}</section>`;
  }

  #materials() {
    return this.#section(
      "materials",
      "Materials",
      "Each material has one job (design/mocks/round-2). Read a page by its materials before its words.",
      html`
        <div class="ds-grid wide">
          ${panel({ title: "Stone", meta: "the frame, live data", body: html`<p class="note">Panels, tiles, tables of current state. A slab with a black edge, lit from above, with a leather strap for a head.</p>` })}
          ${panel({ title: "Parchment", record: true, meta: "records", body: html`<p class="note">Things that happened: the trade ledger, wealth over time. Inked in Georgia.</p>` })}
          <div class="panel"><div class="panel-body">
            <div class="ds-row"><button class="btn primary" type="button">Bronze</button></div>
            <p class="note">The <b>one</b> primary action on a view. Riveted, dull sheen.</p>
          </div></div>
          <div class="panel"><div class="panel-body">
            <div class="ds-row"><span class="dot live"></span><span class="strong">Ember</span>${pill("live", "warn")}</div>
            <p class="note">Live or pending: the connection, a new scan, the selected row, focus.</p>
          </div></div>
          <div class="panel"><div class="panel-body">
            <div class="ds-row">${coins(68124709)}</div>
            <p class="note">Gold: coins only. Nothing else on the page is gold.</p>
          </div></div>
          <div class="side ds-side-sample" style="height:auto;border-radius:var(--radius)"><nav class="nav" style="padding:var(--space-6) var(--space-5)">
            <a class="nav-link" aria-current="page">${icon("trend")}<span class="lbl">Ribbon</span></a>
            <a class="nav-link">${icon("scale")}<span class="lbl">Leather</span></a>
          </nav></div>
        </div>`,
    );
  }

  #colour() {
    return this.#section(
      "colour",
      "Colour",
      html`Two layers: the raw palette (<code class="mono">--c-*</code>, only used in tokens.css) and roles (what a component asks for).
        A <code class="mono">.record</code> re-points the roles to ink, so the same component reads right on paper.`,
      html`
        ${PALETTE.map(([group, list]) => html`<div class="ds-demo"><div class="ds-label">${group}</div><div class="ds-grid">${list.map(swatch)}</div></div>`)}
        <div class="cols even">
          ${panel({ title: "Roles on stone", body: ROLES.map(([group, list]) => demo(group, html`<div class="ds-grid">${list.map(swatch)}</div>`)) })}
          ${panel({ title: "The same roles on parchment", record: true, body: ROLES.map(([group, list]) => demo(group, html`<div class="ds-grid">${list.map(swatch)}</div>`)) })}
        </div>`,
    );
  }

  #type() {
    return this.#section(
      "type",
      "Type",
      "Geist for the interface, Marcellus for page titles and the wordmark only, Georgia on parchment. Sentence case; no capitals or letter-spacing for labels. Figures are tabular.",
      html`
        ${panel({
          title: "Scale",
          body: html`<div class="ds-specimen">
            ${TYPE.map(([token, text, kind]) => html`
              <code>${token}</code>
              <span class="${kind === "display" ? "display" : kind === "muted" ? "muted" : kind === "strong" ? "strong" : ""}"
                style="font-size: var(${token}); ${kind === "record" ? "font-family: var(--font-record); font-weight: 700" : ""}">${text}</span>`)}
          </div>`,
        })}
        <div class="cols even">
          ${panel({
            title: "Families",
            body: html`<div class="ds-specimen">
              <code>--font-ui</code><span>Geist: the interface, 13px body</span>
              <code>--font-display</code><span class="display" style="font-size:20px">Marcellus: Opportunities</span>
              <code>--font-record</code><span style="font-family:var(--font-record);font-style:italic">Georgia, on parchment</span>
              <code>--font-mono</code><span class="mono">/fbl r · item 4338</span>
            </div>`,
          })}
          ${panel({
            title: "Text roles",
            body: html`<div class="stack tight">
              <span class="strong">Strong: titles and figures</span><span>Text: the body</span><span class="muted">Muted: secondary lines</span>
              <span class="faint">Faint: notes and column heads</span><span><a href="#/design">A link</a> and <button class="link" type="button">a link-button</button></span>
              <span><span class="up">+12% up</span> · <span class="down">−8% down</span> · <span class="warn">a warning</span></span>
            </div>`,
          })}
        </div>`,
    );
  }

  #space() {
    return this.#section(
      "space",
      "Space and shape",
      "A 2px-based scale; radii stay small (the app is carved, not rounded); every slab, button and tooltip has its 1px black edge.",
      html`
        <div class="cols even">
          ${panel({ title: "Spacing", body: html`<div class="stack tight">${SPACE.map((t) => html`<div class="ds-space"><i style="width: var(${t})"></i><code class="mono">${t}</code></div>`)}</div>` })}
          ${panel({
            title: "Radii and controls",
            body: html`
              <div class="ds-grid">
                ${["--radius-s", "--radius", "--radius-l", "--radius-pill"].map((t) => html`<div class="ds-swatch"><div class="ds-box" style="border-radius:var(${t});background:var(--surface-raised);box-shadow:var(--shadow-raised)"></div><code>${t}</code></div>`)}
              </div>
              <div class="facts">
                <dt>--control-h</dt><dd>28px buttons</dd><dt>--control-h-s</dt><dd>24px small buttons, segmented</dd>
                <dt>--control-h-l</dt><dd>32px the bronze button</dd><dt>--field-h</dt><dd>30px inputs and selects</dd>
                <dt>--row-h</dt><dd>40px a table row</dd>
              </div>`,
          })}
        </div>
        ${panel({
          title: "Elevation",
          body: html`<div class="ds-grid">${SHADOWS.map((t) => html`<div class="ds-swatch"><div class="ds-box" style="box-shadow:var(${t});background:${t === "--shadow-paper" ? "var(--surface-paper)" : t === "--shadow-glass" ? "var(--surface-glass)" : "var(--surface-panel)"}"></div><code>${t}</code></div>`)}</div>`,
        })}
        ${panel({
          title: "Motion",
          body: html`<div class="facts">
            <dt>--dur-fast</dt><dd>120ms: hovers, toggles</dd><dt>--dur</dt><dd>200ms: a page or dialog rising in</dd>
            <dt>--dur-slow</dt><dd>2.2s: the ember’s smoulder <span class="dot live"></span></dd><dt>--ease</dt><dd>cubic-bezier(.2, .7, .3, 1)</dd>
            <dt>reduced motion</dt><dd>everything stops (base.css)</dd>
          </div>`,
        })}`,
    );
  }

  #buttons() {
    return this.#section(
      "buttons",
      "Buttons",
      "Stone for most actions, bronze for the one that matters on a view, quiet for the many small ones in a row. A danger button reddens; a locked one never looks clickable.",
      panel({
        title: "Variants and states",
        body: html`
          ${demo("Primary (bronze): one per view", html`<div class="ds-row">
            <button class="btn primary" type="button">${icon("copy")}Copy for Auctionator</button>
            <button class="btn primary small" type="button">Small</button>
            <button class="btn primary" type="button" disabled>Disabled</button></div>`)}
          ${demo("Secondary (stone)", html`<div class="ds-row">
            <button class="btn" type="button">Minimize</button><button class="btn" type="button">${icon("refresh", "sm")}With icon</button>
            <button class="btn small" type="button">Small</button><button class="btn icon" type="button" aria-label="Search">${icon("search")}</button>
            <button class="btn" type="button" disabled>Disabled</button></div>`)}
          ${demo("Quiet", html`<div class="ds-row">
            <button class="btn quiet" type="button">Reset to defaults</button><button class="btn quiet small" type="button">Show</button>
            <button class="btn quiet" type="button" disabled>Disabled</button></div>`)}
          ${demo("Danger", html`<div class="ds-row">
            <button class="btn danger" type="button">${icon("x", "sm")}Forget this item</button>
            <button class="btn quiet small danger" type="button">Hide</button>
            <button class="btn danger" type="button" disabled>Disabled</button></div>`)}
          ${demo("Links and locked", html`<div class="ds-row">
            <button class="link" type="button">A link-button</button><a href="#/design">A link</a>
            <span class="locked">${icon("lock", "sm")}Locked while WoW runs</span>
            <span class="record" style="padding:6px 10px"><button class="btn quiet" type="button">Quiet on paper</button></span></div>`)}`,
      }),
    );
  }

  #forms() {
    return this.#section(
      "forms",
      "Forms",
      "Fields are wells cut into the stone. Inputs, selects, checkboxes and radios are styled bare, so a plain <input> already looks right.",
      html`<div class="cols even" data-demo>
        ${panel({
          title: "Fields",
          body: html`
            <label class="search">${icon("search")}<input type="search" placeholder="Search items" aria-label="Search (demo)" /></label>
            <div class="two-up">
              <label class="field"><span class="field-label">Buy at most</span><input type="number" value="25" /><span class="field-help">silver per item</span></label>
              <label class="field"><span class="field-label">List name</span><input type="text" value="FBL Snipes" /><span class="field-help">a short tag is added</span></label>
            </div>
            <div class="two-up">
              <label class="field invalid"><span class="field-label">Cap at</span><input type="number" value="140" aria-invalid="true" /><span class="field-help">must be 100% or less</span></label>
              <label class="field"><span class="field-label">Disabled</span><input type="text" value="read only" disabled /></label>
            </div>
            <div class="two-up">
              <label class="field"><span class="field-label">Select</span><select><option>Name</option><option>Most listed</option></select></label>
              <label class="inline-field">Small select <select class="small"><option>Price, high first</option></select></label>
            </div>
            <label class="field"><span class="field-label">Text area</span><textarea>A note to self.</textarea></label>`,
        })}
        ${panel({
          title: "Choices",
          body: html`
            <fieldset class="fieldset">
              <legend>Checkboxes</legend>
              <label class="check"><input type="checkbox" checked />Only busy markets</label>
              <label class="check"><input type="checkbox" />Include what I hold <span class="hint">bags, bank and mail</span></label>
              <label class="check"><input type="checkbox" disabled />Disabled</label>
            </fieldset>
            <fieldset class="fieldset">
              <legend>Radio group</legend>
              <label class="radio"><input type="radio" name="ds-radio" checked />Usual price <span class="hint">Auctionator’s daily prices</span></label>
              <label class="radio"><input type="radio" name="ds-radio" />Seen dipping <span class="hint">your scans</span></label>
            </fieldset>
            ${demo("Segmented control (aria-pressed)", seg("ds-range", [["6h"], ["24h"], ["3d"], ["7d"], ["All"]], "24h", "Time range"))}
            ${demo("Chips: filter toggles with counts", html`<div class="chips">
              <button class="chip" type="button" aria-pressed="true">Cloth<span class="n">212</span></button>
              <button class="chip" type="button" aria-pressed="false">Herbs<span class="n">96</span></button>
              <button class="chip" type="button" aria-pressed="false">Rising<span class="n">40</span></button></div>`)}
            ${demo("Tabs (<fb-tabs>: click, or arrow keys)", html`<fb-tabs>
              <div class="tabs" role="tablist" aria-label="Demo tabs">
                <button type="button" role="tab" aria-controls="ds-tab-a" aria-selected="true">Price</button>
                <button type="button" role="tab" aria-controls="ds-tab-b">Quantity</button>
                <button type="button" role="tab" aria-controls="ds-tab-c">Values</button>
              </div>
              <div role="tabpanel" id="ds-tab-a" class="note" style="padding-top:8px">The cheapest listing at each scan.</div>
              <div role="tabpanel" id="ds-tab-b" class="note" style="padding-top:8px" hidden>How many were up at each scan.</div>
              <div role="tabpanel" id="ds-tab-c" class="note" style="padding-top:8px" hidden>Each day’s low and high.</div>
            </fb-tabs>`)}`,
        })}
      </div>`,
    );
  }

  #labels() {
    return this.#section(
      "labels",
      "Labels",
      "Pills name a state, badges count what wants you, dots say live or not.",
      panel({
        title: "Pills, badges, dots, item and character marks",
        body: html`
          ${demo("Pills", html`<div class="ds-row">${pill("Default")}${pill("Good gap", "ok")}${pill("Search it", "warn")}${pill("Over budget", "bad")}${pill("Yours", "info")}${pill("Manual", "bronze")}${pill("Safety", "violet")}${pill("Tight", "quiet")}</div>`)}
          ${demo("On parchment", html`<div class="record ds-row" style="padding:10px">${pill("Default")}${pill("Sold", "ok")}${pill("Late", "warn")}${pill("Lost", "bad")}</div>`)}
          ${demo("Badge and count", html`<div class="ds-row"><span class="badge">3</span><span class="badge">12</span><span>Buy these <span class="count">14</span></span></div>`)}
          ${demo("Status dots", html`<div class="ds-row"><span class="cluster"><span class="dot live"></span>Live</span><span class="cluster"><span class="dot ok"></span>Ok</span><span class="cluster"><span class="dot bad"></span>Offline</span><span class="cluster"><span class="dot"></span>Idle</span></div>`)}
          ${demo("Item letter tiles", html`<div class="ds-row">${itemIcon("Arcanite Bar", "sm")}${itemIcon("Runecloth")}${itemIcon("Black Lotus", "lg")}<span class="record" style="padding:6px">${itemIcon("Mageweave Cloth")}</span></div>`)}
          ${demo("Characters, in class colours", html`<div class="ds-row">${["Coinpurse ROGUE", "Thrandor PALADIN", "Velyra DRUID", "Brannic HUNTER", "Fizzwick MAGE", "Sela PRIEST", "Kaelor WARLOCK"].map((s) => character(...s.split(" ")))}</div>`)}`,
      }),
    );
  }

  #panels() {
    return this.#section(
      "panels",
      "Panels and tiles",
      "panel() in lib/ui.js: a stone slab with a leather strap for its head. Body is padded; flush runs a table edge to edge.",
      html`
        ${tiles([
          tile("Worth now", coins(98123456), "gold + items"),
          tile("Gold change", signed(coins(4120000, { signed: true }), 1), "since Nov 10"),
          tile("Listed", "9,324", "quantity, latest day"),
          tile("Change", html`<span class="down">−54%</span>`, "first to last day"),
        ])}
        <div class="cols even">
          ${panel({ title: "A panel", count: 12, meta: "from your scans", body: html`<p class="note">Title, a count, a quiet meta on the right.</p>`, foot: html`Seen <b>12 times</b> in 30 days` })}
          ${panel({ title: "With actions", actions: html`<button class="btn small" type="button">Edit</button><button class="link" type="button">All</button>`, body: html`<p class="note">Actions sit at the strap’s right end.</p>` })}
          ${panel({ title: "Act on this", tone: "act", count: 3, note: "the one panel that wants you", body: html`<p class="note">An ember edge: at most one per page.</p>` })}
          ${panel({ title: "Reference", tone: "quiet", count: 41, note: "recedes", body: html`<p class="note">A well, not a slab, for lists you rarely need.</p>` })}
          ${panel({ title: "Folds open", open: true, count: 5, body: html`<p class="note">open: true or false makes a &lt;details&gt; panel; the strap is its summary.</p>` })}
          ${panel({ title: "Folded", open: false, count: 9, tone: "quiet", body: html`<p class="note">Hidden until opened.</p>` })}
          ${panel({ title: "A record", record: true, meta: "Yesterday 19:40 · 3h 12m", body: html`<p class="note">On parchment, the head is ruled in ink and the opening line reads as a written note.</p><p>Body text in ink. <span class="up">+312g</span> <span class="down">−41g</span></p>` })}
          ${panel({ title: "Sections", body: html`<div class="stack tight"><div class="cluster"><span class="pill">.cluster</span><span class="pill">wraps</span><span class="pill">in a row</span></div><div class="stack tight"><span class="pill">.stack</span><span class="pill">a column</span></div></div>` })}
        </div>`,
    );
  }

  #tables() {
    const columns = [{ label: "Item" }, { label: "Now", cls: "num" }, { label: "Usual", cls: "num" }, { label: "vs usual", cls: "num" }, { label: "Listed", cls: "num" }];
    const row = (r, i) => html`
      <tr class="clickable ${i === 1 ? "sel" : ""}">
        <td class="item"><button class="row-link" type="button">${r.name}</button>${i === 0 && html`<span class="sub hint">relist at 41g 10s</span>`}</td>
        <td class="num">${coins(r.now)}</td><td class="num">${coins(r.usual)}</td>
        <td class="num ${r.now < r.usual ? "down" : "up"}">${Math.round((r.now / r.usual - 1) * 100)}%</td><td class="num">${r.listed}</td>
      </tr>`;
    return this.#section(
      "tables",
      "Tables",
      "table() in lib/ui.js. Figures right-aligned (.num), rows that open something are .clickable (the name is a .row-link button), the selected row has an ember edge. Wide tables scroll inside their box, never the page.",
      html`<div class="cols even">
        ${panel({ title: "On stone", count: SAMPLE.length, flush: table({ columns, rows: SAMPLE, row }) })}
        ${panel({ title: "Sticky head, capped height", meta: "sticky: true", flush: table({ columns, rows: [...SAMPLE, ...SAMPLE, ...SAMPLE], row, sticky: true, compact: true }) })}
        ${panel({ title: "On parchment", record: true, meta: "a ledger", flush: table({ columns, rows: SAMPLE.slice(0, 3), row, compact: true }) })}
        ${panel({ title: "Empty", flush: table({ columns, rows: [], row, empty: "Nothing right now." }) })}
      </div>`,
    );
  }

  #messages() {
    return this.#section(
      "messages",
      "Messages and states",
      "A callout says something about the page; a state stands in for content that is empty, loading or broken.",
      html`
        <div class="stack tight">
          ${callout(html`<b>Last scan: 3 days ago</b> · press Scan at the Auction House; prices land here on logout.`, { tone: "ember", icon: "clock" })}
          ${callout(html`<b>Couldn’t read the save.</b> The file is damaged; nothing was changed.`, { tone: "bad", icon: "alert" })}
          ${callout(html`<b>Saved.</b> 12 ladders came in with your reload.`, { tone: "ok", icon: "check" })}
          ${callout(html`<b>A note on stone</b>, with an ember edge: for tips that matter.`, { tone: "stone", icon: "spark" })}
          ${callout(html`A plain well: background information.`, { icon: "eye" })}
        </div>
        <div class="cols even">
          ${panel({ title: "Empty", body: emptyState({ title: "Nothing fits these settings", text: "Raise “Buy at most”, or lower “Profit per item”.", actions: html`<button class="btn" type="button">Reset to defaults</button>` }) })}
          ${panel({ title: "Loading", body: html`${loadingState("Loading prices…")}<div class="stack tight"><span class="skeleton tall" style="width:60%"></span><span class="skeleton"></span><span class="skeleton" style="width:80%"></span></div>` })}
          ${panel({ title: "Error", body: errorState({ title: "Couldn’t load trades", text: "journal.json: 500. Is fbl-rs live running?", actions: html`<button class="btn" type="button">${icon("refresh", "sm")}Try again</button>` }) })}
          ${panel({ title: "Compact, inside a panel", body: html`${emptyState({ text: "Not searched yet: open this item at the AH, then save.", compact: true })}${loadingState("Building…", { compact: true })}${errorState({ title: "Couldn’t build the list", text: "Bad request", compact: true })}` })}
        </div>`,
    );
  }

  #overlays() {
    return this.#section(
      "overlays",
      "Overlays",
      "Tooltips follow the pointer over charts; the glass one is the game’s own item tooltip. Dialogs are native <dialog> with a strap head; toasts are short news in the corner.",
      html`
        <div class="cols even">
          ${panel({
            title: "Tooltips",
            body: html`
              <div class="ds-row" style="align-items:flex-start">
                <div class="tip" style="position:static">
                  <div class="when">Nov 17, 5:28 PM</div>
                  <div class="row"><span><i class="key" style="--key:var(--chart-1)"></i>Cheapest</span>${coins(50100)}</div>
                  <div class="row"><span>Listed</span>9,324</div>
                </div>
                <div class="tip glass" style="position:static">
                  <div class="title" style="color:#1eff00">Arcanite Bar</div>
                  <div>Max Stack: 20</div><div>Sell Price: ${coins(1000)}</div>
                  <div class="src">Your last scan: 36g 40s each · seen 12 times</div>
                </div>
              </div>
              <p class="note">A hint on hover: <span data-tip="Prices through 2026-11-17" tabindex="0" class="strong">hover me</span></p>`,
          })}
          ${panel({
            title: "Dialog and toasts",
            body: html`
              <div class="ds-row">
                <button class="btn" type="button" data-open-dialog>Open a dialog</button>
              </div>
              <div class="ds-row">
                <button class="btn small" type="button" data-toast="ok" data-msg="Copied. In Auctionator: Shopping, Import, paste.">Toast: ok</button>
                <button class="btn small" type="button" data-toast="ember" data-msg="New price scan: 13 picks to search.">Toast: ember</button>
                <button class="btn small" type="button" data-toast="bad" data-msg="Couldn’t reach fbl-rs live. Retrying…">Toast: bad</button>
              </div>
              <div class="toast ok" style="animation:none">${icon("check")}<span class="msg">A toast, at rest.</span></div>`,
          })}
        </div>
        <dialog class="dialog ds-dialog" aria-labelledby="ds-dialog-title">
          <header class="dialog-head"><span class="dot live"></span><h2 id="ds-dialog-title">New price scan</h2><span>scan finished at 5:28 PM</span>
            <div class="actions"><button class="btn small" type="button" data-close-dialog>${icon("min", "sm")}Minimize</button></div></header>
          <div class="dialog-body">
            ${meter(0.4, { tone: "ember", label: "Demo progress" })}
            <ol class="steps"><li><span>Copy the list.</span></li><li><span>Open each item at the AH.</span></li><li><span>Save prices &amp; reload.</span></li></ol>
            <dl class="facts"><dt>Picks</dt><dd>13</dd><dt>Searched</dt><dd>5</dd></dl>
          </div>
          <footer class="dialog-foot"><button class="btn" type="button" data-close-dialog>Cancel</button><button class="btn primary push" type="button" data-close-dialog>Finish round</button></footer>
        </dialog>`,
    );
  }

  #data() {
    return this.#section(
      "data",
      "Money and charts",
      "coins() draws money the way the game does. Charts are hand-written SVG coloured by the chart tokens; money and quantity axes always start at zero.",
      html`
        <div class="cols even">
          ${panel({
            title: "Money",
            body: html`<div class="facts">
              <dt>coins(68124709)</dt><dd>${coins(68124709)}</dd>
              <dt>coins(12345)</dt><dd>${coins(12345)}</dd>
              <dt>coins(50000)</dt><dd>${coins(50000)}</dd>
              <dt>coins(405)</dt><dd>${coins(405)}</dd>
              <dt>coins(87)</dt><dd>${coins(87)}</dd>
              <dt>signed gain</dt><dd>${signed(coins(31240, { signed: true }), 1)}</dd>
              <dt>signed loss</dt><dd>${signed(coins(-4120), -1)}</dd>
              <dt>coins(null)</dt><dd>${coins(null)}</dd>
            </div>
            <div class="record" style="padding:10px 12px">On parchment: ${coins(7664020)} · <span class="up">${coins(5608000, { signed: true })}</span></div>
            <div class="tile-value">${coins(68124709)}</div>`,
          })}
          ${panel({
            title: "Meters and legends",
            body: html`
              ${demo("meter()", html`${meter(0.62)}${meter(0.35, { tone: "ember" })}${meter(1, { tone: "over" })}`)}
              ${demo("legend()", legend([
                { label: "Lowest buyout", color: "var(--chart-1)" },
                { label: "Median", kind: "dash", color: "var(--chart-2)" },
                { label: "Each scan", kind: "dot", color: "var(--text-muted)" },
                { label: "Day’s range", kind: "band" },
                { label: "To buy", kind: "bar", color: "var(--chart-bar-hot)" },
              ]))}`,
          })}
        </div>
        <div class="cols even">
          ${panel({
            title: "Time chart, on stone",
            meta: "made-up data",
            body: html`${legend([{ label: "Cheapest at each scan", color: "var(--chart-1)" }, { label: "Day’s range", kind: "band" }])}<time-chart kind="price"></time-chart>
              ${legend([{ label: "Listed at each scan", color: "var(--chart-2)" }])}<time-chart kind="listed"></time-chart>`,
          })}
          ${panel({ title: "Wealth chart, on parchment", record: true, meta: "made-up data", body: html`<wealth-chart></wealth-chart>` })}
        </div>`,
    );
  }

  #icons() {
    return this.#section(
      "icons",
      "Icons",
      html`icon(name) in lib/icons.js: the app’s own line set, 16px (or "sm", 14px), in the current text colour. Decorative, so say the meaning in text.`,
      panel({
        title: "The set",
        count: Object.keys(ICONS).length,
        body: html`<div class="ds-grid">${Object.keys(ICONS).map((name) => html`<div class="cluster muted">${icon(name)}<code class="mono">${name}</code></div>`)}</div>`,
      }),
    );
  }
}

define("design-view", DesignView);
