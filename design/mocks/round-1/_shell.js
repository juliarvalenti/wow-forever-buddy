// Shared mock shell: titlebar + sidebar + icon sprite.
// A page provides <body data-page="dashboard"> and a <main class="main"> element.
(function () {
  const ICONS = {
    crest: '<path d="M12 2l7 3v6c0 5-3.5 8.5-7 11-3.5-2.5-7-6-7-11V5z"/><path d="M12 7v9M8.5 10.5h7"/>',
    home: '<path d="M3 11l9-7 9 7"/><path d="M5 10v10h14V10"/><path d="M10 20v-6h4v6"/>',
    archive: '<rect x="3" y="4" width="18" height="5" rx="1"/><path d="M5 9v10h14V9"/><path d="M10 13h4"/>',
    users: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20c.8-3.6 3.4-5.5 6.5-5.5s5.7 1.9 6.5 5.5"/><path d="M16 4.5a3.5 3.5 0 010 7M18 14.8c1.9.8 3.1 2.6 3.5 5.2"/>',
    coins: '<ellipse cx="9" cy="7" rx="6" ry="3"/><path d="M3 7v4c0 1.7 2.7 3 6 3s6-1.3 6-3V7"/><path d="M9 14v3c0 1.7 2.7 3 6 3s6-1.3 6-3v-4c0-1.7-2.7-3-6-3"/>',
    scroll: '<path d="M7 3h11a2 2 0 012 2v1H9"/><path d="M7 3a2 2 0 00-2 2v13a3 3 0 003 3h10a2 2 0 002-2V8"/><path d="M9 10h7M9 14h7"/>',
    scale: '<path d="M12 3v18M7 21h10M5 7h14"/><path d="M5 7l-3 7a3 3 0 006 0zM19 7l-3 7a3 3 0 006 0z"/>',
    puzzle: '<path d="M10 3h4v3a2 2 0 104 0V3h3v7h-3a2 2 0 100 4h3v7h-7v-3a2 2 0 10-4 0v3H3v-7h3a2 2 0 100-4H3V3z"/>',
    terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3M12 15h5"/>',
    spark: '<path d="M12 2l2.4 6.6L21 11l-6.6 2.4L12 20l-2.4-6.6L3 11l6.6-2.4z"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.8-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.8 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.8.3H9a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.8V9a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z"/>',
    folder: '<path d="M3 6a2 2 0 012-2h4l2 2h8a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>',
    check: '<path d="M4 12l5 5L20 6"/>',
    alert: '<path d="M12 3l10 18H2z"/><path d="M12 10v4M12 17.5v.5"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    restore: '<path d="M3 12a9 9 0 103-6.7L3 8"/><path d="M3 3v5h5"/>',
    download: '<path d="M12 3v12M7 10l5 5 5-5M4 20h16"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>',
    more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
    chevron: '<path d="M9 6l6 6-6 6"/>',
    left: '<path d="M15 6l-6 6 6 6"/>',
    down: '<path d="M6 9l6 6 6-6"/>',
    lock: '<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 018 0v4"/>',
    play: '<path d="M7 4l13 8-13 8z"/>',
    refresh: '<path d="M20 11a8 8 0 00-14.5-4.5L3 9M4 13a8 8 0 0014.5 4.5L21 15"/><path d="M3 4v5h5M21 20v-5h-5"/>',
    bag: '<path d="M5 8h14l-1 13H6z"/><path d="M9 8V6a3 3 0 016 0v2"/>',
    sword: '<path d="M14.5 3H21v6.5L10 20.5 3.5 14z"/><path d="M5 19l-2 2M7.5 16.5l-3-3"/>',
    bank: '<path d="M3 10l9-6 9 6M5 10v8M9.5 10v8M14.5 10v8M19 10v8M3 21h18"/>',
    filter: '<path d="M3 5h18l-7 8v6l-4 2v-8z"/>',
    external: '<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5"/>',
    x: '<path d="M6 6l12 12M18 6L6 18"/>',
    min: '<path d="M6 12h12"/>',
    max: '<rect x="6" y="6" width="12" height="12"/>',
    up: '<path d="M7 17L17 7M9 7h8v8"/>',
    file: '<path d="M14 3H6v18h12V7z"/><path d="M14 3v4h4"/>',
  };
  window.icon = (n, cls = '') => `<svg class="i ${cls}" viewBox="0 0 24 24">${ICONS[n] || ''}</svg>`;

  const NAV = [
    { group: 'Overview' },
    { id: 'dashboard', label: 'Dashboard', icon: 'home', href: 'dashboard.html' },
    { id: 'characters', label: 'Characters', icon: 'users', href: 'characters.html', count: 7 },
    { id: 'gold', label: 'Gold & History', icon: 'coins', href: '#' },
    { id: 'sessions', label: 'Sessions', icon: 'scroll', href: '#' },
    { id: 'ah', label: 'Auction House', icon: 'scale', href: '#' },
    { group: 'Game files' },
    { id: 'backups', label: 'Backups', icon: 'archive', href: 'backups.html', count: 23 },
    { id: 'addons', label: 'Addons', icon: 'puzzle', href: '#', soon: true },
    { id: 'macros', label: 'Macros', icon: 'terminal', href: '#', soon: true },
    { id: 'weakauras', label: 'WeakAuras', icon: 'spark', href: '#', soon: true },
  ];

  const page = document.body.dataset.page;
  const wow = document.body.dataset.wow || 'running';
  const main = document.querySelector('main.main');

  const nav = NAV.map(n => n.group
    ? `<div class="group">${n.group}</div>`
    : `<a href="${n.href}" class="${n.id === page ? 'active' : ''} ${n.soon ? 'soon' : ''}">${icon(n.icon)}<span>${n.label}</span>${n.soon ? '<span class="tag">Soon</span>' : n.count ? `<span class="count">${n.count}</span>` : ''}</a>`
  ).join('');

  const wowRow = wow === 'running'
    ? `<div class="row"><span class="dot live"></span><span><b>WoW is running</b> · Thrandor</span></div>`
    : `<div class="row"><span class="dot"></span><span><b>WoW is closed</b></span></div>`;

  const shell = document.createElement('div');
  shell.className = 'window';
  shell.innerHTML = `
    <div class="titlebar">
      ${icon('crest', 'crest')}<span>WoW Forever Buddy</span><span class="grow"></span>
      <div class="wc"><span>${icon('min', 'sm')}</span><span>${icon('max', 'sm')}</span><span>${icon('x', 'sm')}</span></div>
    </div>
    <div class="app">
      <aside class="sidebar">
        <div class="brand"><div class="name">Forever Buddy</div><div class="sub">WoW: Forever companion</div></div>
        <div class="rule"></div>
        <nav class="nav">${nav}</nav>
        <div class="status">
          ${wowRow}
          <div class="row"><span class="dot ok"></span><span>Game folder found</span></div>
          <div class="path">D:\\Games\\WoW Forever\\_classic_</div>
        </div>
        <nav class="nav" style="flex:none;padding-top:0;padding-bottom:10px"><a href="#">${icon('gear')}<span>Settings</span></a></nav>
      </aside>
    </div>`;
  document.body.prepend(shell);
  shell.querySelector('.app').appendChild(main);

  // hydrate inline icon placeholders: <i data-i="name"></i>
  document.querySelectorAll('i[data-i]').forEach(el => { el.outerHTML = icon(el.dataset.i, el.className); });
})();
