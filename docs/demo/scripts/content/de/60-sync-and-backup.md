---
title: Synchronisieren und sichern
---

# Synchronisieren und sichern

NoteCraft hat keine eigene Synchronisierung und kein Konto. Ein Notizbuch ist ein Ordner mit
kleinen Dateien, also legen Sie Ihren Notizbuchordner dorthin, wo Ihr Synchronisierungsdienst
schon hinschaut: OneDrive, Dropbox, Google Drive, iCloud Drive, Syncthing, eine Netzwerkfreigabe
oder ein git-Repository. Keiner davon muss NoteCraft verstehen. Sie kopieren Dateien.

## Warum das funktioniert

- **Eine Datei pro Seite.** Eine Seite zu bearbeiten schreibt eine einzige kleine Datei neu,
  also berühren zwei Rechner selten dieselben Bytes.
- **Atomare Schreibvorgänge.** Eine Seite wird in eine temporäre Datei geschrieben und dann über
  die alte umbenannt, also sieht ein Synchronisierungsclient nie eine halb geschriebene Seite.
- **Kurze, sichere Namen.** Ordnernamen haben höchstens 64 Zeichen, Pfade höchstens 255, und
  nichts, was Windows ablehnt, wird je geschrieben.
- **Konfliktkopien fallen auf.** Legt ein Dienst eine Konfliktkopie neben eine Seite, meldet es
  der Prüfer: **Datei**, dann **Prüfen**, oder `notecraft-cli check` im Terminal.

## Sicherung

Eine Sicherung ist eine Kopie des Ordners. Es gibt nichts anderes zu exportieren.

## Was nicht behandelt wird

NoteCraft führt zwei Fassungen einer Seite nicht zusammen; Ihr Dienst legt eine Konfliktkopie
an, und Sie entscheiden. Es hat keinen eigenen Seitenverlauf: Nutzen Sie einen synchronisierten
Ordner, der Versionen behält, oder git.

Weiter: [Nach Ihrem Geschmack](../make-it-yours/).

*Diese Seite wurde von einem KI-Modell übersetzt. Korrekturen sind willkommen.*
