// Shows the opportunities or the selected item, as the tabs say.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";

class MainView extends Component {
  watch(s) {
    return [s.prefs.view];
  }

  render({ prefs }) {
    if (prefs.view === "item") return html`<item-view></item-view>`;
    if (prefs.view === "wealth") return html`<wealth-view></wealth-view>`;
    return html`<opportunities-view></opportunities-view>`;
  }
}

define("main-view", MainView);
