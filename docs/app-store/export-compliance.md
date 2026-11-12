# Exportkontrolle (Verschlüsselung)

Stand 12.11.2026. Keine Rechtsberatung; die Einordnung folgt Apples Hilfe «Export compliance documentation for
encryption» und den US-Regeln (EAR) für frei verkäufliche Software mit Standardverschlüsselung.

## Was Sesam verschlüsselt

Alles mit offenen Standardverfahren, umgesetzt in Open-Source-Bibliotheken (nicht mit den Verfahren von iPadOS):

| Wofür | Verfahren | Bibliothek |
|---|---|---|
| SSH, SFTP, Türöffner | Curve25519/ECDH, Ed25519/RSA/ECDSA, AES-CTR/GCM, ChaCha20-Poly1305, HMAC-SHA2 | `russh` mit `ring` |
| TLS (RDP, FTPS, RD-Gateway) | TLS 1.2/1.3 | `rustls` mit `ring` |
| RDP-Anmeldung | CredSSP/NLA (NTLM, Kerberos), TLS | `ironrdp` |
| VNC-Anmeldung | VNC-Authentifizierung (DES), Apple Remote Desktop (DH + AES-128) | `des`, `aes` |
| Sicherung `.sesam` | Argon2id + AES-256-GCM | `argon2`, `aes-gcm` |
| Passwörter auf dem Gerät | Schlüsselbund von iPadOS | iPadOS |

Kein eigenes oder geheimes Verfahren. Der Quelltext ist öffentlich (MPL-2.0, GitHub).

## Antwort in App Store Connect

Beim ersten Build (oder unter App › Allgemein › App-Verschlüsselungsdokumentation):

1. «Welche Art von Verschlüsselungsalgorithmen verwendet deine App?»
   → **«Standardverschlüsselungsalgorithmen anstelle von oder zusätzlich zur Verschlüsselung im
   Betriebssystem von Apple»** (Standard encryption algorithms instead of, or in addition to, using or
   accessing the encryption within Apple's operating system).
   Nicht «keine» und nicht «nur Betriebssystem»: Sesam bringt eigene Implementierungen mit.
   Nicht «proprietär»: alles sind IETF/NIST-Standards.
2. «Wird die App im App Store in Frankreich angeboten?»
   - Laut Apple braucht es für Standardverfahren **nur für Frankreich** ein Dokument (die französische
     Verschlüsselungserklärung bei der ANSSI), sonst keine Unterlagen in App Store Connect.
   - **Empfehlung für die erste Version: Frankreich unter «Preise und Verfügbarkeit» abwählen.** Dann ist
     nichts hochzuladen. Frankreich später nachreichen, sobald die Erklärung bei der ANSSI gemacht ist.
3. Ein US-«CCATS» braucht es nur für proprietäre Verfahren, also **nicht**.

## Jährlicher Selbstklassifizierungsbericht (US BIS)

Für frei verkäufliche Software («mass market», ECCN 5D992.c, Ausnahme § 740.17(b)(1) EAR) mit
Standardverfahren hat das US-Handelsministerium den jährlichen Selbstklassifizierungsbericht 2021
abgeschafft; er gilt nur noch für Bauteile wie Chips und Entwicklungs-Kits. Sesam ist eine fertige App für
Endkunden. **Nach dieser Lesart ist kein Jahresbericht nötig.** Apple verlangt ihn in App Store Connect auch
nicht. Unsicher bleibt nur Frankreich (siehe oben). Die Schweiz (Güterkontrollverordnung) nimmt frei
verkäufliche Massenware mit Standardverschlüsselung ebenfalls aus.

## Info.plist-Schlüssel

`ITSAppUsesNonExemptEncryption` ist **noch nicht** gesetzt, mit Absicht:

- Der Wert wäre `YES` (Sesam fällt nicht unter die reinen Ausnahmen wie «nur Anmeldung» oder «nur HTTPS über
  iPadOS»). `NO` wäre falsch.
- Mit `YES`, aber ohne `ITSEncryptionExportComplianceCode` fragt App Store Connect bei jedem Build ohnehin
  nach; ohne Schlüssel ebenso. Gesetzt bringt er erst etwas zusammen mit dem Code.
- **Sobald** die Antwort oben in App Store Connect einmal gegeben ist, zeigt Apple einen Code zur
  Exportkonformität. Dann beide Schlüssel in `src-tauri/gen/apple/sesam-remote_iOS/Info.plist` eintragen:

  ```xml
  <key>ITSAppUsesNonExemptEncryption</key>
  <true/>
  <key>ITSEncryptionExportComplianceCode</key>
  <string>(Code aus App Store Connect)</string>
  ```

  Danach fragt App Store Connect bei neuen Builds nicht mehr.

Quelle: [Apple, Export compliance documentation for encryption](https://developer.apple.com/help/app-store-connect/reference/app-information/export-compliance-documentation-for-encryption/)
(Tabelle: Standardverfahren → nur französische Erklärung, und nur bei Verkauf in Frankreich).
