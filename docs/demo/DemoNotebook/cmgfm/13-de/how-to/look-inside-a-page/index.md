---
id: 11bc19d1e947e1e14bad
title: "In eine Seite hineinschauen"
order: 75
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Viele Notiz-Apps halten ein ganzes Notizbuch in einer einzigen Binärdatei, und ein Texteditor zeigt darin nur Zeichensalat. Eine NoteCraft-Seite ist ein Ordner mit gewöhnlichen Dateien, und die Codeansicht zeigt Ihnen diesen Ordner, Datei für Datei. Probieren Sie es jetzt aus: Klicken Sie auf die Schaltfläche `< >` neben dieser Seite.

## Was Sie sehen

Links stehen die **Dateien dieser Seite**: zuerst die Datei der Seite selbst, `index.md`, dann alles andere im Ordner. Eine Datei, die nichts auf der Seite verwendet, ist mit *von der Seite nicht verwendet* markiert. Rechts steht die Datei, die Sie gewählt haben. Das ist die Seite, die Sie gerade lesen, deshalb zeigt der Betrachter ihren eigenen Text.

Die Seitendatei wird ganz gezeigt: ihr **Kopf** (Kennung, Titel, Reihenfolge, Daten und Einstellungen der Seite) und darunter der **Inhalt**. Sie können alles bearbeiten. Drücken Sie dieselbe Schaltfläche noch einmal, um die Ansicht zu schließen, und die Seite wird nach dem, was Sie geschrieben haben, neu gezeichnet.

| Datei | Was der Betrachter zeigt | Kann man darin schreiben? |
|---|---|---|
| Die Seitendatei, `index.md` oder `index.html` | Den Kopf und den Inhalt genau wie gespeichert | Alles |
| Eine Zeichnung, `ink.svg` | Den SVG-Text mit den eigenen Messwerten des Stifts | Ja, mit Vorsicht |
| Ein Bild | Das Bild | Nein |
| Anderer Text | Seinen Text | Nein |
| Alles andere | Name, Größe und Typ | Nein |

Eine Datei über 256 KB zeigt ihren Anfang und sagt das dazu.

## Was es Ihnen zeigt

- Eine Seite mit Kästen hat um jeden Kasten ein `div` mit `class="nc-box"`.
- Eine Zeichnung ist eine Datei, `ink.svg`, und der Text der Seite nennt sie in einer Zeile.
- Markierungen und Links sind einfacher Text.

Nichts ist versteckt: Was die Seite zeigt, steht in einer Datei in diesem Ordner.

Weiter: [Von rechts nach links](../right-to-left/).

*Diese Seite wurde von einem KI-Modell übersetzt. Korrekturen sind willkommen.*
