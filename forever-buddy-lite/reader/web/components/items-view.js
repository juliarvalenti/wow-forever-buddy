// The Items page: the item finder as a list on the left, the selected item on the right. On a narrow
// screen they stack, and opening an item (from the list, the worklist or the round dialog) scrolls
// down to it.

import { Component, define } from "../lib/component.js";
import { store } from "../lib/store.js";
import "./item-finder.js";
import "./item-view.js";

const STACKED = "(max-width: 900px)";

class ItemsView extends Component {
  #onOpen = () => {
    if (!window.matchMedia(STACKED).matches) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    requestAnimationFrame(() => this.ref("detail")?.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" }));
  };

  setup() {
    this.innerHTML = `
      <div class="items-layout">
        <aside class="finder-pane" aria-label="Find an item"><item-finder></item-finder></aside>
        <div class="detail-pane" data-ref="detail"><item-view></item-view></div>
      </div>`;
    store.addEventListener("open-item", this.#onOpen);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    store.removeEventListener("open-item", this.#onOpen);
  }
}

define("items-view", ItemsView);
