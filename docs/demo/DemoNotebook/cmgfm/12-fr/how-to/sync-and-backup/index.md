---
id: dd2e12c72d6fed399fa5
title: "Synchronisation et sauvegarde"
order: 60
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

NoteCraft n'a ni synchronisation à lui ni compte. Un carnet est un dossier de petits fichiers,
alors mettez votre dossier de carnets là où votre outil de synchronisation regarde déjà :
OneDrive, Dropbox, Google Drive, iCloud Drive, Syncthing, un partage réseau ou un dépôt git.
Aucun n'a besoin de comprendre NoteCraft. Ils copient des fichiers.

## Pourquoi ça marche

- **Un fichier par page.** Modifier une page réécrit un seul petit fichier, donc deux machines
  touchent rarement les mêmes octets.
- **Écritures atomiques.** Une page est écrite dans un fichier temporaire puis renommée par-dessus
  l'ancien, donc un client de synchronisation ne voit jamais une page à moitié écrite.
- **Des noms courts et sûrs.** Les noms de dossier font au plus 64 caractères, les chemins 255,
  et rien de ce que Windows refuse n'est jamais écrit.
- **Les copies en conflit sont repérées.** Quand un outil laisse une copie en conflit à côté
  d'une page, le vérificateur le signale : **Fichier**, puis **Vérifier**, ou
  `notecraft-cli check` depuis un terminal.

## Sauvegarde

Une sauvegarde est une copie du dossier. Il n'y a rien d'autre à exporter.

## Historique des pages

Ouvrez **Historique**, puis **Versions de page**, pour consulter et restaurer les copies
enregistrées de cette page. Pour récupérer des pages, sections ou groupes de sections supprimés,
ouvrez **Historique**, puis **Corbeille du bloc-notes**. Ces outils fonctionnent sur cet appareil
et ne remplacent pas une sauvegarde séparée du bloc-notes.

## Ce qui n'est pas pris en charge

NoteCraft ne fusionne pas deux versions d'une page. Si votre outil de synchronisation crée une
copie en conflit, vous choisissez la version à conserver.

Suite : [À votre goût](../make-it-yours/).

*Cette page a été traduite par un modèle d'intelligence artificielle. Les corrections sont les bienvenues.*
