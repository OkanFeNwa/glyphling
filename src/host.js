// Connects the prototype page to the app: state shared with the Rust backend,
// native dragging, the widget's clickable shape and the real Info screen.
(() => {
  const T = window.__TAURI__, C = window.__companion, host = window.__host;
  if (!T || !C || !host) return;            // in the browser the prototype stays as is
  const { invoke } = T.core;
  const { listen, emit } = T.event;
  const $ = id => document.getElementById(id);

  // ---------- shared state ----------
  // Each window sends its changes to the backend, which saves them and redistributes them to all.
  // The backend ignores unchanged values, so the echo coming back does not create loops.
  window.__hostEvent = (kind, v) => {
    if (kind === 'mode') { invoke('set_mode', { mode: v }); if (host === 'widget') sendShape(); }
    else if (kind === 'move') invoke('set_move', { on: v });
    else if (kind === 'snap') invoke('set_snap', { on: v });
    else if (kind === 'themes') emit('themes-changed', { from: host });
    else if (kind === 'info') fillInfo();
    else if (kind === 'disks') { store.set('glyphling-disks', JSON.stringify(v)); emit('disks-changed', { from: host }); }
    else if (kind === 'volume') sendVolume(v);
    else if (kind === 'seek') invoke('media_control', { action: 'seek', value: v });
  };
  // localStorage is shared by all of the app's windows
  const store = {
    get(k, d) { try { const v = localStorage.getItem(k); return v == null ? d : v; } catch (e) { return d; } },
    set(k, v) { try { localStorage.setItem(k, v); } catch (e) {} },
  };
  let volTimer = 0, volNext = null;
  function sendVolume(v) {
    volNext = v;
    if (volTimer) return;
    volTimer = setTimeout(() => { volTimer = 0; invoke('set_volume', { v: volNext }); }, 60);
  }
  function applyCfg(c) {
    if (C.getMode() !== c.mode) C.setMode(c.mode);
    if (C.isMoving() !== !!c.move_enabled) C.setMoving(!!c.move_enabled);
    if (C.isSnap() !== !!c.snap) C.setSnap(!!c.snap);
    const wEl = $('widget');
    if (wEl && wEl.classList.contains('left') !== !!c.left) wEl.classList.toggle('left', !!c.left);
    document.querySelectorAll('#dirSeg button').forEach(b => b.setAttribute('aria-pressed', b.dataset.d === (c.panel_dir || 'auto')));
    if ($('swTwo')) $('swTwo').setAttribute('aria-checked', !!c.two_pos);
    if (host === 'widget') sendShape();
  }
  invoke('get_config').then(applyCfg);
  // the theme colors are also needed by the tray icon menu, which is a separate window
  function shareColors() {
    if (host !== 'widget') return;
    const cs = getComputedStyle(document.documentElement), v = k => cs.getPropertyValue(k).trim();
    store.set('glyphling-menu-colors', JSON.stringify({ red: v('--red'), ink: v('--ink'), glass: v('--glass') }));
  }
  shareColors();
  listen('config', e => applyCfg(e.payload));
  listen('themes-changed', e => { if (e.payload.from !== host) C.reloadThemes(); shareColors(); });

  // ---------- widget ----------
  if (host === 'widget') {
    // dragging with the native Windows handler; the backend computes the snapping on release
    window.__hostDrag = () => {
      const color = getComputedStyle(document.documentElement).getPropertyValue('--red').trim();
      invoke('start_drag', { color });
    };
    // clickable shape: outside the circle and the panel, clicks pass through to the desktop
    // the final shape (at the end of the animation): circle with the dial and, if open, the panel
    // --pw wide from the circle's edge, attached to the correct side
    const BEZEL = 11, CORNER = 30;
    const shapeGeom = () => {
      const r = $('ring').getBoundingClientRect(), w = $('widget');
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2, half = r.height / 2;
      const pw = (parseFloat(getComputedStyle(w).getPropertyValue('--pw')) || 560) - r.width / 2;
      const left = w.classList.contains('left'), open = C.getMode() !== 'idle';
      return { cx, cy, half, rb: r.width / 2 + BEZEL, left, panel: open ? { x: left ? cx - pw : cx, y: r.top, w: pw, h: r.height } : null };
    };
    document.body.insertAdjacentHTML('beforeend', '<svg id="moveOutline" aria-hidden="true"><path/></svg>');
    // dashed outline of the circle + panel union, offset by o px
    function drawOutline(g) {
      const o = 6, R = g.rb + o, { cx, cy } = g;
      let d;
      if (!g.panel) d = `M ${cx - R} ${cy} a ${R} ${R} 0 1 0 ${2 * R} 0 a ${R} ${R} 0 1 0 ${-2 * R} 0 Z`;
      else {
        const h = g.half + o, k = Math.sqrt(Math.max(0, R * R - h * h)), c = CORNER + o, s = g.left ? -1 : 1;
        const end = g.left ? g.panel.x - o : g.panel.x + g.panel.w + o;
        d = `M ${cx + s * k} ${cy - h} L ${end - s * c} ${cy - h} A ${c} ${c} 0 0 ${g.left ? 0 : 1} ${end} ${cy - h + c}`
          + ` L ${end} ${cy + h - c} A ${c} ${c} 0 0 ${g.left ? 0 : 1} ${end - s * c} ${cy + h} L ${cx + s * k} ${cy + h}`
          + ` A ${R} ${R} 0 1 ${g.left ? 0 : 1} ${cx + s * k} ${cy - h} Z`;
      }
      document.querySelector('#moveOutline path').setAttribute('d', d);
    }
    // the shape visible right now, even mid-animation: circle with the dial, panel,
    // dial label. The backend clips the window to it (clicks and blur).
    // While moving it grows to fit the dashed outline.
    const rectPart = (r, rad, pad = 0) => ({ x: r.left - pad, y: r.top - pad, w: r.width + 2 * pad, h: r.height + 2 * pad, r: rad + pad });
    let lastKey = '';
    function shapeTick() {
      const g = shapeGeom(), pad = document.body.classList.contains('moving') ? 9 : 1, parts = [];
      parts.push({ x: g.cx - g.rb - pad, y: g.cy - g.rb - pad, w: 2 * (g.rb + pad), h: 2 * (g.rb + pad), r: g.rb + pad });
      const pr = document.getElementById('panel').getBoundingClientRect();
      if (pr.width > 2 && getComputedStyle(document.getElementById('panel')).opacity > 0.02) parts.push(rectPart(pr, CORNER, pad));
      const lbl = document.querySelector('.pick-lbl.on');
      if (lbl) parts.push(rectPart(lbl.getBoundingClientRect(), 999, 2));
      const key = JSON.stringify(parts.map(p => [p.x, p.y, p.w, p.h].map(Math.round)));
      if (key !== lastKey) {
        lastKey = key;
        invoke('set_shape', { shape: { parts } });
        drawOutline(g);
      }
      requestAnimationFrame(shapeTick);
    }
    const shapeNow = () => { lastKey = ''; };
    window.sendShape = shapeNow;
    requestAnimationFrame(shapeTick);
    // right click on the circle goes back to the mascot (handled by the prototype); the menu stays on the tray icon
    addEventListener('contextmenu', e => e.preventDefault());
    // the mascot follows the mouse across the whole screen
    listen('cursor', e => C.look(e.payload.x, e.payload.y));
    listen('action', e => {
      if (e.payload === 'sleep') C.toggleSleep();
      else if (e.payload === 'seconds') C.toggleSec();
    });
  }
  function sendShape() { if (window.sendShape) window.sendShape(); }

  // ---------- first-run tutorial ----------
  // The page's tutorial dims the whole screen and points at the tray icon, so while it
  // runs the widget window covers the monitor; the widget is shifted to stay in place.
  if (host === 'widget') {
    let tray = null;
    window.__hostTut = async on => {
      if (!on) {
        window.__hostTutOn = false;
        document.documentElement.classList.remove('tut-host');
        await invoke('tutorial', { on: false });
        sendShape();
        return;
      }
      const p = await invoke('tutorial', { on: true });
      if (!p) return;                        // no monitor info: show it inside the window
      document.body.style.setProperty('--tx', p.dx + 'px');
      document.body.style.setProperty('--ty', p.dy + 'px');
      document.documentElement.classList.add('tut-host');
      tray = p.tray;
      window.__hostTutOn = true;
    };
    // stand-in for the tray icon: the real one, or the bottom right corner of the screen
    window.__tutTray = () => {
      let el = $('tutTray');
      if (!el) { el = document.createElement('div'); el.id = 'tutTray'; document.body.appendChild(el); }
      const [x, y, w, h] = tray || [innerWidth - 140, innerHeight - 44, 32, 32];
      Object.assign(el.style, { left: x + 'px', top: y + 'px', width: w + 'px', height: h + 'px' });
      return el;
    };
    listen('tutorial-start', () => window.__tutOpen && window.__tutOpen());
  }

  // ---------- Settings window ----------
  if (host === 'settings') {
    $('posReset').addEventListener('click', e => { e.stopImmediatePropagation(); invoke('reset_position'); }, true);
    $('posReset').textContent = 'Ripristina';
    const resetRow = $('posReset').closest('.srow');
    if (resetRow) resetRow.querySelector('b').textContent = 'Riporta alla posizione iniziale';
    if (resetRow) resetRow.querySelector('small').textContent = "In basso a destra sul monitor principale, con spazio per il pannello.";
    // start with Windows
    const posGroup = $('posReset').closest('.sgroup');
    if (posGroup) {
      posGroup.insertAdjacentHTML('afterend', `<div class="sgroup"><h3>Avvio</h3>
        <div class="srow"><div><b>Avvia con Windows</b><small>Glyphling parte da solo quando accedi e si mette dove l'avevi lasciato.</small></div><button class="sw-t" id="swAuto" role="switch" aria-checked="false" aria-label="Avvia con Windows"></button></div></div>`);
      const sw = $('swAuto');
      invoke('get_autostart').then(on => sw.setAttribute('aria-checked', on));
      sw.addEventListener('click', () => invoke('set_autostart', { on: sw.getAttribute('aria-checked') !== 'true' }).then(on => sw.setAttribute('aria-checked', on)));
    }
    // two positions: saved and applied by the backend
    $('swTwo').addEventListener('click', e => {
      e.stopImmediatePropagation();
      invoke('set_two_pos', { on: $('swTwo').getAttribute('aria-checked') !== 'true' });
    }, true);
    // the tutorial runs in the widget window
    $('tutAgain').addEventListener('click', e => {
      e.stopImmediatePropagation();
      emit('tutorial-start');
      T.window.getCurrentWindow().close();
    }, true);
    $('dirSeg').addEventListener('click', e => {
      const b = e.target.closest('button'); if (!b) return;
      e.stopImmediatePropagation();
      invoke('set_panel_dir', { dir: b.dataset.d });
    }, true);
    // title bar: dragged from the empty area, minimize and close buttons
    const top = document.querySelector('.swin-top'), win = T.window.getCurrentWindow();
    // only the title and the empty space drag: tabs and buttons must stay clickable
    for (const el of [top, ...top.querySelectorAll(':scope > div:not(.tabs), :scope > div:not(.tabs) > *')]) el.setAttribute('data-tauri-drag-region', '');
    const close = $('setClose');
    close.insertAdjacentHTML('beforebegin', '<span class="wbtns" id="wbtns"></span>');
    $('wbtns').insertAdjacentHTML('beforeend', '<button class="cp-x" id="setMin" aria-label="Riduci a icona"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12h14"/></svg></button>');
    $('wbtns').appendChild(close);
    $('setMin').addEventListener('click', () => win.minimize());
    close.addEventListener('click', e => { e.stopImmediatePropagation(); win.close(); }, true);
    addEventListener('keydown', e => { if (e.key === 'Escape' && document.activeElement === document.body) win.close(); });
    listen('settings-page', e => C.setPage(e.payload));
    invoke('take_settings_page').then(p => { if (p) C.setPage(p); });
    const upd = $('infoUpd'), fresh = upd.cloneNode(true);   // removes the fake update check
    upd.replaceWith(fresh);
    fresh.style.cursor = 'default';
  }

  // ---------- real data: system, music, weather, calendar ----------
  const storedDisks = () => { try { return JSON.parse(store.get('glyphling-disks', 'null')); } catch (e) { return null; } };
  listen('pc', e => {
    C.setPc(e.payload);
    const want = storedDisks();
    if (want && want.join() !== C.shownDisks().join()) C.setShownDisks(want);
    if (host === 'settings') cpuTempStatus(e.payload);
  });
  listen('disks-changed', e => { if (e.payload.from !== host) { const w = storedDisks(); if (w) C.setShownDisks(w); } });

  // the cover becomes 19x19 LEDs: 4 brightness levels, the most saturated pixels as accent
  function coverToLeds(url) {
    return new Promise(res => {
      const img = new Image();
      img.onload = () => {
        const N = 19, c = (N - 1) / 2, cv = document.createElement('canvas');
        cv.width = cv.height = N;
        const g = cv.getContext('2d');
        g.drawImage(img, 0, 0, N, N);
        const px = g.getImageData(0, 0, N, N).data, cells = [];
        for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
          const i = (y * N + x) * 4, r = px[i] / 255, gg = px[i + 1] / 255, b = px[i + 2] / 255;
          const mx = Math.max(r, gg, b), mn = Math.min(r, gg, b);
          cells.push({ x, y, l: 0.2126 * r + 0.7152 * gg + 0.0722 * b, s: mx ? (mx - mn) / mx * mx : 0 });
        }
        const inRing = p => { const d = Math.hypot(p.x - c, p.y - c); return d <= c + 0.3 && d >= 1.6; };
        const sats = cells.filter(inRing).map(p => p.s).sort((a, b) => b - a);
        const cut = Math.max(0.35, sats[Math.floor(sats.length * 0.04)] || 1);
        const out = Array.from({ length: N }, () => Array(N).fill(0));
        for (const p of cells) {
          if (!inRing(p)) continue;
          out[p.y][p.x] = p.s >= cut ? -1 : p.l > 0.75 ? 1 : p.l > 0.5 ? 0.7 : p.l > 0.25 ? 0.35 : 0;
        }
        res(out);
      };
      img.onerror = () => res(null);
      img.src = url;
    });
  }
  if (host === 'widget') {
    listen('media', async e => {
      const m = e.payload;
      if (m.cover_changed) C.setCover(m.cover ? await coverToLeds(m.cover) : null);
      C.setMedia(m);
    });
    listen('spectrum', e => { window.__liveBands = { v: e.payload, at: performance.now() }; });
    $('play').addEventListener('click', () => invoke('media_control', { action: 'toggle' }), true);
    for (const [id, action] of [['next', 'next'], ['prev', 'prev']]) {
      $(id).addEventListener('click', ev => { ev.stopImmediatePropagation(); invoke('media_control', { action }); }, true);
    }
  }

  // weather from Open-Meteo, no API key
  const MILANO = { name: 'Milano', lat: 45.4642, lon: 9.19 };
  const city = () => { try { return JSON.parse(store.get('glyphling-city', 'null')) || MILANO; } catch (e) { return MILANO; } };
  function wmo(code, day) {
    const T = { 0: 'Sereno', 1: 'Poco nuvoloso', 2: 'Parz. nuvoloso', 3: 'Coperto', 45: 'Nebbia', 48: 'Nebbia', 51: 'Pioviggine', 53: 'Pioviggine', 55: 'Pioviggine fitta',
      56: 'Pioviggine gelata', 57: 'Pioviggine gelata', 61: 'Pioggia leggera', 63: 'Pioggia', 65: 'Pioggia forte', 66: 'Pioggia gelata', 67: 'Pioggia gelata',
      71: 'Neve debole', 73: 'Neve', 75: 'Neve forte', 77: 'Nevischio', 80: 'Rovesci', 81: 'Rovesci', 82: 'Rovesci forti', 85: 'Rovesci di neve', 86: 'Rovesci di neve',
      95: 'Temporale', 96: 'Temporale e grandine', 99: 'Temporale e grandine' };
    const key = code <= 1 ? (day ? 'sun' : 'moon') : code <= 48 || (code >= 71 && code <= 77) || code === 85 || code === 86 ? 'cloud' : 'rain';
    return { key, cond: T[code] || WXC_FALLBACK[key] };
  }
  const WXC_FALLBACK = { sun: 'Sereno', moon: 'Sereno', cloud: 'Nuvoloso', rain: 'Pioggia' };
  const p2 = n => String(n).padStart(2, '0');
  const DSHORT = ['dom', 'lun', 'mar', 'mer', 'gio', 'ven', 'sab'];
  const DAYS = ['domenica', 'lunedì', 'martedì', 'mercoledì', 'giovedì', 'venerdì', 'sabato'];
  async function loadWeather() {
    const c = city();
    const url = `https://api.open-meteo.com/v1/forecast?latitude=${c.lat}&longitude=${c.lon}&timezone=auto&forecast_days=7`
      + '&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m,is_day'
      + '&hourly=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m,is_day'
      + '&daily=weather_code,temperature_2m_max,temperature_2m_min,relative_humidity_2m_mean,wind_speed_10m_max';
    const j = JSON.parse(await invoke('fetch_text', { url }));
    const cur = j.current, H = j.hourly, D = j.daily;
    const today = cur.time.slice(0, 10);
    let i0 = H.time.findIndex(t => t.slice(0, 13) === cur.time.slice(0, 13));
    if (i0 < 0) i0 = 0;
    const hours = [];
    for (let i = i0; i < Math.min(H.time.length, i0 + 24); i++) {
      const now = i === i0, t = H.time[i], h = +t.slice(11, 13);
      const w = now ? wmo(cur.weather_code, cur.is_day) : wmo(H.weather_code[i], H.is_day[i]);
      hours.push({
        label: now ? 'ora' : `${h % 12 || 12}<small>${h < 12 ? 'AM' : 'PM'}</small>`, when: now ? 'adesso' : `${t.slice(0, 10) === today ? 'oggi' : 'domani'} alle ${p2(h)}:00`,
        key: w.key, cond: w.cond, temp: Math.round(now ? cur.temperature_2m : H.temperature_2m[i]),
        hum: Math.round(now ? cur.relative_humidity_2m : H.relative_humidity_2m[i]), wind: Math.round(now ? cur.wind_speed_10m : H.wind_speed_10m[i]), range: null,
      });
    }
    const days = D.time.map((t, i) => {
      const d = new Date(t + 'T12:00'), w = wmo(D.weather_code[i], 1);
      const max = Math.round(D.temperature_2m_max[i]), min = Math.round(D.temperature_2m_min[i]);
      const hum = D.relative_humidity_2m_mean ? Math.round(D.relative_humidity_2m_mean[i]) : null;
      return { label: i === 0 ? 'oggi' : DSHORT[d.getDay()], date: d.getDate(), when: i === 0 ? 'oggi' : i === 1 ? 'domani' : `${DAYS[d.getDay()]} ${d.getDate()}`,
        key: w.key, cond: w.cond, temp: max, min, max, hum, wind: Math.round(D.wind_speed_10m_max[i]) };
    });
    C.setWeather({ hours, days, city: c.name, lat: c.lat, lon: c.lon });
  }

  // calendar: iCal URLs (for Google, the "secret address in iCal format")
  const icsUrls = () => store.get('glyphling-ics', '').split(/\s+/).filter(u => u.startsWith('https://'));
  function tzOffset(tz, utcMs) {
    try {
      const f = new Intl.DateTimeFormat('en-US', { timeZone: tz, hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
      const p = Object.fromEntries(f.formatToParts(new Date(utcMs)).map(x => [x.type, x.value]));
      return Date.UTC(+p.year, p.month - 1, +p.day, +p.hour, +p.minute, +p.second) - utcMs;
    } catch (e) { return null; }
  }
  // "20261019T143000Z", "20261019T143000" (+TZID), "20261019" (all day)
  function icsDate(v, params) {
    const m = /^(\d{4})(\d{2})(\d{2})(?:T(\d{2})(\d{2})(\d{2})?(Z)?)?/.exec(v);
    if (!m) return null;
    const [, Y, Mo, Dd, h, mi, se, z] = m;
    if (h == null || /VALUE=DATE(?!-)/.test(params)) return { d: new Date(+Y, Mo - 1, +Dd), allDay: true };
    if (z) return { d: new Date(Date.UTC(+Y, Mo - 1, +Dd, +h, +mi, +(se || 0))), allDay: false };
    const tz = (/TZID=([^;:]+)/.exec(params) || [])[1];
    if (tz) {
      const guess = Date.UTC(+Y, Mo - 1, +Dd, +h, +mi, +(se || 0)), off = tzOffset(tz, guess);
      if (off != null) return { d: new Date(guess - tzOffset(tz, guess - off)), allDay: false };
    }
    return { d: new Date(+Y, Mo - 1, +Dd, +h, +mi, +(se || 0)), allDay: false };
  }
  function parseIcs(text) {
    const lines = text.replace(/\r?\n[ \t]/g, '').split(/\r?\n/), evs = [];
    let ev = null;
    for (const ln of lines) {
      if (ln === 'BEGIN:VEVENT') { ev = { ex: [] }; continue; }
      if (ln === 'END:VEVENT') { if (ev && ev.start) evs.push(ev); ev = null; continue; }
      if (!ev) continue;
      const c = ln.indexOf(':'); if (c < 0) continue;
      const head = ln.slice(0, c), val = ln.slice(c + 1), name = head.split(';')[0], params = head.slice(name.length);
      if (name === 'DTSTART') ev.start = icsDate(val, params);
      else if (name === 'DTEND') ev.end = icsDate(val, params);
      else if (name === 'SUMMARY') ev.title = val.replace(/\\([,;\\])/g, '$1').replace(/\\n/gi, ' ');
      else if (name === 'RRULE') ev.rrule = Object.fromEntries(val.split(';').map(kv => kv.split('=')));
      else if (name === 'EXDATE') for (const v of val.split(',')) { const d = icsDate(v, params); if (d) ev.ex.push(d.d.getTime()); }
      else if (name === 'RECURRENCE-ID') ev.recId = icsDate(val, params);
      else if (name === 'UID') ev.uid = val;
      else if (name === 'STATUS') ev.status = val;
    }
    return evs.filter(e => e.status !== 'CANCELLED' || e.recId);
  }
  const BYDAY = { SU: 0, MO: 1, TU: 2, WE: 3, TH: 4, FR: 5, SA: 6 };
  // all occurrences between from and to (Date)
  function occurrences(ev, from, to, skip) {
    const s = ev.start.d, out = [];
    const ok = d => d >= from && d <= to && !ev.ex.includes(d.getTime()) && !skip.has(d.getTime());
    const r = ev.rrule;
    if (!r) { if (ok(s) || (ev.end && s < from && ev.end.d > from)) out.push(s); return out; }
    const iv = +(r.INTERVAL || 1), count = r.COUNT ? +r.COUNT : Infinity;
    const until = r.UNTIL ? icsDate(r.UNTIL, '').d : null;
    const end = until && until < to ? until : to;
    const at = (y, mo, d) => new Date(y, mo, d, s.getHours(), s.getMinutes(), s.getSeconds());
    let n = 0;
    const push = d => { if (d < s || n >= count) return; n++; if (ok(d) && d <= end) out.push(d); };
    const days = r.BYDAY ? r.BYDAY.split(',') : null;
    for (let k = 0; n < count && k < 5000; k++) {
      let base;
      if (r.FREQ === 'DAILY') { base = at(s.getFullYear(), s.getMonth(), s.getDate() + k * iv); if (base > end) break; push(base); }
      else if (r.FREQ === 'WEEKLY') {
        const wk = at(s.getFullYear(), s.getMonth(), s.getDate() - s.getDay() + k * 7 * iv);
        if (wk > end) break;
        const list = days ? days.map(x => BYDAY[x.slice(-2)]).sort() : [s.getDay()];
        for (const wd of list) push(at(wk.getFullYear(), wk.getMonth(), wk.getDate() + wd));
      } else if (r.FREQ === 'MONTHLY') {
        const y = s.getFullYear(), mo = s.getMonth() + k * iv, first = new Date(y, mo, 1);
        if (first > end) break;
        if (days && /^-?\d/.test(days[0])) {
          const nth = parseInt(days[0], 10), wd = BYDAY[days[0].slice(-2)];
          if (nth > 0) push(at(y, mo, 1 + (wd - first.getDay() + 7) % 7 + (nth - 1) * 7));
          else { const last = new Date(y, mo + 1, 0); push(at(y, mo, last.getDate() - (last.getDay() - wd + 7) % 7 + (nth + 1) * 7)); }
        } else {
          const day = r.BYMONTHDAY ? +r.BYMONTHDAY : s.getDate(), d = at(y, mo, day);
          if (d.getDate() === day) push(d);
        }
      } else if (r.FREQ === 'YEARLY') {
        const d = at(s.getFullYear() + k * iv, s.getMonth(), s.getDate());
        if (d > end) break;
        if (d.getDate() === s.getDate()) push(d);
      } else break;
    }
    return out;
  }
  const EV_COLORS = ['accent', '#4d7cff', '#2ee6a6', '#ffcc33', '#b04dff', '#ff8a65'];
  async function loadCalendar() {
    const urls = icsUrls();
    if (!urls.length) { C.setEvents(() => [], 'Non collegato'); return; }
    const byDay = new Map(), now = new Date();
    const from = new Date(now.getFullYear(), now.getMonth() - 3, 1), to = new Date(now.getFullYear() + 1, now.getMonth() + 1, 1);
    const add = (d, item) => { const k = d.toDateString(); if (!byDay.has(k)) byDay.set(k, []); byDay.get(k).push(item); };
    let failed = 0;
    for (const [ci, url] of urls.entries()) {
      let evs;
      try { evs = parseIcs(await invoke('fetch_text', { url })); } catch (e) { failed++; continue; }
      const color = EV_COLORS[ci % EV_COLORS.length];
      const moved = new Map();   // moved or cancelled occurrences: the series skips them
      for (const e of evs) if (e.recId) { if (!moved.has(e.uid)) moved.set(e.uid, new Set()); moved.get(e.uid).add(e.recId.d.getTime()); }
      for (const e of evs) {
        if (e.status === 'CANCELLED') continue;
        const title = e.title || '(senza titolo)';
        for (const d of occurrences(e, from, to, (!e.recId && moved.get(e.uid)) || new Set())) {
          if (e.start.allDay) {
            const len = e.end ? Math.max(1, Math.round((e.end.d - e.start.d) / 86400000)) : 1;
            for (let i = 0; i < len; i++) add(new Date(d.getFullYear(), d.getMonth(), d.getDate() + i), ['giorno', title, color]);
          } else add(d, [`${p2(d.getHours())}:${p2(d.getMinutes())}`, title, color]);
        }
      }
    }
    for (const list of byDay.values()) list.sort((a, b) => (a[0] === b[0] ? 0 : a[0] === 'giorno' ? -1 : b[0] === 'giorno' ? 1 : a[0] > b[0] ? 1 : -1));
    C.setEvents(d => byDay.get(d.toDateString()) || [], failed === urls.length ? 'Non raggiungibile' : 'Google');
  }

  if (host === 'widget') {
    const safe = f => () => f().catch(err => console.warn(err));
    const wxNow = safe(loadWeather), calNow = safe(loadCalendar);
    wxNow(); calNow();
    setInterval(wxNow, 30 * 60 * 1000);
    setInterval(calNow, 10 * 60 * 1000);
    listen('data-changed', e => { if (e.payload === 'city') wxNow(); else calNow(); });
  }

  // ---------- Settings: Data group ----------
  let cpuTempEl = null;
  function cpuTempStatus(s) {
    if (!cpuTempEl) return;
    const SRC = { hwinfo: 'da HWiNFO', lhm: 'da LibreHardwareMonitor', acpi: 'da Windows, approssimativa' };
    cpuTempEl.textContent = s.tc != null ? `${Math.round(s.tc)}° ${SRC[s.tc_source] || ''}` : 'Non disponibile';
  }
  if (host === 'settings') {
    const after = $('swAuto') ? $('swAuto').closest('.sgroup') : $('posReset').closest('.sgroup');
    after.insertAdjacentHTML('afterend', `<div class="sgroup" id="dataGroup"><h3>Dati</h3>
      <div class="srow"><div><b>Città del meteo</b><small id="cityNow"></small></div>
        <div class="dfield"><input id="cityQ" placeholder="Cerca una città" aria-label="Cerca una città"><button class="ghost" id="cityGo">Cerca</button></div></div>
      <div class="dres" id="cityRes" hidden></div>
      <div class="srow col"><div><b>Calendario Google</b><small>In Google Calendar apri Impostazioni, scegli il calendario e copia l'<b>indirizzo segreto in formato iCal</b> da "Integra calendario". Uno per riga se sono più di uno. Resta salvato solo su questo PC.</small></div>
        <textarea id="icsIn" rows="2" spellcheck="false" placeholder="https://calendar.google.com/calendar/ical/…/basic.ics" aria-label="Indirizzi iCal"></textarea>
        <div class="drow"><small id="icsMsg"></small><button class="ghost" id="icsSave">Salva</button></div></div>
      <div class="srow"><div><b>Temperatura della CPU</b><small>Senza programmi esterni Glyphling usa la zona termica di Windows, che su molti PC fissi non è la CPU. Per il valore preciso apri <b>HWiNFO</b> con Impostazioni › Shared Memory Support attivo, oppure <b>LibreHardwareMonitor</b> con Opzioni › Remote Web Server (porta 8085). La GPU NVIDIA si legge da sola.</small></div><span class="dstat" id="cpuTemp">Si aggiorna con la modalità Sistema</span></div>
    </div>`);
    cpuTempEl = $('cpuTemp');
    // the real disks for the dial picker, even if the widget is not in System mode
    invoke('list_disks').then(disks => {
      if (!disks.length) return;
      C.setPc({ cpu: 0, ram: 0, ram_used: 0, ram_total: 0, gpu: null, tc: null, tg: null, up_min: 0, disks });
      const w = storedDisks(); if (w) C.setShownDisks(w);
    });
    const showCity = () => { $('cityNow').textContent = `Adesso: ${city().name}. Le previsioni arrivano da Open-Meteo.`; };
    showCity();
    const search = async () => {
      const q = $('cityQ').value.trim(), res = $('cityRes');
      if (!q) return;
      res.hidden = false; res.textContent = 'Cerco…';
      try {
        const j = JSON.parse(await invoke('fetch_text', { url: `https://geocoding-api.open-meteo.com/v1/search?count=5&language=it&name=${encodeURIComponent(q)}` }));
        const list = j.results || [];
        res.innerHTML = list.length ? '' : 'Nessuna città trovata.';
        for (const r of list) {
          const b = document.createElement('button');
          b.className = 'ghost';
          b.textContent = [r.name, r.admin1, r.country].filter(Boolean).join(', ');
          b.addEventListener('click', () => {
            store.set('glyphling-city', JSON.stringify({ name: r.name, lat: r.latitude, lon: r.longitude }));
            emit('data-changed', 'city'); showCity(); res.hidden = true; $('cityQ').value = '';
          });
          res.appendChild(b);
        }
      } catch (e) { res.textContent = 'Ricerca non riuscita. Controlla la connessione.'; }
    };
    $('cityGo').addEventListener('click', search);
    $('cityQ').addEventListener('keydown', e => { if (e.key === 'Enter') search(); });
    $('icsIn').value = store.get('glyphling-ics', '');
    $('icsSave').addEventListener('click', async () => {
      const urls = $('icsIn').value.split(/\s+/).filter(Boolean), msg = $('icsMsg');
      if (urls.some(u => !u.startsWith('https://'))) { msg.textContent = 'Serve un indirizzo che inizia con https://'; return; }
      msg.textContent = urls.length ? 'Controllo…' : '';
      let n = 0;
      for (const u of urls) { try { n += parseIcs(await invoke('fetch_text', { url: u })).length; } catch (e) { msg.textContent = 'Non riesco a leggere uno degli indirizzi.'; return; } }
      store.set('glyphling-ics', urls.join('\n'));
      emit('data-changed', 'calendar');
      msg.textContent = urls.length ? `Collegato: ${n} impegni trovati.` : 'Calendario scollegato.';
    });
  }

  // ---------- Info with real data ----------
  let newsAdded = false;
  async function fillInfo() {
    if (host !== 'settings') return;
    const i = await invoke('app_info');
    $('infoOs').textContent = i.os;
    $('infoPos').textContent = i.position;
    const ver = document.querySelector('.about .ver');
    if (ver) ver.innerHTML = `<span>Versione <b>${i.version}</b></span><span>Canale <b>prova</b></span><span id="infoUpd">Aggiornamenti <b>non ancora attivi</b></span>`;
    if (!newsAdded) {
      newsAdded = true;
      const lists = document.querySelectorAll('.spage[data-p="info"] .news');
      if (lists[0]) lists[0].insertAdjacentHTML('afterbegin', `<li><span><b>${i.version}</b> · ghiera per scegliere la modalità, riquadro a destra o a sinistra, menu dell'icona e impostazioni su misura, il widget resta visibile con Mostra desktop.</span></li><li><span><b>0.2</b> · dati veri: musica, meteo, calendario Google, CPU, GPU, RAM, temperature e dischi. Font a punti.</span></li>`);
      if (lists[1]) lists[1].insertAdjacentHTML('afterbegin', `<li><span>Font a punti <b>Glyphling Dot</b>, disegnato per Glyphling (SIL Open Font License). Se sul PC è installato il font 5x7 di Nothing, si usa quello.</span></li>`);
      if (lists[1]) lists[1].insertAdjacentHTML('beforeend', `<li><span>Meteo da <b>Open-Meteo</b> (CC BY 4.0). Temperatura della CPU da <b>HWiNFO</b> o <b>LibreHardwareMonitor</b> se sono aperti, altrimenti dalla zona termica di Windows.</span></li>`);
      if (lists[1]) lists[1].insertAdjacentHTML('beforeend', `<li><span>Costruito con <b>Tauri</b> (licenza MIT/Apache 2.0). Impostazioni salvate in <b>${i.config_dir}</b>.</span></li>`);
    }
  }
})();
