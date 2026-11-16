// A tagged template for building markup safely.
//
//   html`<td>${item.name}</td>`        the name is escaped: data can never inject markup
//   html`<ul>${items.map(row)}</ul>`   arrays and nested html`` results are inserted as markup
//   raw(trustedString)                 opt out, for markup we built ourselves (e.g. an SVG path)
//
// null, undefined and false render as nothing, so `${cond && html`...`}` works.

class SafeHtml {
  constructor(text) {
    this.text = text;
  }
  toString() {
    return this.text;
  }
}

const ENTITIES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

export function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (c) => ENTITIES[c]);
}

export function raw(text) {
  return new SafeHtml(String(text));
}

function toMarkup(value) {
  if (value == null || value === false) return "";
  if (value instanceof SafeHtml) return value.text;
  if (Array.isArray(value)) return value.map(toMarkup).join("");
  return escapeHtml(value);
}

export function html(strings, ...values) {
  let out = strings[0];
  values.forEach((value, i) => {
    out += toMarkup(value) + strings[i + 1];
  });
  return new SafeHtml(out);
}
