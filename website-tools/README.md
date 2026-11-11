# Website: Screenshots neu erzeugen

Die Bilder in `website/img/*-light|dark-*.webp` und `ipad-*.webp` sind die echte Oberfläche (index.html, src/),
im Browser gerendert. Das Tauri-Backend ist durch `mock.js` ersetzt (Beispiel-Bibliothek, Shell-Ausgabe aus
`shell.js`, Remote-Bild aus `desk.js`). Windows-Darstellung mit Fluent-Symbolen; die Fensterknöpfe sind
ausgeblendet. Auf macOS zeichnet das System die SF Symbols, das geht im Browser nicht.

```bash
npx vite --port 1420 &                       # im Repo-Wurzelordner
cd website-tools
ACT=./act-sel.mjs node shot.mjs lib-light light win 1440 900 ./desk.js,./shell.js
# weitere: act-desk, act-shell, act-files, act-cred; Schema light|dark; ipad statt win (1180 820)
node og.mjs                                  # website/img/og.png
```

Danach PNG → WebP (1440 und 2400 px breit, Qualität 82), siehe Dateinamen in `website/img/`.
