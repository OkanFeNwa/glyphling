# Glyphling

Widget desktop per Windows in stile Nothing OS: una mascotte a LED in un cerchio, con un pannello per orologio, calendario, musica, timer, meteo e stato del PC. Fatto con Tauri 2 (backend Rust, interfaccia web). Progetto completo in `../architettura/progetto-implementazione.md`.

## Versione di prova 0.3

**Novità 0.3**: interfaccia del prototipo 0.13 (ghiera per scegliere la modalità, clic destro per tornare alla mascotte, riquadro a destra, a sinistra o automatico), menu dell'icona e finestra Impostazioni su misura senza cornice di Windows, area di trascinamento uguale alla forma vera, il widget resta visibile dopo Mostra desktop.

### Dalla 0.2

**Funziona davvero**: finestra trasparente senza bordi, icona nell'area di notifica con menu (anche con il clic destro sul widget), finestra Impostazioni separata, spostamento con aggancio alla griglia di 24 px (Alt = libero), livello sul desktop / sempre in primo piano / normale, avvio con Windows, una sola copia attiva, posizione e impostazioni salvate, clic che passano al desktop fuori dal cerchio, la mascotte segue il mouse.

**Dati veri (0.2)**:
- Musica da qualunque lettore che usa i controlli multimediali di Windows (Spotify, browser, VLC...): titolo, artista, avanzamento, comandi, copertina sui LED, volume generale, visualizer dallo spettro reale dell'audio in uscita.
- Meteo da Open-Meteo, città scelta nelle Impostazioni (predefinita Milano), aggiornato ogni 30 minuti.
- Calendario Google tramite l'indirizzo segreto in formato iCal, incollato nelle Impostazioni (eventi ricorrenti compresi), aggiornato ogni 10 minuti.
- CPU, RAM e dischi; GPU NVIDIA (uso e temperatura) tramite il driver; temperatura della CPU da LibreHardwareMonitor se è aperto con il server web attivo (porta 8085).
- Font a punti Glyphling Dot (`tools/make_font.py`); se sul PC è installato il font 5x7 di Nothing si usa quello.

## Struttura

- `src/index.html`: il prototipo dell'interfaccia, generato da `tools/port_prototype.py` a partire dall'Artifact. Non va modificato a mano: si aggiorna il prototipo e si rilancia lo script.
- `src/host.js`, `src/host.css`, `src/host-pre.js`: adattano il prototipo alle finestre dell'app e lo collegano al backend.
- `src/grid.html`: la griglia di punti mostrata durante il trascinamento.
- `src/menu.html`: il menu in vetro dell'icona nell'area di notifica.
- `src-tauri/src/`: backend. `lib.rs` (stato e comandi), `tray.rs` (icona e menu), `snap.rs` (trascinamento e aggancio), `hit.rs` (forma cliccabile e mouse per la mascotte), `platform.rs` (chiamate Win32), `config.rs` (impostazioni), `system.rs` (statistiche del PC), `media.rs` (musica e volume), `spectrum.rs` (visualizer).

## Compilare

Sul PC Windows: Rust (`rustup`), Node.js LTS, poi

```
npm install
npx tauri dev          # avvio in sviluppo
npx tauri build        # installer in src-tauri/target/release/bundle/nsis/
```

Dal cloud (Linux) l'installer si produce con il target `x86_64-pc-windows-gnu` (servono mingw-w64 e NSIS):

```
npx tauri build --target x86_64-pc-windows-gnu --bundles nsis
```

Aggiornare l'interfaccia dal prototipo: `python3 tools/port_prototype.py prototipo.html src/index.html`.

## Note

- L'installer non è firmato: Windows SmartScreen avvisa al primo avvio ("Ulteriori informazioni" → "Esegui comunque").
- Impostazioni salvate in `%APPDATA%\com.okanfenwa.glyphling\config.json`.
- Font: Glyphling Dot (nostro), Doto e Space Mono (SIL Open Font License), inclusi in `src/fonts/`.
- Indirizzo del calendario, città e dischi scelti restano nel PC (archivio locale della finestra).
