// Shows the page the header's navigation points at. Switching pages starts at the top.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";

class MainView extends Component {
  watch(s) {
    return [s.prefs.view];
  }

  render({ prefs }) {
    if (prefs.view === "items") return html`<items-view></items-view>`;
    if (prefs.view === "wealth") return html`<wealth-view></wealth-view>`;
    return html`<opportunities-view></opportunities-view>`;
  }

  rendered() {
    window.scrollTo(0, 0);
  }
}

define("main-view", MainView);
