---
id: 2db8352078c7e7bfff3f
title: "De droite à gauche"
order: 80
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

# De droite à gauche

L'arabe, l'hébreu, le persan et l'ourdou se lisent depuis la droite. NoteCraft les dessine
ainsi : les lettres se lient, une ligne pend du bord droit, et une ligne qui mélange les deux
sens est ordonnée par l'algorithme Unicode, si bien qu'un prix garde son nombre à l'endroit :

السعر 123 ريال

Les polices qui le permettent sont livrées avec l'application, donc la ligne ci-dessus a le même
aspect sur une machine sans police arabe. L'hébreu et le hindi ont aussi leur police intégrée :

שלום עולם

नमस्ते दुनिया

## Trois sens, trois commandes

| Commande | Où | Ce qu'elle fixe |
|---|---|---|
| **Page** | **Accueil**, puis **Sens** | Dans quel sens se lit toute la page. Une ligne sans lettres à elle le suit |
| **Paragraphe** | Le menu d'alignement dans **Texte de base** | Dans quel sens se lit le paragraphe où est le curseur |
| **Segment** | **Plus...** dans le groupe Sens | Dans quel sens se lit la sélection, ou ce que vous tapez ensuite |

Le sens de page est propre à NoteCraft ; les applications d'où viennent les gens n'ont pas ce
réglage, et un carnet écrit en arabe en a besoin.

## Où le réglage est gardé

Dans le texte lui-même. Un sens de page est `dir: rtl` dans l'en-tête d'une page Markdown ou
une balise meta dans une page HTML. Un sens de paragraphe est un caractère invisible à son
début, la marque Unicode de droite à gauche ou de gauche à droite. Les deux survivent dans tout
autre éditeur.

Les groupes arabe, persan, ourdou et hébreu de ce carnet sont écrits de droite à gauche d'un bout
à l'autre. Ouvrez-en un depuis [la page de départ](../../../00-start/welcome/) et comparez.

*Cette page a été traduite par un modèle d'intelligence artificielle. Les corrections sont les bienvenues.*
