<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri">
  <h1>Kiri</h1>
  <p>Screenshots, Texterkennung und Bildschirmaufnahmen. Ihre Aufnahmen bleiben lokal.</p>
</div>

[简体中文](README.md) · [English](README_EN.md) · [繁體中文](README_ZH_TW.md) · [日本語](README_JA.md) · **Deutsch** · [한국어](README_KO.md) · [Français](README_FR.md)

[Website](https://kiri.yuxino.cn) · [Aktuelle Version herunterladen](https://github.com/yuxino/kiri/releases/latest)

Kiri ist eine Screenshot- und Bildschirmaufnahme-App für macOS, Windows und Linux. Fügen Sie Pfeile, Text oder Mosaik zu Screenshots hinzu, kopieren Sie Text aus Bildern und speichern Sie Bilder und Videos auf Ihrem Computer.

![Anmerkungsoberfläche von Kiri](docs/assets/readme-preview.png)

## Funktionen

- Fenster oder Bereiche erfassen, zuschneiden, zeichnen, Text oder Mosaik hinzufügen und Screenshots als schwebende Referenz anheften.
- Text mit lokalem OCR kopieren und QR-Codes aus Bildern lesen.
- MP4 mit optionalem Systemton und Mikrofon aufnehmen oder ein GIF ohne Ton erstellen. Bei der GIF-Konvertierung gespeicherter Videos wird zuerst die Dekodierung geprüft. Prüfung und Konvertierung lassen sich abbrechen. macOS zeigt den Fortschritt pro Bild und hält Fehlerdetails sichtbar.
- Clips eines Videos kürzen und umordnen, dann eine neue MP4-Datei exportieren.
- Lokale Aufnahmen suchen, mit Tags versehen, favorisieren und exportieren. Versehentlich gelöschte Inhalte lassen sich aus dem Papierkorb wiederherstellen.

Wählen Sie eine vorhandene Anmerkung aus, um ihren Stil zu ändern. Mosaik bietet Freihand, Rechteck und Ellipse sowie Pixel- und Unschärfeeffekte mit einstellbarer Stärke. Mit Wasserzeichen (W) schreiben Sie ein wiederholtes Muster direkt ins Bild und passen Deckkraft, Winkel und Abstand an. Wählen Sie das Werkzeug erneut, um vorhandenen Text weiterzubearbeiten, auch nach dem Speichern.

## Erste Schritte

1. Drücken Sie `⇧⌘A` auf macOS oder `Shift+Ctrl+A` auf Windows / Linux X11. Wählen Sie ein Fenster oder ziehen Sie einen Bereich auf.
2. Wählen Sie Screenshot, Aufnahme oder OCR.
3. Bestätigen Sie einen Screenshot mit `Enter` oder einem Doppelklick auf eine unmarkierte Stelle im Bereich; `Esc` bricht ab. Wechseln Sie beim Zeichnen zunächst zum Auswahlwerkzeug. Ein Doppelklick auf Text bearbeitet ihn weiterhin. Screenshots werden in die Zwischenablage kopiert und in der Bibliothek gespeichert. Das Pinsymbol in der Werkzeugleiste speichert und heftet den Screenshot mit einem Klick an. „Anheften“ auf der Abschlusskarte bleibt verfügbar. Die rahmenlose Referenz lässt sich ziehen und an der unteren rechten Ecke proportional vergrößern oder verkleinern. Beim Darüberfahren erscheinen die Schaltflächen zum Lösen und Schließen.

Im Screenshot-Modus zeigt die Lupe beim Bewegen des Mauszeigers die ursprünglichen Pixel, Koordinaten und den HEX-Farbwert. Mit `⌘C` auf macOS oder `Ctrl+C` kopierst du die Farbe; die Auswahl bleibt erhalten. Beim Annotieren wird die Lupe ausgeblendet.

Klicken Sie vor dem Zeichnen oder Aufnehmen auf die Schieberegler, um Breite und Höhe direkt am Auswahlrand in Pixeln einzugeben.

Unter „Einstellungen → Allgemein → Sprache“ stehen sieben Oberflächensprachen zur Auswahl. Ihre Wahl gilt für alle Kiri-Fenster und bleibt nach einem Neustart erhalten. Beim ersten Start wird die Systemsprache verwendet.

Aufnahmen bleiben lokal. Remote-OCR ist optional und fragt vor jedem Upload nach. Einrichtung, MP4-Tonaufnahme, Wayland-Tastenkürzel und Einschränkungen unter Linux stehen im [Linux-Leitfaden](docs/linux.md).

[Installation und Hilfe](docs/usage.de.md) · [Videobearbeitung](docs/video-editing.de.md) · [Fehler melden](https://github.com/yuxino/kiri/issues) · [Mitwirken](CONTRIBUTING.md)

## Mitwirkende

Vielen Dank an alle, die Code schreiben, Probleme melden, Kiri ausprobieren oder teilen.

Besonderer Dank gilt [@kerwin2046](https://github.com/kerwin2046) für die [erste Linux-Unterstützung](https://github.com/yuxino/kiri/pull/20) und [@LLLin000](https://github.com/LLLin000) für die [Korrektur der Screenshot-Ausrichtung bei Windows-Textskalierung](https://github.com/yuxino/kiri/pull/61).

[Alle Mitwirkenden](https://github.com/yuxino/kiri/graphs/contributors)

## Community

Danke auch an die Menschen bei [V2EX](https://www.v2ex.com/), [LINUX DO](https://linux.do/), [Appinn](https://www.appinn.com/) und [NodeLoc](https://www.nodeloc.com/) für das Ausprobieren, Rückmeldungen und Weiterempfehlen.

[MIT](LICENSE) © 2026 yuxino
