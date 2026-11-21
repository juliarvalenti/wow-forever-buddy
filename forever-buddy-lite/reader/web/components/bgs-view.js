// The Battlegrounds page: your match history, from the matches the addon records when each one ends
// (Enemies.lua). Live figures on stone (win rate, your averages, honor per hour, by battleground);
// the matches themselves are a record of what happened (parchment). Only your own row and each
// side's totals are kept: no other player's name.

import { Component, define } from "../lib/component.js";
import { html } from "../lib/html.js";
import { count, ago, percent } from "../lib/format.js";
import { fetchBattlegrounds } from "../lib/api.js";
import { pageHead, panel, tiles, tile, table, pill, emptyState, loadingState, errorState } from "../lib/ui.js";

const dash = html`<span class="muted">–</span>`;
const num = (v, digits = 1) => (v == null ? dash : Number(v).toLocaleString(undefined, { maximumFractionDigits: digits }));
const big = (v) => (v == null ? dash : Number(v) >= 1000 ? `${(Number(v) / 1000).toFixed(1)}k` : String(Math.round(v)));
const rate = (r) => (r == null ? dash : `${Math.round(r * 100)}%`);
const minutes = (s) => (s == null ? dash : `${Math.round(s / 60)} min`);
const charName = (key) => (key || "").split("-")[0];

const RESULT = { win: ["Win", "ok"], loss: ["Loss", "bad"], draw: ["Draw", "quiet"], unknown: ["?", "quiet"] };

class BgsView extends Component {
  #data = null;
  #error = null;
  #loadedFor = undefined;
  #char = "";

  watch(s) {
    return [s.lastUpdate];
  }

  setup() {
    this.addEventListener("change", (event) => {
      if (event.target.matches("[data-char]")) {
        this.#char = event.target.value;
        this.#loadedFor = undefined;
        this.#data = null;
        this.refresh(true);
      }
    });
  }

  render() {
    const head = (lede) =>
      pageHead({
        title: "Battlegrounds",
        lede,
        actions: this.#data?.characters?.length > 1
          ? html`<label class="inline-field">Character <select class="small" data-char="1">
              <option value="" ${this.#char === "" ? "selected" : ""}>All</option>
              ${this.#data.characters.map((c) => html`<option value="${c}" ${this.#char === c ? "selected" : ""}>${charName(c)}</option>`)}
            </select></label>`
          : "",
      });
    if (this.#error) return html`${head()}${errorState({ title: "Couldn’t load battlegrounds", text: this.#error })}`;
    if (!this.#data) return html`${head()}${loadingState("Loading matches…")}`;
    const { totals: t, maps, matches } = this.#data;
    if (!t.matches) {
      return html`
        ${head("Every battleground you finish, with your numbers.")}
        ${panel({
          title: "No matches yet",
          record: true,
          body: emptyState({
            icon: "target",
            title: "Play a battleground",
            text: "When a match ends the addon records it: the result, your killing blows, deaths, damage, healing and honor, and each side's totals. Save & Reload (/fbl r) after a match and it shows here.",
          }),
        })}`;
    }
    return html`
      ${head(`${count(t.matches)} matches, ${count(t.minutes)} minutes played.`)}
      ${tiles([
        tile("Win rate", rate(t.winRate), `${count(t.wins)} won, ${count(t.losses)} lost`),
        tile("Killing blows", num(t.kb), "per match"),
        tile("Deaths", num(t.deaths), "per match"),
        tile("Honor per hour", t.honorPerHour == null ? dash : count(t.honorPerHour), `${count(t.honor)} honor in all`),
        tile("Damage", big(t.damage), "per match, where shown"),
        tile("Healing", big(t.healing), "per match, where shown"),
      ])}
      ${panel({
        title: "By battleground",
        count: maps.length,
        flush: table({
          columns: [
            { label: "Battleground" }, { label: "Matches", cls: "num" }, { label: "Win rate", cls: "num" }, { label: "KB", cls: "num" },
            { label: "Deaths", cls: "num" }, { label: "Damage", cls: "num" }, { label: "Honor / hour", cls: "num" },
          ],
          rows: maps,
          row: (m) => html`
            <tr>
              <td>${m.map}</td>
              <td class="num">${count(m.matches)}</td>
              <td class="num">${rate(m.winRate)}</td>
              <td class="num">${num(m.kb)}</td>
              <td class="num">${num(m.deaths)}</td>
              <td class="num">${big(m.damage)}</td>
              <td class="num">${m.honorPerHour == null ? dash : count(m.honorPerHour)}</td>
            </tr>`,
        }),
      })}
      ${panel({
        title: "Matches",
        count: matches.length,
        record: true,
        flush: table({
          sticky: true,
          columns: [
            { label: "When", cls: "num" }, { label: "Battleground" }, { label: "Result" }, { label: "Length", cls: "num" },
            { label: "KB", cls: "num" }, { label: "HK", cls: "num" }, { label: "Deaths", cls: "num" }, { label: "Damage", cls: "num" },
            { label: "Healing", cls: "num" }, { label: "Honor", cls: "num" }, { label: "Objectives", cls: "text" },
          ],
          rows: matches,
          row: (m) => {
            const me = m.me || {};
            const [label, tone] = RESULT[m.result] || RESULT.unknown;
            const objectives = (me.stats || []).filter((s) => s.value).map((s) => `${s.name} ${s.value}`).join(", ");
            return html`
              <tr>
                <td class="num">${ago(m.at)}</td>
                <td>${m.map}${this.#char === "" && m.char ? html`<span class="sub">${charName(m.char)}</span>` : ""}</td>
                <td>${pill(label, tone)}</td>
                <td class="num">${minutes(m.duration)}</td>
                <td class="num">${num(me.kb, 0)}</td>
                <td class="num">${num(me.hk, 0)}</td>
                <td class="num">${num(me.deaths, 0)}</td>
                <td class="num">${big(me.damage)}</td>
                <td class="num">${big(me.healing)}</td>
                <td class="num">${num(me.honor, 0)}</td>
                <td class="text">${objectives || dash}</td>
              </tr>`;
          },
        }),
      })}`;
  }

  async rendered({ lastUpdate }) {
    const key = `${lastUpdate}|${this.#char}`;
    if (this.#loadedFor === key && (this.#data || this.#error)) return;
    this.#loadedFor = key;
    try {
      this.#data = await fetchBattlegrounds(this.#char);
      this.#error = null;
    } catch (error) {
      this.#error = error.message;
    }
    this.refresh(true);
  }
}

define("bgs-view", BgsView);
