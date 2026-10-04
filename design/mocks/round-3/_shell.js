// Round 2 shell for the "D" language: titlebar, leather sidebar w/ ribbon nav, icon set,
// class crests, portrait slot, and the item-tooltip engine.
// Page contract: <body data-page="dashboard" data-wow="running|closed"> + <main class="main">.
//   <i data-i="name" class="sm"></i>                 → icon
//   <span data-portrait="crest|armory|shot" data-class="paladin" data-race="Hu" data-w="56"></span> → portrait slot
//   any element with data-tt="itemKey"               → in-game tooltip on hover
//   ?tt=itemKey in the URL pins that tooltip (for screenshots); ?still freezes motion
(function () {
  const ICONS = {
    shield: '<path d="M12 2l8 3v6c0 5.5-3.8 9.3-8 11-4.2-1.7-8-5.5-8-11V5z"/>',
    home: '<path d="M3 11l9-7 9 7"/><path d="M5 10v10h14V10"/><path d="M10 20v-6h4v6"/>',
    users: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20c.8-3.6 3.4-5.5 6.5-5.5s5.7 1.9 6.5 5.5"/><path d="M16 4.5a3.5 3.5 0 010 7M18 14.8c1.9.8 3.1 2.6 3.5 5.2"/>',
    coins: '<ellipse cx="9" cy="7" rx="6" ry="3"/><path d="M3 7v4c0 1.7 2.7 3 6 3s6-1.3 6-3V7"/><path d="M9 14v3c0 1.7 2.7 3 6 3s6-1.3 6-3v-4c0-1.7-2.7-3-6-3"/>',
    scroll: '<path d="M7 3h11a2 2 0 012 2v1H9"/><path d="M7 3a2 2 0 00-2 2v13a3 3 0 003 3h10a2 2 0 002-2V8"/><path d="M9 10h7M9 14h7"/>',
    scale: '<path d="M12 3v18M7 21h10M5 7h14"/><path d="M5 7l-3 7a3 3 0 006 0zM19 7l-3 7a3 3 0 006 0z"/>',
    archive: '<rect x="3" y="4" width="18" height="5" rx="1"/><path d="M5 9v10h14V9"/><path d="M10 13h4"/>',
    puzzle: '<path d="M10 3h4v3a2 2 0 104 0V3h3v7h-3a2 2 0 100 4h3v7h-7v-3a2 2 0 10-4 0v3H3v-7h3a2 2 0 100-4H3V3z"/>',
    terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3M12 15h5"/>',
    spark: '<path d="M12 2l2.4 6.6L21 11l-6.6 2.4L12 20l-2.4-6.6L3 11l6.6-2.4z"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9L7 7M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1"/>',
    folder: '<path d="M3 6a2 2 0 012-2h4l2 2h8a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>',
    check: '<path d="M4 12l5 5L20 6"/>',
    alert: '<path d="M12 3l10 18H2z"/><path d="M12 10v4M12 17.5v.5"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    restore: '<path d="M3 12a9 9 0 103-6.7L3 8"/><path d="M3 3v5h5"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>',
    more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
    chevron: '<path d="M9 6l6 6-6 6"/>', left: '<path d="M15 6l-6 6 6 6"/>', down: '<path d="M6 9l6 6 6-6"/>',
    lock: '<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 018 0v4"/>',
    play: '<path d="M7 4l13 8-13 8z"/>',
    refresh: '<path d="M20 11a8 8 0 00-14.5-4.5L3 9M4 13a8 8 0 0014.5 4.5L21 15"/><path d="M3 4v5h5M21 20v-5h-5"/>',
    bag: '<path d="M5 8h14l-1 13H6z"/><path d="M9 8V6a3 3 0 016 0v2"/>',
    sword: '<path d="M14.5 3H21v6.5L10 20.5 3.5 14z"/><path d="M5 19l-2 2M7.5 16.5l-3-3"/>',
    bank: '<path d="M3 10l9-6 9 6M5 10v8M9.5 10v8M14.5 10v8M19 10v8M3 21h18"/>',
    mail: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 7l9 6 9-6"/>',
    file: '<path d="M14 3H6v18h12V7z"/><path d="M14 3v4h4"/>',
    key: '<circle cx="8" cy="15" r="4"/><path d="M11 12l9-9M17 6l3 3M15 8l2 2"/>',
    link: '<path d="M10 14a4 4 0 005.7 0l3-3a4 4 0 00-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 00-5.7 0l-3 3a4 4 0 005.7 5.7l1-1"/>',
    camera: '<path d="M4 8h3l2-3h6l2 3h3v11H4z"/><circle cx="12" cy="13" r="3.5"/>',
    image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="M21 16l-5-5-9 9"/>',
    globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18"/>',
    x: '<path d="M6 6l12 12M18 6L6 18"/>', min: '<path d="M6 12h12"/>', max: '<rect x="6" y="6" width="12" height="12"/>',
    eye: '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
    plus: '<path d="M12 5v14M5 12h14"/>', trend: '<path d="M3 17l6-6 4 4 8-8"/><path d="M15 7h6v6"/>',
    hourglass: '<path d="M6 3h12M6 21h12M7 3c0 5 10 5 10 9s-10 4-10 9M17 3c0 5-10 5-10 9s10 4 10 9"/>',
    map: '<path d="M9 4l-6 2v14l6-2 6 2 6-2V4l-6 2z"/><path d="M9 4v14M15 6v14"/>',
  };
  const icon = (n, cls = '') => `<svg class="i ${cls}" viewBox="0 0 24 24">${ICONS[n] || ''}</svg>`;
  window.icon = icon;

  // class crest: shield in class tint + class glyph
  const GLYPH = {
    paladin: '<path d="M8 5h8v5H8z"/><path d="M11 10h2v10h-2z"/>',
    druid: '<circle cx="8" cy="8" r="1.8"/><circle cx="12" cy="6.5" r="1.8"/><circle cx="16" cy="8" r="1.8"/><path d="M12 11c3 0 5 3 5 5.5 0 1.7-1.5 2.5-3 2-1.3-.4-2.7-.4-4 0-1.5.5-3-.3-3-2C7 14 9 11 12 11z"/>',
    hunter: '<path d="M6 18L17 7" stroke-width="2"/><path d="M13 6h5v5z"/><path d="M5 16l3 3-3 1z"/>',
    mage: '<path d="M12 3l2 6.5L20.5 12 14 14.5 12 21l-2-6.5L3.5 12 10 9.5z"/>',
    priest: '<circle cx="12" cy="12" r="4.5"/><path d="M12 3v3M12 18v3M3 12h3M18 12h3" stroke-width="2"/>',
    rogue: '<path d="M12 3l2 3v9h-4V6z"/><path d="M8 15h8v2H8z"/><path d="M11 17h2v4h-2z"/>',
    warrior: '<path d="M5 5l14 14M19 5L5 19" stroke-width="2.2"/><path d="M4 8l4-4M16 4l4 4"/>',
  };
  // colours come from _d.css (.crest > path / .crest > g); var() doesn't work in SVG attributes
  window.crest = cls => `<svg class="crest" viewBox="0 0 24 24"><path d="M12 1.5l9 3.3v6.6c0 6.2-4.3 10.3-9 12.1-4.7-1.8-9-5.9-9-12.1V4.8z"/><g transform="translate(4.2 4.2) scale(.65)">${GLYPH[cls] || ''}</g></svg>`;

  const NAV = [
    { group: 'Overview' },
    { id: 'dashboard', label: 'Dashboard', icon: 'home', href: 'dashboard.html' },
    { id: 'characters', label: 'Characters', icon: 'users', href: 'characters.html', n: 7 },
    { id: 'gold', label: 'Ledger', icon: 'coins', href: 'gold.html' },
    { id: 'sessions', label: 'Adventures', icon: 'scroll', href: 'session.html' },
    { id: 'ah', label: 'Auction House', icon: 'scale', href: 'ah.html' },
    { group: 'Game files' },
    { id: 'backups', label: 'Backups', icon: 'archive', href: 'backups.html', n: 23 },
    { id: 'addons', label: 'Addons', icon: 'puzzle', href: 'addons.html', soon: 1 },
    { id: 'macros', label: 'Macros', icon: 'terminal', href: 'macros.html', soon: 1 },
    { id: 'weakauras', label: 'WeakAuras', icon: 'spark', href: 'weakauras.html', soon: 1 },
  ];
  // body data-wow: running | closed | nofolder ; data-addon: none (v0.1, no companion addon yet)
  const page = document.body.dataset.page, wow = document.body.dataset.wow || 'running';
  const noAddon = document.body.dataset.addon === 'none';
  const main = document.querySelector('main.main');
  const nav = NAV.map(n => n.group ? `<div class="nav-group">${n.group}</div>`
    : `<a class="nav-item ${n.id === page ? 'active' : ''} ${n.soon ? 'soon' : ''}" href="${n.href}" title="${n.label}">${icon(n.icon)}<span class="lbl">${n.label}</span>${n.soon ? '<span class="n">soon</span>' : n.n && wow !== 'nofolder' ? `<span class="n">${n.n}</span>` : ''}</a>`).join('');
  const who = noAddon ? '' : ` · ${document.body.dataset.char || 'Thrandor'}`;
  const wowRow = wow === 'running'
    ? `<div class="st-row"><span class="live"></span><span class="st-t"><b>WoW is running</b>${who}</span></div>`
    : `<div class="st-row"><span class="okdot" style="background:#6a6358"></span><span class="st-t"><b>WoW is closed</b></span></div>`;
  const folderRow = wow === 'nofolder'
    ? `<div class="st-row"><span class="okdot" style="background:var(--ember-2)"></span><span class="st-t">Game folder not set</span></div>`
    : `<div class="st-row"><span class="okdot"></span><span class="st-t">Game folder found</span></div>`;

  const win = document.createElement('div');
  win.className = 'win';
  win.innerHTML = `
    <div class="titlebar">${icon('shield', 'sm')}<span>WoW Forever Buddy</span><span class="grow"></span><span class="wc">${icon('min', 'sm')}</span><span class="wc">${icon('max', 'sm')}</span><span class="wc">${icon('x', 'sm')}</span></div>
    <div class="app">
      <aside class="side">
        <div class="brand"><div class="mark">${icon('shield')}</div><div class="bt"><div class="name">Forever Buddy</div><div class="sub">for WoW: Forever</div></div></div>
        <nav class="nav">${nav}</nav>
        <div class="status">${wow === 'nofolder' ? '' : wowRow}${folderRow}</div>
        <div class="side-foot"><a class="nav-item ${page === 'settings' ? 'active' : ''}" href="settings.html" title="Settings">${icon('gear')}<span class="lbl">Settings</span></a></div>
      </aside>
    </div>`;
  document.body.prepend(win);
  win.querySelector('.app').appendChild(main);
  main.insertAdjacentHTML('afterbegin', '<div class="vignette"></div>');

  document.querySelectorAll('i[data-i]').forEach(el => { el.outerHTML = icon(el.dataset.i, el.className); });

  // portrait slots
  const SRC = { armory: 'portraits/armory-thrandor.svg', shot: 'portraits/screenshot-velyra.svg' };
  document.querySelectorAll('[data-portrait]').forEach(el => {
    const kind = el.dataset.portrait, cls = el.dataset.class, w = el.dataset.w || 56;
    const img = kind === 'crest' ? crest(cls) : `<img src="${el.dataset.src || SRC[kind]}" alt="">`;
    const badge = kind === 'crest' && el.dataset.race ? `<span class="race">${el.dataset.race}</span>` : '';
    const tag = el.dataset.tag ? `<span class="src" title="${kind}">${icon(kind === 'armory' ? 'globe' : kind === 'shot' ? 'camera' : 'shield')}</span>` : '';
    const on = el.dataset.online ? '<span class="live on"></span>' : '';
    el.outerHTML = `<span class="pslot c-${cls}" style="--w:${w}px"><span class="pi">${img}</span>${badge}${tag}${on}</span>`;
  });

  // item tooltips — real in-game tooltip content
  const C = (g, s, c) => `<span class="coins">${g ? `<span class="g">${g}</span>` : ''}${s ? `<span class="s">${s}</span>` : ''}${c ? `<span class="c">${c}</span>` : ''}</span>`;
  const ITEMS = {
    truestrike: { q: 'rare', n: 'Truestrike Shoulders', lines: ['Binds when picked up', ['Shoulder', 'Leather'], '129 Armor', '+24 Agility', '+11 Stamina', ['g', 'Equip: Improves your chance to hit by 2%.'], 'Durability 60 / 60', 'Requires Level 58'], sell: C(2, 31, 40), src: 'Looted 21:47 · Stratholme' },
    runecloth: { q: 'common', n: 'Runecloth', lines: ['Max Stack: 20', ['y', 'Used to make Runecloth armor and bags.']], sell: C(0, 20), src: 'Last scanned on the AH: 1g 12s each' },
    lionheart: { q: 'epic', n: 'Lionheart Helm', lines: ['Binds when equipped', ['Head', 'Plate'], '565 Armor', '+18 Strength', ['g', 'Equip: Improves your chance to get a critical strike by 2%.'], ['g', 'Equip: Improves your chance to hit by 2%.'], 'Durability 100 / 100', 'Requires Level 60'], sell: C(4, 12, 8), src: 'Crafted · Blacksmithing (300)' },
    flask: { q: 'common', n: 'Flask of the Titans', lines: ['Requires Alchemy (300)', ['g', 'Use: Increases the player\'s maximum health by 400 for 2 hrs. Effect persists through death.']], sell: C(0, 50), src: 'Used 21:12 · Stratholme' },
    orb: { q: 'uncommon', n: 'Righteous Orb', lines: ['Max Stack: 20', ['y', 'Used by Thorium Brotherhood smiths.']], sell: C(0, 0, 0), src: 'Looted 20:51 · Stratholme' },
    potion: { q: 'common', n: 'Major Healing Potion', lines: ['Max Stack: 5', ['g', 'Use: Restores 1050 to 1750 health.']], sell: C(0, 10), src: 'Bought from Alchemist' },
    ashkandi: { q: 'epic', n: 'Ashkandi, Greatsword of the Brotherhood', lines: ['Binds when picked up', ['Two-Hand', 'Sword'], ['171 - 258 Damage', 'Speed 3.50'], '+33 Stamina', ['g', 'Equip: +86 Attack Power.'], 'Durability 120 / 120', 'Requires Level 60'], sell: C(23, 41, 50), src: 'Looted 14 Sep · Blackwing Lair' },
    arcanite: { q: 'uncommon', n: 'Arcanite Bar', lines: ['Max Stack: 20'], sell: C(0, 10), src: 'Last scanned on the AH: 38g each' },
  };
  // pages may add their own: <script>window.EXTRA_ITEMS = { key: { q, n, lines, sell, src } }</script> before _shell.js
  Object.assign(ITEMS, window.EXTRA_ITEMS || {});
  window.C = C;
  const tt = document.createElement('div');
  tt.className = 'tt'; tt.style.display = 'none';
  document.body.appendChild(tt);
  const fill = k => {
    const it = ITEMS[k]; if (!it) return false;
    tt.innerHTML = `<div class="t q-${it.q}">${it.n}</div>` + it.lines.map(l => Array.isArray(l)
      ? (l[0] === 'g' || l[0] === 'y' ? `<div class="${l[0]}">${l[1]}</div>` : `<div class="row"><span>${l[0]}</span><span>${l[1]}</span></div>`)
      : `<div>${l}</div>`).join('') + (it.sell ? `<div>Sell Price: ${it.sell}</div>` : '') + (it.src ? `<div class="src">${it.src}</div>` : '');
    return true;
  };
  const place = (el, pinned) => {
    const r = el.getBoundingClientRect(); tt.style.display = 'block';
    let x = r.right + 12, y = r.top - 6;
    if (x + 260 > innerWidth) x = r.left - 268;
    if (y + tt.offsetHeight > innerHeight - 8) y = innerHeight - tt.offsetHeight - 8;
    tt.style.left = x + 'px'; tt.style.top = y + 'px';
    if (pinned) el.classList.add('tt-on');
  };
  document.querySelectorAll('[data-tt]').forEach(el => {
    el.addEventListener('mouseenter', () => { if (fill(el.dataset.tt)) place(el); });
    el.addEventListener('mouseleave', () => { if (!pinned) tt.style.display = 'none'; });
  });
  const pin = new URLSearchParams(location.search).get('tt');
  let pinned = false;
  if (pin) { const el = document.querySelector(`[data-tt="${pin}"][data-pin]`) || document.querySelector(`[data-tt="${pin}"]`); if (el && fill(pin)) { pinned = true; requestAnimationFrame(() => place(el, true)); } }

  if (location.search.includes('still')) document.documentElement.style.setProperty('--still', '1'),
    document.head.insertAdjacentHTML('beforeend', '<style>*{animation:none!important;transition:none!important}</style>');
})();
