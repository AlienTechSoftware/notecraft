---
id: 924deacaf9f4e7b92159
title: "Von rechts nach links"
order: 80
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Arabisch, Hebräisch, Persisch und Urdu werden von rechts gelesen. NoteCraft zeichnet sie so: Die
Buchstaben verbinden sich, eine Zeile hängt am rechten Rand, und eine Zeile, die beide
Richtungen mischt, wird vom Unicode-Algorithmus geordnet, sodass ein Preis seine Zahl richtig
herum behält:

السعر 123 ريال

Die Schriften, die das möglich machen, kommen mit der Anwendung, also sieht die Zeile oben auf
einem Rechner ohne arabische Schrift genauso aus. Hebräisch und Hindi haben ebenfalls
mitgelieferte Schriften:

שלום עולם

नमस्ते दुनिया

## Drei Richtungen, drei Bedienelemente

| Bedienelement | Wo | Was es festlegt |
|---|---|---|
| **Seite** | **Start**, dann **Richtung** | In welche Richtung die ganze Seite gelesen wird. Eine Zeile ohne eigene Buchstaben folgt ihr |
| **Absatz** | Das Ausrichtungsmenü in **Basistext** | In welche Richtung der Absatz mit dem Cursor gelesen wird |
| **Lauf** | **Mehr...** in der Gruppe Richtung | In welche Richtung die Auswahl gelesen wird, oder was Sie als Nächstes tippen |

Die Seitenrichtung ist NoteCrafts eigene; die Anwendungen, von denen die Leute kommen, haben
keine solche Einstellung, und ein auf Arabisch geschriebenes Notizbuch braucht sie.

## Wo die Einstellung gespeichert wird

Im Text selbst. Eine Seitenrichtung ist `dir: rtl` im Vorspann einer Markdown-Seite oder ein
meta-Tag in einer HTML-Seite. Eine Absatzrichtung ist ein unsichtbares Zeichen an seinem Anfang,
die Unicode-Marke für rechts-nach-links oder links-nach-rechts. Beides übersteht jeden anderen
Editor.

Die Gruppen Arabisch, Persisch, Urdu und Hebräisch dieses Notizbuchs sind durchgehend von rechts
nach links geschrieben. Öffnen Sie eine von der [Startseite](../../../00-start/welcome/) aus und
vergleichen Sie.

*Diese Seite wurde von einem KI-Modell übersetzt. Korrekturen sind willkommen.*
