// Markup builders for the design system's pieces (styles/components.css). Each returns html``, so
// everything interpolated is escaped; pass html`` (or coins()) where a slot takes markup.
//
//   pageHead({ title, lede, actions })                    the page's title block
//   panel({ title, count, meta, actions, body, flush, foot, tone, record, open, id })
//   tiles([tile(label, value, note), ...])                a row of stat tiles
//   table({ columns, rows, row, sticky, compact, empty })  a table in its scroll box
//   pill(text, tone)   seg(name, options, current)   chips(...)   legend([...])
//   emptyState / loadingState / errorState                 in place of content
//   callout(text, { tone, icon })   itemIcon(name)   meter(fraction)
//
// See DESIGN.md for how a page puts them together.

import { html, raw } from "./html.js";
import { icon } from "./icons.js";

export { icon };

// The title block at the top of a page. actions: html`` buttons, right-aligned.
export function pageHead({ title, lede, actions, id }) {
  return html`
    <header class="page-head">
      <div class="titles">
        <h1 ${id ? html`id="${id}"` : ""}>${title}</h1>
        ${lede && html`<p class="lede">${lede}</p>`}
      </div>
      ${actions && html`<div class="page-actions">${actions}</div>`}
    </header>`;
}

// A heading between groups of panels.
export function sectionHead({ title, note, id, actions }) {
  return html`
    <header class="section-head">
      <h2 ${id ? html`id="${id}"` : ""}>${title}</h2>
      ${note && html`<p class="section-note">${note}</p>`}
      ${actions && html`<div class="cluster">${actions}</div>`}
    </header>`;
}

// A stone panel with a leather strap for its head.
//   body    padded content          flush   edge-to-edge content (a table, a list)
//   tone    "act" (the one that wants you) | "quiet" (reference)
//   record  true: parchment, for things that happened
//   open    true/false: a <details> panel that folds (the strap is its summary)
//   level   the heading level (default 2)
export function panel({ title, count, note, meta, actions, body, flush, foot, tone, record, open, id, level = 2, cls = "" }) {
  // The tag name is ours (a number we control); the id is escaped like any attribute.
  const h = `h${Math.min(6, Math.max(2, Number(level) || 2))}`;
  const heading = html`${raw(`<${h}`)} ${id ? html`id="${id}"` : ""}>${title}${raw(`</${h}>`)}`;
  const head = html`
    ${heading}
    ${count != null && count !== "" && html`<span class="count">${count}</span>`}
    ${note && html`<span class="note">${note}</span>`}
    ${meta && html`<span class="meta">${meta}</span>`}
    ${actions && html`<div class="actions">${actions}</div>`}`;
  const classes = ["panel", tone, record && "record", cls].filter(Boolean).join(" ");
  const inner = html`
    ${body != null && body !== "" && html`<div class="panel-body">${body}</div>`}
    ${flush != null && flush !== "" && html`<div class="panel-flush">${flush}</div>`}
    ${foot && html`<footer class="panel-foot">${foot}</footer>`}`;
  if (open != null) {
    return html`<details class="${classes}" ${open ? "open" : ""}><summary class="panel-head">${head}</summary>${inner}</details>`;
  }
  return html`<section class="${classes}" ${id ? html`aria-labelledby="${id}"` : ""}><header class="panel-head">${head}</header>${inner}</section>`;
}

export function tiles(list) {
  return html`<div class="tiles">${list}</div>`;
}

export function tile(label, value, note) {
  return html`<div class="tile"><div class="tile-label">${label}</div><div class="tile-value">${value}</div>${note && html`<div class="tile-note">${note}</div>`}</div>`;
}

// A table in its scroll box.
//   columns  [{ label, cls }]           cls "num" right-aligns a figure column, "text" wraps
//   rows     the data                   row(r) -> html`<tr>...</tr>`
//   sticky   heads stay put while a long table scrolls in its own box
export function table({ columns, rows, row, sticky = false, compact = false, cls = "", empty = "Nothing here." }) {
  if (!rows.length) return emptyState({ text: empty, compact: true });
  return html`
    <div class="table-scroll ${sticky ? "capped" : ""}">
      <table class="table ${sticky ? "sticky" : ""} ${compact ? "compact" : ""} ${cls}">
        <thead><tr>${columns.map((c) => html`<th class="${c.cls || ""}" scope="col">${c.label}</th>`)}</tr></thead>
        <tbody>${rows.map(row)}</tbody>
      </table>
    </div>`;
}

// tone: "ok" | "warn" | "bad" | "info" | "bronze" | "violet" | "quiet"
export function pill(text, tone = "") {
  return html`<span class="pill ${tone}">${text}</span>`;
}

// A segmented control: options [[value, label]], data-<name>="value" on each button.
export function seg(name, options, current, label) {
  return html`
    <div class="seg" role="group" aria-label="${label || name}">
      ${options.map(([value, text]) => html`<button type="button" data-${name}="${value}" aria-pressed="${String(value) === String(current)}">${text ?? value}</button>`)}
    </div>`;
}

// Legend keys: [{ label, kind: "line" | "dash" | "dot" | "band" | "bar", color: "var(--chart-1)" }]
export function legend(keys) {
  return html`
    <div class="legend">
      ${keys.map((k) => html`<span><i class="key ${k.kind === "line" ? "" : k.kind || ""}" style="--key:${k.color || "var(--chart-1)"}"></i>${k.label}</span>`)}
    </div>`;
}

export function callout(body, { tone = "", icon: name } = {}) {
  return html`<div class="callout ${tone}">${name && icon(name)}<span>${body}</span></div>`;
}

export function emptyState({ title, text, actions, icon: name = "search", compact = false } = {}) {
  return html`
    <div class="state ${compact ? "compact" : ""}">
      ${!compact && icon(name)}
      ${title && html`<div class="state-title">${title}</div>`}
      ${text && html`<p>${text}</p>`}
      ${actions && html`<div class="actions">${actions}</div>`}
    </div>`;
}

export function loadingState(text = "Loading…", { compact = false } = {}) {
  return html`<div class="state loading ${compact ? "compact" : ""}" role="status"><span class="spinner" aria-hidden="true"></span><p>${text}</p></div>`;
}

export function errorState({ title = "Something went wrong", text, actions, compact = false } = {}) {
  return html`
    <div class="state error ${compact ? "compact" : ""}" role="alert">
      ${!compact && icon("alert")}
      <div class="state-title">${title}</div>
      ${text && html`<p>${text}</p>`}
      ${actions && html`<div class="actions">${actions}</div>`}
    </div>`;
}

// An item's letter tile: the first letter of its name ("10 Pound Mud Snapper" -> P), else its first digit.
export function itemIcon(name, size = "") {
  const letter = (String(name).match(/[A-Za-z]/) || String(name).match(/[0-9]/) || ["?"])[0].toUpperCase();
  return html`<span class="item-icon ${size}" aria-hidden="true">${letter}</span>`;
}

// A character in their class colour: character("Thrandor", "PALADIN").
const CLASSES = ["warrior", "paladin", "hunter", "rogue", "priest", "shaman", "mage", "warlock", "druid"];
export function character(name, classToken) {
  const cls = String(classToken || "").toLowerCase();
  const style = CLASSES.includes(cls) ? `--cc:var(--class-${cls})` : "";
  return html`<span class="who" style="${style}"><i class="cdot" aria-hidden="true"></i><span class="cc">${name}</span></span>`;
}

export function meter(fraction, { tone = "", label } = {}) {
  const pct = Math.max(0, Math.min(1, fraction || 0)) * 100;
  return html`<div class="meter ${tone}" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.round(pct)}" ${label ? html`aria-label="${label}"` : ""}><i style="width:${pct}%"></i></div>`;
}

// A gain or loss in colour, with its sign: signed(coins(x, { signed: true }), x)
export function signed(markup, value) {
  return html`<span class="${value > 0 ? "up" : value < 0 ? "down" : "muted"}">${markup}</span>`;
}
