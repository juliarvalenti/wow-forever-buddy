// Tabs with the keyboard behaviour they need. Write the markup, this wires it up:
//
//   <fb-tabs>
//     <div class="tabs" role="tablist" aria-label="Views">
//       <button role="tab" aria-controls="t-a" aria-selected="true">One</button>
//       <button role="tab" aria-controls="t-b">Two</button>
//     </div>
//     <div role="tabpanel" id="t-a">...</div>
//     <div role="tabpanel" id="t-b" hidden>...</div>
//   </fb-tabs>
//
// Click or Left/Right/Home/End moves between tabs; only the selected one is in the Tab order. It
// fires "tab-change" with { detail: panelId }. Works on markup re-rendered by a parent, too.

class FbTabs extends HTMLElement {
  connectedCallback() {
    this.addEventListener("click", (event) => {
      const tab = event.target.closest('[role="tab"]');
      if (tab && this.contains(tab)) this.select(tab);
    });
    this.addEventListener("keydown", (event) => {
      const tab = event.target.closest('[role="tab"]');
      if (!tab) return;
      const tabs = this.#tabs();
      const i = tabs.indexOf(tab);
      const next = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1 }[event.key];
      if (next == null) return;
      event.preventDefault();
      const target = tabs[(next + tabs.length) % tabs.length];
      this.select(target);
      target.focus();
    });
    // Sync once the children are parsed.
    queueMicrotask(() => this.select(this.#tabs().find((t) => t.getAttribute("aria-selected") === "true") || this.#tabs()[0], { quiet: true }));
  }

  #tabs() {
    return [...this.querySelectorAll('[role="tab"]')].filter((t) => t.closest("fb-tabs") === this);
  }

  select(tab, { quiet = false } = {}) {
    if (!tab) return;
    for (const t of this.#tabs()) {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      const panel = document.getElementById(t.getAttribute("aria-controls"));
      if (panel) {
        panel.hidden = !on;
        panel.tabIndex = 0;
      }
    }
    if (!quiet) this.dispatchEvent(new CustomEvent("tab-change", { detail: tab.getAttribute("aria-controls"), bubbles: true }));
  }
}

if (!customElements.get("fb-tabs")) customElements.define("fb-tabs", FbTabs);
