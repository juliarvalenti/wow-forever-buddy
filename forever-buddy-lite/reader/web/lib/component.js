// Base class for the app's components. Light DOM, so style.css applies everywhere.
//
// A subclass defines:
//   watch(state)   -> array of the state pieces it shows; it re-renders only when one changes
//   render(state)  -> html`...`, replacing its contents; leave it out to manage the DOM yourself
//   setup()        optional, once when attached (event listeners on `this`, a fixed skeleton)
//   rendered()     optional, after each render (e.g. hand data to child widgets)

import { store } from "./store.js";
import { html } from "./html.js";

export class Component extends HTMLElement {
  #unsubscribe = null;
  #lastWatched = null;

  connectedCallback() {
    this.setup?.();
    this.#unsubscribe = store.subscribe(() => this.refresh());
    this.refresh(true);
  }

  disconnectedCallback() {
    this.#unsubscribe?.();
    this.#lastWatched = null;
  }

  refresh(force = false) {
    const watched = this.watch ? this.watch(store.state) : null;
    if (!force && watched && sameValues(watched, this.#lastWatched)) return;
    this.#lastWatched = watched;
    // Through html``, so a render() that returns an array or a plain string still renders right.
    if (this.render) this.innerHTML = String(html`${this.render(store.state)}`);
    this.rendered?.(store.state);
  }

  // The element inside marked data-ref="name".
  ref(name) {
    return this.querySelector(`[data-ref="${name}"]`);
  }
}

function sameValues(a, b) {
  return b != null && a.length === b.length && a.every((value, i) => value === b[i]);
}

// Shorthand: define a custom element once (modules can be imported more than once).
export function define(name, cls) {
  if (!customElements.get(name)) customElements.define(name, cls);
}
