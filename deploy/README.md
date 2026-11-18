# Online-Katalog veröffentlichen

Diesen Ordner so hochladen, dass er unter der Katalog-Adresse erreichbar ist.
Standard im Launcher: `https://mlbclient.vercel.app/mooncrest/v1/products.json`
(also `mooncrest/` in das Root der MLBClient-Website legen). Die Adresse lässt sich in den Einstellungen ändern.

```
mooncrest/v1/products.json   Produkte, Versionen, SHA-256, Größen, Download-URLs
mooncrest/v1/news.json       News (gleicher Ordner wie products.json)
mooncrest/v1/files/          Installer – Dateinamen müssen zu den "url"-Einträgen passen:
    MLBClient-Launcher-Beta2-0.3.0-setup.exe          ← D:\MLBClient\Launcher\Beta 2 (0.3.0)\MLBClient-Launcher-Beta2-0.3.0-setup.exe
    MLBClient-Launcher_0.2.0-beta_x64-setup.exe       ← D:\MLBClient\Launcher\Beta 1 (0.2.0)\MLBClient Launcher_0.2.0-beta_x64-setup.exe
```

Prüfen nach dem Hochladen:
- `products.json` und `news.json` müssen als JSON ausgeliefert werden (nicht als HTML-Seite).
- Die Installer müssen genau die Größe und den SHA-256 aus `products.json` haben, sonst lehnt der Launcher sie ab.
- Range-Requests sollten funktionieren (Vercel, Netlify, GitHub Releases und Cloudflare können das), damit Downloads nach Abbrüchen weiterlaufen.

Neue Version: Datei in `files/` legen, in `products.json` einen neuen Eintrag oben in `releases` anlegen
(`version`, `date`, `notes`, `files[]` mit `sha256` und `size`), `generated` auf das heutige Datum setzen, hochladen.
PowerShell: `Get-FileHash -Algorithm SHA256 .\datei.exe` und `(Get-Item .\datei.exe).Length`.

Launcher-Update: im Block `launcher.latest` eine neue `version` mit `files[]` (Plattform `windows-x86_64`) eintragen.
