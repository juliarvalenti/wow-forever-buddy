// A local price dashboard from Auctionator's scans: item finder with facets on the left; on the right the
// opportunities (from market.mjs) or one item's numbers, price chart, listings chart and table.
//   node dashboard.mjs [--out file.html] [--open]      one self-contained HTML file
//   node live.mjs                                       the same page served live, updating after each scan
// Names come from QuestieDB (if installed), the gear AH scan and your characters' items; Auctionator
// stores ids only.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { decodeCbor, loadPrices, load, call } from "./fbl.mjs";
import { analyze } from "./market.mjs";

const TRADE = { 1: "Parts", 2: "Explosives", 3: "Devices", 5: "Cloth", 6: "Leather", 7: "Metal & stone", 8: "Meat & fish", 9: "Herbs", 10: "Elemental", 11: "Other trade goods", 12: "Enchanting" };
const CLASS = { 0: "Consumables", 1: "Bags", 2: "Weapons", 4: "Armor", 5: "Reagents", 6: "Ammo", 9: "Recipes", 11: "Quivers", 12: "Quest items", 15: "Miscellaneous" };
// Classic-era data files trade goods mostly under one subclass, so names decide the finer kinds.
const BY_NAME = [[/^(Linen|Wool|Silk|Mageweave|Runecloth|Felcloth|Mooncloth) Cloth$|^Bolt of |^(Runecloth|Felcloth|Mooncloth|Shadow Silk|Spider's Silk|Thick Spider's Silk|Ironweb Spider Silk)$/, "Cloth"],
  [/Leather$|Hide$|Scale$|Scales$/, "Leather"], [/Ore$|Bar$|Stone$|^Coal$|^Elementium/, "Metal & stone"],
  [/^Elemental |^Essence of (Air|Earth|Fire|Water|Undeath)$|^(Heart|Breath|Core|Globe) of /, "Elemental"],
  [/Dust$|Essence$|Shard$|^Nexus Crystal$/, "Enchanting"], [/Meat$|Flesh$|Wing$|Egg$|Leg$|Tail$|Clam Meat$|Haunch|Chunk of|Fillet|Snapper|Mackerel|Squid$|Eel$|Sagefish/, "Meat & fish"],
  [/Lotus$|bloom$|thistle$|^(Sungrass|Dreamfoil|Gromsblood|Icecap|Kingsblood|Liferoot|Fadeleaf|Goldthorn|Wintersbite|Firebloom|Sorrowmoss|Blindweed|Stranglekelp|Bruiseweed|Silverleaf|Briarthorn|Mageroyal|Earthroot|Wild Steelbloom|Grave Moss|Khadgar's Whisker|Arthas' Tears|Ghost Mushroom|Mountain Silversage|Plaguebloom|Golden Sansam|Black Lotus|Swiftthistle)$/, "Herbs"],
  [/Potion$|Elixir|Flask of|Draught/, "Potions & elixirs"], [/^(Pattern|Recipe|Schematic|Formula|Plans|Manual|Design): /, "Recipes"]];
const kind = (i) => { for (const [re, k] of BY_NAME) if (re.test(i?.name || "")) return k;
  return i?.cls === 7 ? TRADE[i.sub] || "Other trade goods" : CLASS[i?.cls] || "Other"; };


// QuestieDB's name, class and subclass for most items, read once.
let questie;
function questieInfo() {
  if (questie) return questie;
  questie = new Map();
  const toc = path.join(process.env.QUESTIEDB_DIR || "C:/Program Files (x86)/World of Warcraft/_forever_/Interface/AddOns/QuestieDB", "QuestieDB_Forever.toc");
  if (fs.existsSync(toc)) for (const l of fs.readFileSync(toc, "utf8").split(/\r?\n/)) {
    const m = /^## X-Item-(\d+)-S: (.*)$/.exec(l); if (!m) continue;
    const d = decodeCbor(Buffer.from(m[2], "base64"));
    const t = (v) => (Buffer.isBuffer(v) ? v.toString("utf8") : v);
    questie.set(m[1], { name: t(d[1]), cls: d[12], sub: d[13] });
  }
  return questie;
}

// Every named item with its daily lows, highs and quantities: { items, stamp }.
export function buildItems() {
const info = new Map(questieInfo());
const { chars, ah } = load();
for (const it of Object.values(ah?.items || {})) if (it.id && !info.has(String(it.id))) info.set(String(it.id), { name: it.name, cls: it.class, sub: it.subclass });
for (const c of Object.keys(chars)) for (const r of call("items", { character: c }).items)
  if (r.id && !info.get(String(r.id))?.name) info.set(String(r.id), { ...(info.get(String(r.id)) || {}), name: r.name });
const DAY0 = Date.UTC(2020, 0, 1);
const items = [];
let scanned = 0;
for (const data of Object.values(loadPrices().realms)) for (const [id, e] of Object.entries(data)) {
  if (id === "version" || !e?.h) continue;
  const i = info.get(id);
  if (!i?.name) continue;
  const days = Object.keys(e.h).map(Number).sort((a, b) => a - b);
  scanned = Math.max(scanned, days.at(-1));
  items.push({ id: +id, n: i.name, k: kind(i), now: e.m,
    h: days.map((d) => [new Date(DAY0 + d * 864e5).toISOString().slice(5, 10), e.l?.[d] ?? e.h[d], e.h[d], e.a?.[d] ?? 0]) });
}
items.sort((a, b) => a.n.localeCompare(b.n));
const stamp = new Date(DAY0 + scanned * 864e5).toISOString().slice(0, 10);
return { items, stamp };
}

// The page. live: it fetches /data.json and follows /events (live.mjs); otherwise the data is embedded.
export function page(items, stamp, { opps = null, live = false } = {}) {
return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Auction House Prices</title>
<style>
:root{color-scheme:light;--surface-0:#f4f4f2;--surface-1:#fcfcfb;--line:#e1e0db;--grid:#ecebe7;--text-primary:#0b0b0b;--text-secondary:#52514e;--text-muted:#7a7974;--series-1:#2a78d6;--band:#2a78d622;--accent-soft:#e6efff;--up:#008300;--down:#c4352f;--font:system-ui,-apple-system,"Segoe UI",sans-serif;--mono:ui-monospace,Consolas,monospace}
@media (prefers-color-scheme:dark){:root:where(:not([data-theme="light"])){color-scheme:dark;--surface-0:#121211;--surface-1:#1a1a19;--line:#2e2e2b;--grid:#252523;--text-primary:#f0efec;--text-secondary:#c3c2b7;--text-muted:#8f8e86;--series-1:#3987e5;--band:#3987e530;--accent-soft:#1d2a3d;--up:#3fb950;--down:#f0716b}}
:root[data-theme="dark"]{color-scheme:dark;--surface-0:#121211;--surface-1:#1a1a19;--line:#2e2e2b;--grid:#252523;--text-primary:#f0efec;--text-secondary:#c3c2b7;--text-muted:#8f8e86;--series-1:#3987e5;--band:#3987e530;--accent-soft:#1d2a3d;--up:#3fb950;--down:#f0716b}
*{box-sizing:border-box}body{margin:0;background:var(--surface-0);color:var(--text-primary);font:14px/1.45 var(--font)}
.app{display:grid;grid-template-columns:340px 1fr;height:100vh}
aside{border-right:1px solid var(--line);background:var(--surface-1);display:grid;grid-template-rows:auto auto 1fr;min-height:0}
.head{padding:16px 16px 8px}.head h1{font-size:17px;margin:0}.head .sub{color:var(--text-muted);font-size:12px}
.filters{padding:8px 16px 12px;display:grid;gap:10px;border-bottom:1px solid var(--line)}
input[type=search],input[type=number],select{font:inherit;width:100%;padding:6px 8px;border:1px solid var(--line);border-radius:6px;background:var(--surface-0);color:var(--text-primary)}
.row2{display:grid;grid-template-columns:1fr 1fr;gap:8px}
details summary{cursor:pointer;color:var(--text-secondary);font-size:12px;text-transform:uppercase;letter-spacing:.06em}
.facet{display:flex;flex-wrap:wrap;gap:6px;margin-top:6px}
.chip{font:inherit;font-size:12px;padding:3px 9px;border-radius:99px;border:1px solid var(--line);background:var(--surface-0);color:var(--text-primary);cursor:pointer}
.chip[aria-pressed=true]{background:var(--text-primary);color:var(--surface-1);border-color:var(--text-primary)}
.chip .c{color:var(--text-muted);margin-left:4px}.chip[aria-pressed=true] .c{color:inherit;opacity:.7}
label.l{font-size:12px;color:var(--text-secondary);display:grid;gap:3px}
.list{overflow:auto;min-height:0}
.item{display:grid;grid-template-columns:1fr auto;gap:2px 8px;padding:8px 16px;border-bottom:1px solid var(--grid);cursor:pointer}
.item:hover{background:var(--surface-0)}.item[aria-selected=true]{background:var(--accent-soft)}
.item .nm{font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.item .pr{font-family:var(--mono);font-variant-numeric:tabular-nums;text-align:right}
.item .k{color:var(--text-muted);font-size:12px}.item .ch{font-size:12px;text-align:right;font-variant-numeric:tabular-nums}
.up{color:var(--up)}.down{color:var(--down)}.muted{color:var(--text-muted)}
main{overflow:auto;padding:24px;display:grid;gap:20px;align-content:start}
main h2{margin:0;font-size:22px}main .kind{color:var(--text-muted)}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:12px}
.tile{background:var(--surface-1);border:1px solid var(--line);border-radius:10px;padding:12px 14px}
.tile .lab{font-size:12px;color:var(--text-secondary)}.tile .val{font-size:20px;font-weight:600;font-variant-numeric:tabular-nums}.tile .note{font-size:12px;color:var(--text-muted)}
.card{background:var(--surface-1);border:1px solid var(--line);border-radius:10px;padding:14px 16px;display:grid;gap:8px;position:relative}
.card h3{margin:0;font-size:14px}.card .sub{font-size:12px;color:var(--text-muted)}
svg{display:block;width:100%;height:auto;overflow:visible}
svg text{fill:var(--text-muted);font:11px var(--font)}
.tip{position:absolute;pointer-events:none;background:var(--surface-1);border:1px solid var(--line);border-radius:6px;padding:6px 9px;font-size:12px;box-shadow:0 2px 8px #0002;white-space:nowrap;display:none}
table{border-collapse:collapse;width:100%;font-size:13px}th,td{padding:6px 10px;border-bottom:1px solid var(--grid);text-align:right;font-variant-numeric:tabular-nums}th:first-child,td:first-child{text-align:left}th{color:var(--text-secondary);font-weight:500;font-size:12px}
.empty{color:var(--text-muted);padding:40px 0}
.tabs{display:flex;gap:6px;align-items:center;flex-wrap:wrap}.tabs .live{margin-left:auto;font-size:12px;color:var(--text-muted)}
.tabs .live b{display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--up);margin-right:6px;vertical-align:middle}
.opp h3{display:flex;gap:8px;align-items:baseline}.opp h3 .n{color:var(--text-muted);font-weight:400;font-size:12px}
.opp .why{font-size:12px;color:var(--text-muted);margin-top:-4px}
.opp tr[data-id]{cursor:pointer}.opp tr[data-id]:hover td{background:var(--surface-0)}
.flash{animation:flash 1.2s ease-out}@keyframes flash{from{background:var(--accent-soft)}to{background:transparent}}
@media (max-width:760px){.app{grid-template-columns:1fr;height:auto}aside{max-height:60vh}}
</style></head><body>
<div class="app">
<aside>
  <div class="head"><h1>Auction House Prices</h1><div class="sub">Auctionator scans through ${stamp} · ${items.length.toLocaleString()} named items</div></div>
  <div class="filters">
    <input type="search" id="q" placeholder="Find an item…" autocomplete="off">
    <details open><summary>Category</summary><div class="facet" id="kinds"></div></details>
    <details open><summary>Trend over the scans</summary><div class="facet" id="trends"></div></details>
    <div class="row2">
      <label class="l">Price from (silver)<input type="number" id="pmin" min="0" placeholder="any"></label>
      <label class="l">to (silver)<input type="number" id="pmax" min="0" placeholder="any"></label>
    </div>
    <div class="row2">
      <label class="l">Listed at least<input type="number" id="lmin" min="0" value="0"></label>
      <label class="l">Sort<select id="sort"><option value="n">Name</option><option value="vol">Most listed</option><option value="p">Price, high first</option><option value="up">Rising most</option><option value="down">Falling most</option></select></label>
    </div>
    <div class="sub muted" id="count"></div>
  </div>
  <div class="list" id="list" role="listbox" aria-label="Items"></div>
</aside>
<main id="main"><div class="empty">Pick an item on the left.</div></main>
</div>
<script>
let ITEMS=${JSON.stringify(items)};
let OPPS=${JSON.stringify(opps)};
let STAMP=${JSON.stringify(stamp)};
const LIVE=${live ? "true" : "false"};
const esc=(s)=>String(s).replace(/[&<>"]/g,(c)=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
const money=(c)=>{if(c==null)return"–";c=Math.round(c);const g=Math.floor(c/1e4),s=Math.floor(c%1e4/100),k=c%100;return g?g+"g "+s+"s":s?s+"s "+k+"c":k+"c"};
const median=(a)=>{const s=[...a].sort((x,y)=>x-y);return s[Math.floor((s.length-1)/2)]};
// Usual = median of each day's higher cheapest price, today left out (as market.mjs): a stray cheap listing
// sets a day's low, not its high.
function prep(list){for(const it of list){const lows=it.h.map(d=>d[1]),highs=it.h.map(d=>d[2]);it.usual=median(highs.length>1?highs.slice(0,-1):highs);it.first=lows[0];it.last=lows.at(-1);it.listed=it.h.at(-1)[3];
  it.chg=it.h.length>1&&it.first?(it.last-it.first)/it.first:0;it.trend=it.h.length<2?"New":it.chg>0.1?"Rising":it.chg<-0.1?"Falling":"Flat";
  const floor=Math.min(it.usual,it.now||it.usual);it.snipe=Math.floor(Math.min(floor*0.8,floor*0.95-100-200))}return list}
prep(ITEMS);
const store={get(k,d){try{const v=localStorage.getItem(k);return v==null?d:JSON.parse(v)}catch(e){return d}},set(k,v){try{localStorage.setItem(k,JSON.stringify(v))}catch(e){}}};
const st=Object.assign({q:"",kinds:[],trends:[],pmin:"",pmax:"",lmin:0,sort:"n",sel:null,view:"opps"},store.get("fbl-dash",{}));
const $=(id)=>document.getElementById(id);
function chips(el,values,key){const counts={};for(const it of ITEMS)counts[it[key==="kinds"?"k":"trend"]]=(counts[it[key==="kinds"?"k":"trend"]]||0)+1;
  el.innerHTML=values.filter(v=>counts[v]).map(v=>'<button class="chip" type="button" data-v="'+esc(v)+'" aria-pressed="'+st[key].includes(v)+'">'+esc(v)+'<span class="c">'+counts[v]+'</span></button>').join("");
  el.onclick=(e)=>{const b=e.target.closest(".chip");if(!b)return;const v=b.dataset.v;st[key]=st[key].includes(v)?st[key].filter(x=>x!==v):[...st[key],v];b.setAttribute("aria-pressed",st[key].includes(v));render()}}
function facets(){const KINDS=[...new Set(ITEMS.map(i=>i.k))].sort((a,b)=>ITEMS.filter(i=>i.k===b).length-ITEMS.filter(i=>i.k===a).length);
  chips($("kinds"),KINDS,"kinds");chips($("trends"),["Rising","Flat","Falling","New"],"trends")}
facets();
for(const id of ["q","pmin","pmax","lmin","sort"]){$(id).value=st[id]??"";$(id).addEventListener("input",()=>{st[id]=$(id).value;render()})}
function filtered(){const q=st.q.trim().toLowerCase(),pmin=st.pmin===""?null:+st.pmin*100,pmax=st.pmax===""?null:+st.pmax*100,lmin=+st.lmin||0;
  let r=ITEMS.filter(it=>(!q||it.n.toLowerCase().includes(q))&&(!st.kinds.length||st.kinds.includes(it.k))&&(!st.trends.length||st.trends.includes(it.trend))
    &&(pmin==null||it.now>=pmin)&&(pmax==null||it.now<=pmax)&&it.listed>=lmin);
  const s=st.sort;r.sort(s==="vol"?(a,b)=>b.listed-a.listed:s==="p"?(a,b)=>b.now-a.now:s==="up"?(a,b)=>b.chg-a.chg:s==="down"?(a,b)=>a.chg-b.chg:(a,b)=>a.n.localeCompare(b.n));return r}
const pct=(x)=>(x>0?"+":"")+Math.round(x*100)+"%";
function render(){store.set("fbl-dash",st);const r=filtered();$("count").textContent=r.length.toLocaleString()+" items"+(r.length>400?" · showing 400":"");
  $("list").innerHTML=r.slice(0,400).map(it=>'<div class="item" role="option" data-id="'+it.id+'" aria-selected="'+(it.id===st.sel)+'"><div class="nm">'+esc(it.n)+'</div><div class="pr">'+money(it.now)+'</div><div class="k">'+esc(it.k)+' · '+it.listed.toLocaleString()+' listed</div><div class="ch '+(it.chg>0.1?"up":it.chg<-0.1?"down":"muted")+'">'+(it.h.length>1?pct(it.chg):"new")+'</div></div>').join("");
  if(st.view==="opps")showOpps();else show(ITEMS.find(i=>i.id===st.sel))}
$("list").onclick=(e)=>{const el=e.target.closest(".item");if(!el)return;st.sel=+el.dataset.id;st.view="item";render()};
// Tabs over the right side: the opportunities, or the item picked on the left.
let lastUpdate=null;
function tabs(){const it=ITEMS.find(i=>i.id===st.sel);
  return '<div class="tabs"><button class="chip" type="button" data-view="opps" aria-pressed="'+(st.view==="opps")+'">Opportunities</button>'
   +(it?'<button class="chip" type="button" data-view="item" aria-pressed="'+(st.view==="item")+'">'+esc(it.n)+'</button>':"")
   +'<span class="live">'+(LIVE?'<b></b>live · ':"")+'scans through '+esc(STAMP)+(lastUpdate?' · updated '+lastUpdate:"")+'</span></div>'}
$("main").addEventListener("click",(e)=>{const t=e.target.closest("[data-view]");if(t){st.view=t.dataset.view;render();return}
  const r=e.target.closest("tr[data-id]");if(r){st.sel=+r.dataset.id;st.view="item";render()}});
// The sections market.mjs finds, one card each; a row opens the item.
const SECTIONS=[
 ["buy","Buy","A stray cheap listing on a busy market: under usual and under today\u2019s own high. Profit is one item resold near usual, after the 5% cut and a 1s deposit.",[["Now",r=>money(r.now)],["Usual",r=>money(r.usual)],["vs usual",r=>pct(r.now/r.usual-1)],["Profit each",r=>money(r.profit)],["Listed",r=>r.qty.toLocaleString()]]],
 ["dips","Dip bets","The whole market is down today. Not a sure snipe: a bet that it comes back to usual.",[["Now",r=>money(r.now)],["Usual",r=>money(r.usual)],["vs usual",r=>pct(r.now/r.usual-1)],["If it recovers",r=>money(r.profit)],["Listed",r=>r.qty.toLocaleString()]]],
 ["sell","Sell what you hold","Things on your characters (bags, bank, mail) that sell above their usual today.",[["You have",r=>r.held],["Now",r=>money(r.now)],["Usual",r=>money(r.usual)],["vs usual",r=>pct(r.now/r.usual-1)],["After cut",r=>money(r.worth)]]],
 ["squeeze","Squeeze","Supply falling while the price rises: sellers can ask more.",[["Price",r=>pct(r.dp)],["Supply",r=>pct(r.dq)],["Now",r=>money(r.now)],["You have",r=>r.held||""]]],
 ["flood","Flood","Supply rising while the price falls: do not hold these; buy later.",[["Price",r=>pct(r.dp)],["Supply",r=>pct(r.dq)],["Now",r=>money(r.now)],["You have",r=>r.held||""]]],
 ["swingy","Swingy","The widest range today on busy markets: buy near the low, list near the high.",[["Today low",r=>money(r.low)],["Today high",r=>money(r.high)],["Now",r=>money(r.now)],["Listed",r=>r.qty.toLocaleString()]]],
];
function showOpps(){if(!OPPS){$("main").innerHTML=tabs()+'<div class="empty">No opportunities in this file: run node live.mjs, or rebuild with dashboard.mjs.</div>';return}
  $("main").innerHTML=tabs()+'<div class="sub muted">'+OPPS.busy.toLocaleString()+' busy items ('+OPPS.minListed+'+ listed) of '+OPPS.items.toLocaleString()+'; buys up to '+money(OPPS.budget)+'.</div>'
   +SECTIONS.map(([key,title,why,cols])=>{const rows=(OPPS[key]||[]).slice(0,15);
     return '<div class="card opp"><h3>'+title+'<span class="n">'+(OPPS[key]||[]).length+'</span></h3><div class="why">'+why+'</div>'
      +(rows.length?'<table><thead><tr><th>Item</th>'+cols.map(c=>'<th>'+c[0]+'</th>').join("")+'</tr></thead><tbody>'
        +rows.map(r=>'<tr data-id="'+r.id+'"><td>'+esc(r.name)+'</td>'+cols.map(c=>'<td>'+c[1](r)+'</td>').join("")+'</tr>').join("")+'</tbody></table>'
        :'<div class="muted">Nothing right now.</div>')+'</div>'}).join("")}
// Live: the server says when a scan lands; fetch the new data and redraw, keeping filters and selection.
if(LIVE&&window.EventSource){const ev=new EventSource("/events");ev.addEventListener("update",async()=>{
  try{const d=await (await fetch("/data.json",{cache:"no-store"})).json();ITEMS=prep(d.items);OPPS=d.opps;STAMP=d.stamp;
    lastUpdate=new Date().toLocaleTimeString([],{hour:"2-digit",minute:"2-digit"});facets();render();$("main").classList.remove("flash");void $("main").offsetWidth;$("main").classList.add("flash")}catch(e){}})}
// Charts: one measure per chart (no dual axis). Price: daily low as the line, the day's low-to-high as a band.
function chart(it,kind){const W=640,H=200,L=56,R=12,T=12,B=26,w=W-L-R,h=H-T-B,n=it.h.length;
  const ys=kind==="price"?it.h.flatMap(d=>[d[1],d[2]]):it.h.map(d=>d[3]);const max=Math.max(...ys)*1.08||1,min=kind==="price"?Math.min(...ys)*0.92:0;
  const slot=w/n,x=(i)=>L+(i+0.5)*slot,y=(v)=>T+h-(v-min)/(max-min||1)*h;
  const ticks=[0,0.25,0.5,0.75,1].map(t=>min+(max-min)*t);
  let s='<svg viewBox="0 0 '+W+' '+H+'" role="img" aria-label="'+(kind==="price"?"Daily lowest price":"Quantity listed")+' for '+esc(it.n)+'">';
  for(const t of ticks)s+='<line x1="'+L+'" x2="'+(W-R)+'" y1="'+y(t)+'" y2="'+y(t)+'" stroke="var(--grid)"/><text x="'+(L-8)+'" y="'+(y(t)+4)+'" text-anchor="end">'+(kind==="price"?money(t):Math.round(t).toLocaleString())+'</text>';
  it.h.forEach((d,i)=>{s+='<text x="'+x(i)+'" y="'+(H-6)+'" text-anchor="middle">'+d[0]+'</text>'});
  if(kind==="price"){if(n>1)s+='<path d="M'+it.h.map((d,i)=>x(i)+","+y(d[2])).join("L")+"L"+[...it.h].reverse().map((d,j)=>x(n-1-j)+","+y(d[1])).join("L")+'Z" fill="var(--band)"/>';
    if(n>1)s+='<path d="M'+it.h.map((d,i)=>x(i)+","+y(d[1])).join("L")+'" fill="none" stroke="var(--series-1)" stroke-width="2" stroke-linejoin="round"/>';
    it.h.forEach((d,i)=>{s+='<circle cx="'+x(i)+'" cy="'+y(d[1])+'" r="4" fill="var(--series-1)" stroke="var(--surface-1)" stroke-width="2"/>'})}
  else{const bw=Math.min(48,slot*0.5);it.h.forEach((d,i)=>{const top=y(d[3]),hh=T+h-top;s+='<path d="M'+(x(i)-bw/2)+','+(T+h)+'V'+(top+4)+'q0,-4 4,-4H'+(x(i)+bw/2-4)+'q4,0 4,4V'+(T+h)+'Z" fill="var(--series-1)"/>'})}
  it.h.forEach((d,i)=>{s+='<rect x="'+(L+i*slot)+'" y="'+T+'" width="'+slot+'" height="'+h+'" fill="transparent" data-i="'+i+'"/>'});
  return s+"</svg>"}
function hover(card,it,kind){const tip=card.querySelector(".tip");card.querySelector("svg").addEventListener("mousemove",(e)=>{const r=e.target.closest("rect[data-i]");if(!r){tip.style.display="none";return}
  const d=it.h[+r.dataset.i];tip.innerHTML=kind==="price"?"<b>"+d[0]+"</b><br>Low "+money(d[1])+"<br>High "+money(d[2]):"<b>"+d[0]+"</b><br>"+d[3].toLocaleString()+" listed";
  const box=card.getBoundingClientRect();tip.style.display="block";tip.style.left=Math.min(e.clientX-box.left+12,box.width-140)+"px";tip.style.top=(e.clientY-box.top-10)+"px"});
  card.querySelector("svg").addEventListener("mouseleave",()=>tip.style.display="none")}
function show(it){if(!it){$("main").innerHTML=tabs()+'<div class="empty">Pick an item on the left.</div>';return}
  const c=it.h.length>1?'<span class="'+(it.chg>0.1?"up":it.chg<-0.1?"down":"muted")+'">'+pct(it.chg)+'</span>':"–";
  $("main").innerHTML=tabs()+'<div><h2>'+esc(it.n)+'</h2><div class="kind">'+esc(it.k)+' · item '+it.id+'</div></div>'
   +'<div class="tiles"><div class="tile"><div class="lab">Cheapest now</div><div class="val">'+money(it.now)+'</div><div class="note">latest scan</div></div>'
   +'<div class="tile"><div class="lab">Usual</div><div class="val">'+money(it.usual)+'</div><div class="note">median daily price</div></div>'
   +'<div class="tile"><div class="lab">Change</div><div class="val">'+c+'</div><div class="note">first to last day</div></div>'
   +'<div class="tile"><div class="lab">Listed</div><div class="val">'+it.listed.toLocaleString()+'</div><div class="note">quantity, latest day</div></div>'
   +'<div class="tile"><div class="lab">Snipe at or under</div><div class="val">'+(it.snipe>0?money(it.snipe):"–")+'</div><div class="note">'+(it.snipe>0?"80% of floor, clears 1s deposit + 2s":"too cheap to flip singly")+'</div></div></div>'
   +'<div class="card" id="pc"><h3>Lowest price by day</h3><div class="sub">Line: the day\\'s lowest listing. Band: lowest to highest cheapest listing seen that day.</div>'+chart(it,"price")+'<div class="tip"></div></div>'
   +'<div class="card" id="qc"><h3>Quantity listed by day</h3>'+chart(it,"qty")+'<div class="tip"></div></div>'
   +'<div class="card"><h3>Values</h3><table><thead><tr><th>Day</th><th>Lowest</th><th>Highest</th><th>Listed</th></tr></thead><tbody>'+it.h.map(d=>'<tr><td>'+d[0]+'</td><td>'+money(d[1])+'</td><td>'+money(d[2])+'</td><td>'+d[3].toLocaleString()+'</td></tr>').join("")+'</tbody></table></div>';
  hover($("pc"),it,"price");hover($("qc"),it,"qty")}
if(st.sel==null){const m=ITEMS.find(i=>i.n==="Mageweave Cloth");st.sel=m?m.id:null}
if(!OPPS&&st.view==="opps")st.view="item";
render();
</script></body></html>`;
}

export function openInBrowser(target) {
  execFile(process.platform === "win32" ? "explorer.exe" : process.platform === "darwin" ? "open" : "xdg-open", [target]);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const argv = process.argv.slice(2);
  const flag = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
  const out = path.resolve(flag("out", path.join(os.tmpdir(), "fbl-prices.html")));
  const { items, stamp } = buildItems();
  fs.writeFileSync(out, page(items, stamp, { opps: analyze() }));
  console.log(`${out} (${items.length} items, scans through ${stamp})`);
  if (argv.includes("--open")) openInBrowser(out);
}
