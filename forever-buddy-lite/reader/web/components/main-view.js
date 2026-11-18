// Shows the page the address names (the header's navigation, or the style guide at #/design).
// Switching pages starts at the top.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";

class MainView extends Component {
  watch(s) {
    return [s.prefs.view];
  }

  render({ prefs }) {
    if (prefs.view === "items") return html`<items-view></items-view>`;
    if (prefs.view === "wealth") return html`<wealth-view></wealth-view>`;
    if (prefs.view === "snipes") return html`<snipes-view></snipes-view>`;
    if (prefs.view === "trades") return html`<trades-view></trades-view>`;
    if (prefs.view === "design") return html`<design-view></design-view>`;
    return html`<opportunities-view></opportunities-view>`;
  }

  rendered() {
    window.scrollTo(0, 0);
  }
}

define("main-view", MainView);
