// Shows the opportunities or the selected item, as the tabs say.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";

class MainView extends Component {
  watch(s) {
    return [s.prefs.view];
  }

  render({ prefs }) {
    return prefs.view === "item" ? html`<item-view></item-view>` : html`<opportunities-view></opportunities-view>`;
  }
}

define("main-view", MainView);
