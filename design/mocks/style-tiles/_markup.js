// Shared dashboard markup for the three style tiles. Each tile supplies only CSS,
// so the comparison is purely about material, not layout.
(function () {
  const P = {
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
    chevron: '<path d="M9 6l6 6-6 6"/>',
    refresh: '<path d="M20 11a8 8 0 00-14.5-4.5L3 9M4 13a8 8 0 0014.5 4.5L21 15"/><path d="M3 4v5h5M21 20v-5h-5"/>',
    play: '<path d="M7 4l13 8-13 8z"/>',
    x: '<path d="M6 6l12 12M18 6L6 18"/>', min: '<path d="M6 12h12"/>', max: '<rect x="6" y="6" width="12" height="12"/>',
    shield: '<path d="M12 2l8 3v6c0 5.5-3.8 9.3-8 11-4.2-1.7-8-5.5-8-11V5z"/>',
  };
  const i = (n, c = '') => `<svg class="i ${c}" viewBox="0 0 24 24">${P[n]}</svg>`;
  const coins = (g, s, c) => `<span class="coins"><span class="g">${g}</span>${s != null ? `<span class="s">${s}</span>` : ''}${c != null ? `<span class="c">${c}</span>` : ''}</span>`;

  const nav = [
    ['g', 'Overview'],
    ['home', 'Dashboard', 1], ['users', 'Characters', 0, '7'], ['coins', 'Ledger'], ['scroll', 'Adventures'], ['scale', 'Auction House'],
    ['g', 'Game files'],
    ['archive', 'Backups', 0, '23'], ['puzzle', 'Addons', 0, 'soon'], ['terminal', 'Macros', 0, 'soon'], ['spark', 'WeakAuras', 0, 'soon'],
  ].map(([ic, label, on, n]) => ic === 'g'
    ? `<div class="nav-group">${label}</div>`
    : `<a class="nav-item ${on ? 'active' : ''} ${n === 'soon' ? 'soon' : ''}" title="${label}">${i(ic)}<span class="lbl">${label}</span>${n ? `<span class="n">${n}</span>` : ''}</a>`).join('');

  const item = (q, glyph, name, x) => `<li><span class="ico q-${q}"><b>${glyph}</b></span><span class="nm q-${q}">${name}</span>${x ? `<span class="x">${x}</span>` : ''}</li>`;
  const alt = (cls, name, g, lv, note = '') => `<li><span class="cdot c-${cls}"></span><span class="an c-${cls}">${name}</span>${note ? `<span class="note">${note}</span>` : ''}<span class="ag">${coins(g)}</span><span class="lv">${lv}</span></li>`;

  document.body.insertAdjacentHTML('afterbegin', `
  <div class="win">
    <div class="titlebar"><span class="tb-crest">${i('shield')}</span><span>WoW Forever Buddy</span><span class="grow"></span><span class="wc">${i('min')}</span><span class="wc">${i('max')}</span><span class="wc">${i('x')}</span></div>
    <div class="app">
      <aside class="side">
        <div class="brand"><div class="mark">${i('shield')}</div><div class="bt"><div class="name">Forever Buddy</div><div class="sub">for WoW: Forever</div></div></div>
        <nav class="nav">${nav}</nav>
        <div class="status">
          <div class="st-row"><span class="live"></span><span class="st-t"><b>WoW is running</b> · Thrandor</span></div>
          <div class="st-row"><span class="okdot"></span><span class="st-t">Game folder found</span></div>
        </div>
        <a class="nav-item settings" title="Settings">${i('gear')}<span class="lbl">Settings</span></a>
      </aside>

      <main class="main">
        <header class="head">
          <div>
            <h1>Dashboard</h1>
            <div class="lede">Sunday, 4 October · Realm Ashenvale · 7 characters</div>
          </div>
          <div class="actions">
            <button class="btn ghost">${i('folder')}Open game folder</button>
            <button class="primary">${i('archive')}<span>Back up now</span></button>
          </div>
        </header>

        <section class="strip">
          <div class="tile"><div class="k">Account gold</div><div class="v gold-v">${coins('6,812', '47', '09')}</div><div class="s"><span class="up">▲ 412g</span> this week</div>
            <svg class="spark" viewBox="0 0 74 24"><polyline points="0,20 10,18 18,19 27,14 36,15 45,10 54,11 63,6 74,3"/></svg></div>
          <div class="tile"><div class="k">Last backup</div><div class="v">2 hours ago</div><div class="s">Auto · on game exit · 48 MB</div></div>
          <div class="tile"><div class="k">Game</div><div class="v"><span class="live"></span>Running</div><div class="s">Thrandor · 1h 42m this session</div></div>
          <div class="tile"><div class="k">Characters</div><div class="v">7 <small>· 2 at 60</small></div><div class="s">3 fully rested</div></div>
        </section>

        <section class="cols">
          <article class="panel recap">
            <div class="ph"><h2>Last adventure</h2><span class="meta">Yesterday 19:40 – 22:52 · 3h 12m</span><a class="link">Journal ${i('chevron')}</a></div>
            <div class="pb">
              <div class="who">
                <div class="crest c-paladin">${i('shield')}<b>T</b></div>
                <div class="who-t"><div class="wn">Thrandor</div><div class="wc2">Human Paladin · Level 59 → <b class="lvl">60</b></div></div>
                <div class="ding" aria-label="Level up"><span class="ding-ring"></span><span class="ding-t">Ding!</span><span class="ding-n">60</span></div>
              </div>
              <div class="tally">
                <div><div class="k">Gold</div><div class="v up">+${coins('312', '40')}</div></div>
                <div><div class="k">Experience</div><div class="v">+148,210</div></div>
                <div><div class="k">Loot</div><div class="v">47 items</div></div>
                <div><div class="k">Quests</div><div class="v">9</div></div>
              </div>
              <svg class="goldline" viewBox="0 0 600 56" preserveAspectRatio="none">
                <path class="area" d="M0,48 L40,46 80,44 110,45 150,38 190,39 230,32 260,34 300,24 340,27 380,18 420,20 450,21 470,28 500,16 540,10 580,7 600,5 L600,56 L0,56Z"/>
                <polyline class="line" points="0,48 40,46 80,44 110,45 150,38 190,39 230,32 260,34 300,24 340,27 380,18 420,20 450,21 470,28 500,16 540,10 580,7 600,5"/>
                <circle class="mark" cx="470" cy="28" r="3"/>
              </svg>
              <div class="loot">
                <div><h3>Gained</h3><ul class="items">
                  ${item('rare', 'S', 'Truestrike Shoulders')}${item('common', 'R', 'Runecloth', '×40')}${item('uncommon', 'O', 'Righteous Orb', '×2')}${item('common', 'P', 'Major Healing Potion', '×6')}
                </ul></div>
                <div><h3>Spent</h3><ul class="items">
                  ${item('common', 'F', 'Flask of the Titans', '×1')}${item('common', 'P', 'Major Healing Potion', '×4')}${item('poor', 'J', 'Vendor junk, sold', '×22')}
                  <li class="repair"><span class="ico q-poor"><b>⚒</b></span><span class="nm">Repairs</span><span class="x">−18g</span></li>
                </ul></div>
              </div>
              <div class="zones"><span class="zl">Travelled</span><span class="zone">Eastern Plaguelands</span><span class="zone">Stratholme</span><span class="zone">Light's Hope Chapel</span></div>
            </div>
          </article>

          <div class="stack">
            <article class="panel">
              <div class="ph"><h2>Game folder</h2><a class="link icon">${i('refresh')}</a></div>
              <ul class="checks">
                <li>${i('check', 'ok')}<div><div>Client detected</div><small>D:\\Games\\WoW Forever\\_classic_ · 1.15.4</small></div></li>
                <li>${i('check', 'ok')}<div><div>ForeverBuddy addon</div><small>v0.2.0 · on 7 / 7 characters</small></div></li>
                <li>${i('check', 'ok')}<div><div>SavedVariables read</div><small>4 min ago · next on logout or /reload</small></div></li>
                <li>${i('alert', 'warn')}<div><div>Auction prices are 3 days old</div><small>Scan the AH in-game to refresh</small></div></li>
              </ul>
            </article>
            <article class="panel">
              <div class="ph"><h2>Characters</h2><a class="link">All ${i('chevron')}</a></div>
              <ul class="roster">
                ${alt('warrior', 'Coinpurse', '2,779', 12, 'bank')}${alt('paladin', 'Thrandor', '2,140', 60)}${alt('druid', 'Velyra', '1,066', 60)}${alt('hunter', 'Brannic', '488', 52)}${alt('mage', 'Fizzwick', '212', 44)}
              </ul>
            </article>
          </div>
        </section>
        ${document.body.dataset.extra || ''}
      </main>
    </div>
  </div>`);

  // delight: coin counter rolls up on load
  const g = document.querySelector('.strip .gold-v .g');
  if (g && !matchMedia('(prefers-reduced-motion: reduce)').matches && !location.search.includes('still')) {
    const end = 6812, t0 = performance.now();
    const step = t => { const k = Math.min(1, (t - t0) / 900), e = 1 - Math.pow(1 - k, 3); g.textContent = Math.round(end * e).toLocaleString('en-US'); if (k < 1) requestAnimationFrame(step); };
    requestAnimationFrame(step);
  }
})();
