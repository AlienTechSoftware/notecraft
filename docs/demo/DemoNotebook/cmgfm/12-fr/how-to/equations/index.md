---
id: c69db873fd40256eefc8
title: "Équations"
order: 25
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

NoteCraft dessine lui-même les mathématiques en TeX, avec de vraies polices mathématiques. Chaque formule de cette page est du texte simple : appuyez sur le bouton `< >` pour voir son TeX.

## Dans une ligne, ou seules

Dans la ligne : $E = mc^2$, $\frac{a}{b}$ et $\sqrt{x^2+y^2}$ suivent les mots. Un bloc à part est composé plus grand :

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## Fractions, racines, sommes et intégrales

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## Matrices, cas et alignement

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## Accents, alphabets et symboles

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## Des mots dans une formule

Les lettres que les polices mathématiques n'ont pas, comme l'arabe et l'hébreu, sont dessinées avec la police de la page et s'enchaînent comme il se doit :

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## Partir d'une liste

Insertion, puis Équation, ouvre une liste d'équations pour commencer. Choisissez-en une et son TeX entre à l'endroit du curseur.

Si une formule est fausse, NoteCraft affiche son texte au lieu de la cacher.

Suite : [Étiqueter une ligne](../tag-a-line/).

*Cette page a été traduite par un modèle d'intelligence artificielle. Les corrections sont les bienvenues.*
