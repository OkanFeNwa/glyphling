#!/usr/bin/env python3
"""Ports the HTML prototype (Artifact) into the Tauri app.

Usage: python3 tools/port_prototype.py prototipo.html src/index.html

The prototype stays the source of the interface: this script copies it and applies
a few targeted changes (local fonts, hooks for host.js). If a hook can no longer
be found, the script stops and says which one, so it can be updated here.
"""
import re
import sys

src, dst = sys.argv[1], sys.argv[2]
html = open(src, encoding="utf-8").read()


def sub(old, new, count=1):
    global html
    if old not in html:
        sys.exit(f"hook not found: {old[:70]!r}")
    html = html.replace(old, new, count)


# 1. local fonts instead of Google Fonts (the app must work offline)
html, n = re.subn(r'<link rel="(?:preconnect|stylesheet)" href="https://fonts\.(?:googleapis|gstatic)\.com[^>]*>\n?', "", html)
if n < 1:
    sys.exit("Google Fonts links not found")
sub("<style>\n/* Layout:", '<link rel="stylesheet" href="fonts.css">\n<script src="host-pre.js"></script>\n<style>\n/* Layout:')

# 2. mode changes are reported to the host
sub("  function setMode(m) {\n    if (m === mode) return;\n    mode = m;\n",
    "  function setMode(m) {\n    if (m === mode) return;\n    mode = m;\n    if (window.__hostEvent) queueMicrotask(() => window.__hostEvent('mode', m));\n")

# 3. saved themes are reported to the other window
sub("  function saveThemes() {\n",
    "  function saveThemes() {\n    if (window.__hostEvent) queueMicrotask(() => window.__hostEvent('themes'));\n")

# 4. moving: in the app the window is dragged by the native Windows handler
# the circle's place and the panel side are decided by the backend, which knows the screen
sub("  function layout() {\n", "  function layout() {\n    if (window.__host) return;\n")
sub("    if (!moving) return;\n    e.preventDefault(); e.stopPropagation();\n",
    "    if (!moving) return;\n    e.preventDefault(); e.stopPropagation();\n    if (window.__hostDrag) { window.__hostDrag(e); return; }\n")
sub("  function setMoving(v) {\n    moving = v;\n",
    "  function setMoving(v) {\n    if (window.__hostEvent && v !== moving) queueMicrotask(() => window.__hostEvent('move', v));\n    moving = v;\n")
sub("  function setSnap(v) {\n",
    "  function setSnap(v) {\n    if (window.__hostEvent && v !== snap) queueMicrotask(() => window.__hostEvent('snap', v));\n")
sub("  function infoUi() {\n", "  function infoUi() {\n    if (window.__hostEvent) queueMicrotask(() => window.__hostEvent('info'));\n")

# 5a. real data instead of fake data (host.js provides it through window.__companion)
sub("  const pc = { cpu", "  let pcLive = false;\n  const pc = { cpu")
sub("  function updatePc(dt) {\n", "  function updatePc(dt) {\n    if (pcLive) return;\n")
sub("r(pc.gpu);", "(pc.gpuNa ? '--' : r(pc.gpu));")
sub("$('pcTc').textContent = tC(pc.tc) + '°';", "$('pcTc').textContent = pc.tc == null ? '--' : tC(pc.tc) + '°';")
sub("$('pcTg').textContent = tC(pc.tg) + '°';", "$('pcTg').textContent = pc.tg == null ? '--' : tC(pc.tg) + '°';")
sub("· 9,8 di 16 GB`", "· ${pc.ramText || '9,8 di 16 GB'}`")
sub("    else if (dkShown.length < 4) dkShown = DISKS.map(d => d.lt).filter(x => dkShown.includes(x) || x === lt);\n    dkUi();\n",
    "    else if (dkShown.length < 4) dkShown = DISKS.map(d => d.lt).filter(x => dkShown.includes(x) || x === lt);\n    dkUi();\n    if (window.__hostEvent) window.__hostEvent('disks', dkShown);\n")
sub("    vol = Math.max(0, Math.min(1, Math.round(v * 100) / 100));\n", "    vol = Math.max(0, Math.min(1, Math.round(v * 100) / 100));\n    if (window.__hostEvent) window.__hostEvent('volume', vol);\n")
sub("      bands[x] += (target - bands[x]) * Math.min(1, dt * (target > bands[x] ? 18 : 6));",
    "      const lv = window.__liveBands, tg = on && lv && performance.now() - lv.at < 400 ? lv.v[x] * (0.35 + vol * 0.65) : target;\n      bands[x] += (tg - bands[x]) * Math.min(1, dt * (tg > bands[x] ? 18 : 6));")
sub("    pos = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * tracks[ti].dur;\n",
    "    pos = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * tracks[ti].dur;\n    if (window.__hostEvent) window.__hostEvent('seek', pos);\n")
sub("$('artist').textContent = `${tr.artist} · ${tr.album}`;", "$('artist').textContent = tr.album ? `${tr.artist} · ${tr.album}` : tr.artist;")
sub("$('wxCond').textContent = WXC[it.key].cond;", "$('wxCond').textContent = it.cond || WXC[it.key].cond;")
sub("  const LAT = 45.4642, LON = 9.19,", "  let LAT = 45.4642, LON = 9.19;\n  const")
sub("  function eventsFor(d) {\n", "  function eventsFor(d) {\n"
    "    if (window.__liveEvents) {\n"
    "      const list = window.__liveEvents(d).slice();\n"
    "      for (const u of userEv) if (evOn(u, d)) list.push([u.all ? 'tutto il dì' : u.time, u.title, u.color, u]);\n"
    "      return list.sort((a, b) => (a[0] > b[0] ? 1 : -1));\n"
    "    }\n")
# the event editor needs room: in the app the widget window grows over the screen while it is open
sub("  function openEvPop(day, ax, ay) {\n    calSel = day; calUi();\n",
    "  async function openEvPop(day, ax, ay) {\n"
    "    if (window.__host === 'widget' && window.__hostTut && !window.__hostTutOn) {\n"
    "      await window.__hostTut(true);\n"
    "      const o = window.__hostTutOff || [0, 0]; ax += o[0]; ay += o[1];\n"
    "    }\n"
    "    calSel = day; calUi();\n")
sub("  function closeEvPop() { epEl.hidden = true; }", "  function closeEvPop() {\n    epEl.hidden = true;\n    if (window.__host === 'widget' && window.__hostTutOn && !window.__hostTutRun) window.__hostTut(false);\n  }")


# 5. functions exposed to host.js
API = """
  // ---------- agganci per l'app (aggiunti da tools/port_prototype.py) ----------
  window.__companion = {
    setMode, getMode: () => mode, setMoving, setSnap, setPage,
    isMoving: () => moving, isSnap: () => snap,
    toggleSec() { showSec = !showSec; syncCtx(); },
    toggleSleep() { if (mode !== 'idle') setMode('idle'); asleep ? wake() : fallAsleep(); },
    reloadThemes() {
      try {
        const raw = JSON.parse(localStorage.getItem('companion-themes') || 'null');
        if (raw && Array.isArray(raw.user)) {
          ts.user = raw.user.filter(t => t && typeof t.id === 'string' && KEYS.every(k => /^#[0-9a-f]{6}$/i.test(t[k] || ''))).map(t => ({ id: t.id, name: String(t.name || 'Tema').slice(0, 14), ...pick(t), user: true }));
          if (typeof raw.active === 'string') ts.active = raw.active;
        }
      } catch (e) {}
      if (!findTheme(ts.active)) ts.active = 'b0';
      applyColors(findTheme(ts.active)); renderThemes();
    },
    look(x, y) {
      const r = ring.getBoundingClientRect();
      const dx = x - (r.left + r.width / 2), dy = y - (r.top + r.height / 2);
      const d = Math.hypot(dx, dy) || 1, k = Math.min(1, d / 260);
      eye.mouse = { x: dx / d * 3 * k, y: dy / d * 2 * k, at: performance.now() };
      wake();
    },
    setPc(s) {
      pcLive = true;
      pc.cpu = s.cpu; pc.ram = s.ram; pc.gpu = s.gpu == null ? 0 : s.gpu; pc.gpuNa = s.gpu == null;
      pc.tc = s.tc; pc.tg = s.tg; pc.upMin = s.up_min;
      pc.ramText = `${s.ram_used.toFixed(1).replace('.', ',')} di ${Math.round(s.ram_total)} GB`;
      if (s.disks && s.disks.length) {
        const key = s.disks.map(d => d.lt + Math.round(d.used)).join();
        if (key !== pc.dkKey) {
          pc.dkKey = key;
          DISKS.splice(0, DISKS.length, ...s.disks.map(d => ({ lt: d.lt, name: d.name, used: Math.round(d.used), size: Math.round(d.size) })));
          dkKey = '';   // the list of buttons is rebuilt only when this changes: names are new now
          const lts = DISKS.map(d => d.lt);
          dkShown = dkShown.filter(l => lts.includes(l));
          if (!dkShown.length) dkShown = lts.slice(0, 4);
          if (!lts.includes(dkSel)) dkSel = dkShown[0];
          dkUi();
        }
      }
      if (mode === 'disks') pcUi();
    },
    setShownDisks(list) {
      const lts = DISKS.map(d => d.lt);
      const ok = (list || []).filter(l => lts.includes(l)).slice(0, 4);
      if (ok.length) { dkShown = ok; if (!ok.includes(dkSel)) dkSel = ok[0]; dkUi(); }
    },
    shownDisks: () => dkShown.slice(),
    setMedia(m) {
      const srcEl = document.querySelector('.view[data-v="music"] .src');
      if (!m.active) {
        tracks.splice(0, tracks.length, { title: 'Niente in riproduzione', artist: 'Apri Spotify o un altro lettore', album: '', dur: 1, seed: 5, live: true });
        ti = 0; pos = 0; window.__companion._seed = null;
        if (playing) { playing = false; setPlayIcon(); syncDisc(); }
        loadTrack(); $('upNext').textContent = ''; if (srcEl) srcEl.childNodes[1].nodeValue = 'Musica';
        return;
      }
      const old = tracks[0];
      const changed = !old || !old.live || old.title !== m.title || old.artist !== m.artist;
      tracks.splice(0, tracks.length, { title: m.title || '—', artist: m.artist || m.app, album: m.album, dur: Math.max(1, m.duration || 1), seed: window.__companion._seed || 7, live: true });
      ti = 0; pos = Math.min(m.position || 0, tracks[0].dur);
      if (playing !== m.playing) { playing = m.playing; setPlayIcon(); syncDisc(); }
      if (changed) { loadTrack(); $('tDur').textContent = m.duration ? fmt(m.duration) : '—'; }
      else if (coverSeed !== tracks[0].seed) { trk.from = coverSeed; trk.t = 1; drawCover(tracks[0].seed); }
      $('upNext').textContent = '';
      if (srcEl) srcEl.childNodes[1].nodeValue = m.app || 'Musica';
      if (m.volume != null && volShow <= 0) { vol = m.volume; $('volTxt').textContent = `vol ${Math.round(vol * 100)}%`; }
    },
    // real cover: a function that renders it at the circle's resolution (NH x NH values);
    // it takes effect with the next setMedia, with the page's cover sweep
    setCover(fn) {
      const all = window.__covers = window.__covers || {};
      if (!fn) { window.__companion._seed = null; return; }
      all['r' + fn.id] = fn;
      window.__companion._seed = 'r' + fn.id;
      for (const k of Object.keys(all).slice(0, -3)) delete all[k];   // keep the last few for the sweep
    },
    // the format and resolution chosen in the settings window
    reloadFmt() {
      try { Object.assign(units, JSON.parse(localStorage.getItem('companion-units') || '{}')); } catch (e) {}
      const r = +localStorage.getItem('companion-res');
      if (RES[r] && r !== resPts) setRes(r);
      calKey = ''; clockUi(new Date()); wxUi(); cdUi(); pcUi(); infoUi(); fmtUi();
    },
    setWeather(w) {
      FC.hours = w.hours; FC.days = w.days;
      wx.i = Math.min(wx.i, FC[wx.scale].length - 1);
      if (w.lat != null) { LAT = w.lat; LON = w.lon; calKey = ''; }
      const srcEl = document.querySelector('.view[data-v="weather"] .src');
      if (srcEl && w.city) srcEl.childNodes[1].nodeValue = w.city + ' · ';
      wxUi();
    },
    setLocation(lat, lon) { LAT = lat; LON = lon; calKey = ''; },
    setEvents(fn, label) {
      window.__liveEvents = fn; evCache.clear(); calKey = '';
      const srcEl = document.querySelector('.view[data-v="calendar"] .src');
      if (srcEl && label) srcEl.childNodes[1].nodeValue = label;
      calUi();
    },
    themeName() { const at = allThemes().find(t => t.id === ts.active); return at ? at.name : 'Nothing'; },
  };
"""
sub("  requestAnimationFrame(frame);\n})();\n</script>", API + "  requestAnimationFrame(frame);\n})();\n</script>")


# 5d. real covers: a seed "r<id>" draws the cover the host registered, at every resolution
sub("      for (let y = 0, i = 0; y < NH; y++) for (let x = 0; x < NH; x++, i++) c[i] = coverArt(seed, LX[x], LX[y]);\n",
    "      const rc = typeof seed === 'string' && window.__covers && window.__covers[seed];\n"
    "      if (rc) c.set(rc(NH)); else for (let y = 0, i = 0; y < NH; y++) for (let x = 0; x < NH; x++, i++) c[i] = coverArt(seed, LX[x], LX[y]);\n")

# the live track has no "up next" line (the animated swap would write it after the host cleared it)
sub("      swapText($('upNext'), `a seguire: ${nx.title}`, 90);", "      swapText($('upNext'), tr.live ? '' : `a seguire: ${nx.title}`, 90);")
sub("      $('upNext').textContent = `a seguire: ${nx.title}`;\n      trackShown = true;", "      $('upNext').textContent = tr.live ? '' : `a seguire: ${nx.title}`;\n      trackShown = true;")

# 5f. cover switch OFF = the cover keeps its own colors: real covers carry an RGB map per cell,
# fed into the prototype's per-LED color buffer (hcol/hcolOn)
sub("  function drawCoverLed() {\n    const cur = coverBuf(coverSeed), old = trk.t > 0 ? coverBuf(trk.from) : null;\n",
    "  function coverRgb(seed) {\n"
    "    const rc = typeof seed === 'string' && window.__covers && window.__covers[seed];\n"
    "    return rc && rc.rgb ? rc.rgb(NH) : null;\n"
    "  }\n"
    "  function drawCoverLed() {\n"
    "    const cur = coverBuf(coverSeed), old = trk.t > 0 ? coverBuf(trk.from) : null;\n"
    "    const curR = units.cover ? null : coverRgb(coverSeed), oldR = old && !units.cover ? coverRgb(trk.from) : null;\n")
sub("      const v = src[sy * NH + sx];\n      if (v === 0) continue;\n",
    "      const v = src[sy * NH + sx];\n"
    "      const srcR = src === cur ? curR : oldR;\n"
    "      if (srcR) {\n"
    "        const j = (sy * NH + sx) * 3;\n"
    "        if (srcR[j] | srcR[j + 1] | srcR[j + 2]) { hset(i, 1); hcol[i * 3] = srcR[j]; hcol[i * 3 + 1] = srcR[j + 1]; hcol[i * 3 + 2] = srcR[j + 2]; hcolOn[i] = 1; continue; }\n"
    "      }\n"
    "      if (v === 0) continue;\n")

# at 2048 points the canvas repaints at 30 fps
sub("    compose();\n    draw();\n", "    compose();\n    if (NH < 51 || (frameNo++ & 1) === 0) draw();\n")
sub("  function frame(nowMs) {\n", "  let frameNo = 0;\n  function frame(nowMs) {\n")

# 5e. format and resolution are shared with the other window
sub("    setRes(+b.dataset.r); try { localStorage.setItem('companion-res', resPts); } catch (e) {}\n",
    "    setRes(+b.dataset.r); try { localStorage.setItem('companion-res', resPts); } catch (e) {}\n    if (window.__hostEvent) window.__hostEvent('fmt');\n")
sub("    try { localStorage.setItem('companion-units', JSON.stringify(units)); } catch (e) {}\n",
    "    try { localStorage.setItem('companion-units', JSON.stringify(units)); } catch (e) {}\n    if (window.__hostEvent) window.__hostEvent('fmt');\n")

# 5c. tutorial: only in the widget window, which first grows to cover the screen;
# the tray icon is the real one in the taskbar
sub("  function tutOpen() {",
    "  window.__tutOpen = () => tutOpen();\n"
    "  function tutOpen() {\n"
    "    if (window.__host && window.__host !== 'widget') return;\n"
    "    window.__hostTutRun = true;\n"
    "    if (window.__hostTut && !window.__hostTutOn) { window.__hostTut(true).then(() => tutOpen()); return; }\n   ")
sub("  function tutClose() {\n", "  function tutClose() {\n    window.__hostTutRun = false;\n    if (window.__hostTut) window.__hostTut(false);\n")
# in the app the first-run check is done by the host with a flag saved by the backend
sub("  if (!tutSeen) setTimeout(tutOpen, 1400);", "  if (!tutSeen && !window.__host) setTimeout(tutOpen, 1400);")
sub("el: () => $('trayIc')", "el: () => (window.__tutTray ? window.__tutTray() : $('trayIc'))", 2)

# 5g. music source: the app lists come from Windows (audio sessions), never from a timer
sub("  const AUDIO_NOW = [['Spotify', true]", "  const AUDIO_NOW = window.__host ? [] : [['Spotify', true]")
sub("    const done = list => { if (list) runSnap = list; b.classList.remove('spin'); srcUi(); };",
    "    const done = list => {\n"
    "      if (list) { runSnap = list; if (window.__host) AUDIO_NOW.splice(0, AUDIO_NOW.length, ...list.map(x => x.slice())); }\n"
    "      b.classList.remove('spin'); srcUi();\n"
    "    };")
sub("  window.__srcRefresh = srcUi;\n", "  window.__srcRefresh = srcUi;\n  window.__srcRunning = () => $('srcRefresh').click();\n")

# 6. app name: Glyphling (visible text only, internal keys stay lowercase)
html = html.replace("Companion", "Glyphling")
html = html.replace(" Meteo e brani in questo prototipo sono finti.", "")

# 7. host.css and host.js at the end of the page
sub("</body></html>", '<link rel="stylesheet" href="host.css">\n<script src="host.js"></script>\n</body></html>')

open(dst, "w", encoding="utf-8").write(html)
print(f"wrote {dst} ({len(html)} bytes)")
