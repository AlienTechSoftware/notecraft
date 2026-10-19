---
id: 2f36e0a86a2107d3577b
title: "Regarder à l'intérieur d'une page"
order: 75
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

# Regarder à l'intérieur d'une page

Beaucoup d'applications de notes gardent un carnet entier dans un seul fichier binaire, et un éditeur de texte n'y montre que du bruit. Une page NoteCraft est un dossier de fichiers ordinaires, et la vue code vous montre ce dossier, fichier par fichier. Essayez maintenant : appuyez sur le bouton `< >` à côté de cette page.

## Ce que vous verrez

À gauche, **Fichiers de cette page** : d'abord le fichier de la page elle-même, `index.md`, puis tout le reste du dossier. Un fichier que rien dans la page n'utilise est marqué *non utilisé par la page*. À droite se trouve le fichier que vous avez choisi. C'est la page que vous lisez, donc le visualiseur montre son propre texte.

Le fichier de la page affiche son **en-tête** en gris : l'identifiant, le titre, l'ordre et les dates de la page. Vous ne pouvez pas y écrire, donc une modification à la main ne peut pas abîmer l'identifiant de la page. En dessous se trouve le **corps**, que vous pouvez modifier. Appuyez de nouveau sur le même bouton pour fermer la vue, et la page est redessinée à partir de ce que contient le corps.

| Fichier | Ce que montre le visualiseur | Peut-on y écrire ? |
|---|---|---|
| Le fichier de la page, `index.md` ou `index.html` | L'en-tête, et le corps tel qu'il a été enregistré | Le corps |
| Un dessin, `ink.svg` | Le texte SVG, avec les mesures propres du stylet | Oui, avec précaution |
| Une image | L'image | Non |
| Un autre texte | Son texte | Non |
| Tout le reste | Son nom, sa taille et son type | Non |

Un fichier de plus de 256 Ko montre son début et le dit.

## Ce que cela vous montre

- Une page organisée en cadres a un `div` avec `class="nc-box"` autour de chaque cadre.
- Un dessin est un fichier, `ink.svg`, et le texte de la page le nomme en une ligne.
- Les étiquettes et les liens sont du texte simple.

Rien n'est caché : si la page le montre, un fichier de ce dossier le contient.

Suite : [De droite à gauche](../right-to-left/).

*Cette page a été traduite par un modèle d'intelligence artificielle. Les corrections sont les bienvenues.*
