---
id: 04c9de631cb42e1eb7d2
title: "Images et fichiers"
order: 40
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Faites glisser un fichier depuis votre gestionnaire de fichiers sur cette page. NoteCraft vous
demande quoi en faire :

- **Insérer l'image** : une copie va dans le dossier `assets` de cette page et la page l'affiche.
- **Joindre le fichier** : la même copie, affichée comme un lien.
- **Lier le fichier là où il est** : rien n'est copié, et le lien casse si le fichier bouge.

**Insertion**, puis **Image** ou **Fichier**, fait la même chose et vous demande de déposer le
fichier.

## Où ça va

```text
pictures-and-files/
  index.md
  assets/
    diagram.png
    budget.xlsx
```

L'image voyage avec la page : déplacez le dossier, l'image suit. Les noms sont rendus sûrs pour
tout système d'exploitation, deux fichiers ne portent jamais le même nom, et un fichier de plus
de 512 Mo est refusé.

## Coller

Un tableau copié depuis une autre application de notes ou un tableur se colle en tableau. Pour
coller une image, cliquez dans la page et appuyez sur **Ctrl+V**. NoteCraft enregistre une copie
dans le dossier `assets` de la page et l'insère dans la note.

## Impressions

Certaines applications savent transformer un PDF ou un document en images sur la page.
NoteCraft ne le peut pas encore ; un PDF que vous joignez reste un fichier, et la boîte de
dialogue le dit.

Suite : [Deux formats de page](../page-formats/).

*Cette page a été traduite par un modèle d'intelligence artificielle. Les corrections sont les bienvenues.*

![Ink](ink.svg)
