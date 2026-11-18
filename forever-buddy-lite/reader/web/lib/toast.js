// Short news in the corner: toast("Copied 12 items"), toast("Couldn't save", { tone: "bad" }).
// Tones: "ok" (default), "ember" (live, pending), "bad", "plain". It fades after a few seconds;
// screen readers hear it through the region's aria-live.

import { html } from "./html.js";
import { icon } from "./icons.js";

const ICON = { ok: "check", ember: "clock", bad: "alert", plain: "spark" };

function region() {
  let box = document.querySelector(".toasts");
  if (!box) {
    box = document.createElement("div");
    box.className = "toasts";
    box.setAttribute("role", "status");
    box.setAttribute("aria-live", "polite");
    document.body.append(box);
  }
  return box;
}

export function toast(message, { tone = "ok", ms = 4000 } = {}) {
  const item = document.createElement("div");
  item.className = `toast ${tone}`;
  item.innerHTML = String(html`${icon(ICON[tone] || ICON.plain)}<span class="msg">${message}</span>`);
  region().append(item);
  setTimeout(() => {
    item.classList.add("leaving");
    setTimeout(() => item.remove(), 250);
  }, ms);
  return item;
}
