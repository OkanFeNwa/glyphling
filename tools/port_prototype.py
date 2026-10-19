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
sub("  function applyPos() {", "  function applyPos() { if (window.__host) return;")
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
sub("$('pcTc').textContent = Math.round(pc.tc) + '°';", "$('pcTc').textContent = pc.tc == null ? '--' : Math.round(pc.tc) + '°';")
sub("$('pcTg').textContent = Math.round(pc.tg) + '°';", "$('pcTg').textContent = pc.tg == null ? '--' : Math.round(pc.tg) + '°';")
sub("· 9,8 di 16 GB`", "· ${pc.ramText || '9,8 di 16 GB'}`")
sub("    else if (dkShown.length < 4) dkShown = DISKS.map(d => d.lt).filter(x => dkShown.includes(x) || x === lt);\n    dkUi();\n",
    "    else if (dkShown.length < 4) dkShown = DISKS.map(d => d.lt).filter(x => dkShown.includes(x) || x === lt);\n    dkUi();\n    if (window.__hostEvent) window.__hostEvent('disks', dkShown);\n")
sub("    vol = Math.max(0, Math.min(1, v));\n", "    vol = Math.max(0, Math.min(1, v));\n    if (window.__hostEvent) window.__hostEvent('volume', vol);\n")
sub("      bands[x] += (target - bands[x]) * Math.min(1, dt * (target > bands[x] ? 18 : 6));",
    "      const lv = window.__liveBands, tg = on && lv && performance.now() - lv.at < 400 ? lv.v[x] * (0.35 + vol * 0.65) : target;\n      bands[x] += (tg - bands[x]) * Math.min(1, dt * (tg > bands[x] ? 18 : 6));")
sub("    pos = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * tracks[ti].dur;\n",
    "    pos = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * tracks[ti].dur;\n    if (window.__hostEvent) window.__hostEvent('seek', pos);\n")
sub("$('artist').textContent = `${tr.artist} · ${tr.album}`;", "$('artist').textContent = tr.album ? `${tr.artist} · ${tr.album}` : tr.artist;")
sub("$('wxCond').textContent = WXC[it.key].cond;", "$('wxCond').textContent = it.cond || WXC[it.key].cond;")
sub("  const LAT = 45.4642, LON = 9.19,", "  let LAT = 45.4642, LON = 9.19;\n  const")
sub("  function eventsFor(d) {\n", "  function eventsFor(d) {\n    if (window.__liveEvents) return window.__liveEvents(d);\n")

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
        ti = 0; pos = 0; window.__companion._cover = null;
        if (playing) { playing = false; setPlayIcon(); syncDisc(); }
        loadTrack(); $('upNext').textContent = ''; if (srcEl) srcEl.childNodes[1].nodeValue = 'Musica';
        return;
      }
      const old = tracks[0];
      const changed = !old || !old.live || old.title !== m.title || old.artist !== m.artist;
      tracks.splice(0, tracks.length, { title: m.title || '—', artist: m.artist || m.app, album: m.album, dur: Math.max(1, m.duration || 1), seed: 7, live: true });
      ti = 0; pos = Math.min(m.position || 0, tracks[0].dur);
      if (playing !== m.playing) { playing = m.playing; setPlayIcon(); syncDisc(); }
      if (changed) { loadTrack(); $('tDur').textContent = m.duration ? fmt(m.duration) : '—'; }
      $('upNext').textContent = '';
      if (srcEl) srcEl.childNodes[1].nodeValue = m.app || 'Musica';
      if (m.volume != null && volShow <= 0) { vol = m.volume; $('volTxt').textContent = `vol ${Math.round(vol * 100)}%`; }
      const cv = window.__companion._cover;
      if (cv) cover = cv.map(r => Float32Array.from(r)); else if (changed) drawCover(7);
    },
    setCover(levels) { window.__companion._cover = levels; if (levels) cover = levels.map(r => Float32Array.from(r)); else drawCover(7); },
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

# 6. app name: Glyphling (visible text only, internal keys stay lowercase)
# in the app the panel side is decided by the backend, which knows the screen
html, n = re.subn(r"(  function applyDir\([a-z]*\) \{\n)", r"\1    if (window.__host) return;\n", html, count=1)
if not n:
    sys.exit("hook not found: function applyDir")
html = html.replace("Companion", "Glyphling")
html = html.replace(" Meteo e brani in questo prototipo sono finti.", "")

# 7. host.css and host.js at the end of the page
sub("</body></html>", '<link rel="stylesheet" href="host.css">\n<script src="host.js"></script>\n</body></html>')

open(dst, "w", encoding="utf-8").write(html)
print(f"wrote {dst} ({len(html)} bytes)")
