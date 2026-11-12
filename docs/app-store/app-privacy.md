# App Store Connect: App-Datenschutz («App Privacy»)

Antworten für App Store Connect › App › App-Datenschutz. Grundlage: `website/datenschutz.html`,
`src-tauri/gen/apple/sesam-remote_iOS/PrivacyInfo.xcprivacy` und der Code (geprüft am 12.11.2026).

## Datenschutzrichtlinie

- URL: https://sesam-remote.app/datenschutz.html
- Auswahl für die Datenschutzerklärung der App (falls gefragt): dieselbe URL.

## «Erfassen du oder deine Drittanbieter Daten von dieser App?»

**Nein, wir erfassen keine Daten von dieser App.**

Ergebnis im App Store: Etikett «Keine Daten erfasst» («Data Not Collected»).

Begründung (Apples Definition: «erfassen» = Daten verlassen das Gerät so, dass der Entwickler oder Dritte sie
länger speichern, als für die Anfrage nötig):

| Was | Wohin | Erfassung im Sinne von Apple? |
|---|---|---|
| Verbindungen, Gruppen, Einstellungen, Hostschlüssel, Vorschaubilder, Protokoll `sesam.log` | Nur App-Datenordner auf dem iPad | Nein, verlässt das Gerät nicht |
| Passwörter, SSH-Schlüssel | Schlüsselbund des iPad | Nein |
| Anmeldedaten, Bildschirm, Tastatur, Dateien, Zwischenablage | Nur an den Server, den der Benutzer selbst einträgt | Nein: Der Entwickler betreibt diese Server nicht und erhält nichts; Apple zählt das nicht als Erfassung durch den Entwickler |
| Sicherung (`.sesam`) | Datei dort, wo der Benutzer sie speichert, verschlüsselt (Argon2id + AES-256-GCM) | Nein |
| Absturzberichte/Analysen | Nur Apples eigene, wenn der Benutzer sie in iPadOS freigibt | Nein: Daten, die Apple selbst erhebt, gibt man nicht an |

Kein SDK von Dritten, keine Werbung, keine Analyse, kein Absturz-Werkzeug, kein eigener Server. Der
automatische Updater (`tauri_plugin_updater`, fragt GitHub ab) ist nur auf dem Desktop eingebaut
(`#[cfg(desktop)]` in `src-tauri/src/lib.rs`); die iPad-App macht keine Anfragen an eigene oder fremde Dienste.
Einziger Link nach aussen: die Website in «Über Sesam» und in der Hilfe, die der Benutzer selbst öffnet
(Safari).

## Tracking

- «Verwendet die App Daten zum Tracking?» **Nein.**
- Kein App-Tracking-Transparency-Dialog nötig, keine `NSUserTrackingUsageDescription`.
- Manifest: `NSPrivacyTracking = false`, `NSPrivacyTrackingDomains` leer, `NSPrivacyCollectedDataTypes` leer.

## Konsistenz mit dem Manifest (`PrivacyInfo.xcprivacy`)

| Manifest | App Store Connect |
|---|---|
| `NSPrivacyCollectedDataTypes` leer | «Keine Daten erfasst» |
| `NSPrivacyTracking` false | Tracking: Nein |
| Required Reason API: Dateizeitstempel (DDA9.1, C617.1, 3B52.1) | nicht in App Store Connect anzugeben (nur Manifest) |
| Required Reason API: Systemstartzeit (35F9.1) | nicht in App Store Connect anzugeben |

Zu prüfen beim ersten Hochladen: App Store Connect meldet in der E-Mail nach dem Upload fehlende
Required-Reason-Gründe (ITMS-91053), falls Bibliotheken weitere APIs nutzen (z. B. `UserDefaults` über WebKit
oder Tauri). Dann den Grund im Manifest ergänzen, nicht in App Store Connect.

## Berechtigungstexte (Info.plist), passend zur Erklärung

- `NSLocalNetworkUsageDescription`: «Sesam verbindet sich mit Rechnern in deinem Netzwerk (SSH, RDP, VNC und Dateien).»
- `NSFaceIDUsageDescription`: «Sesam zeigt gespeicherte Passwörter erst nach deiner Bestätigung.»
- Kamera, Mikrofon, Standort, Fotos, Kontakte: nicht benutzt, kein Text vorhanden. Richtig so.

## Wenn sich etwas ändert

Bekommt die iPad-App je eine Abfrage an einen eigenen Dienst (Update-Hinweis, Lizenzprüfung, Absturz-SDK),
müssen Etikett, Manifest und `website/datenschutz.html` zusammen angepasst werden.
