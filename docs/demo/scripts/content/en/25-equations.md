---
title: Equations
---

# Equations

NoteCraft draws TeX maths itself, in real maths fonts. Every formula on this page is plain text: press the `< >` button to see its TeX.

## In a line, or on its own

Inline: $E = mc^2$, $\frac{a}{b}$ and $\sqrt{x^2+y^2}$ flow with the words. A block of its own is set larger:

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## Fractions, roots, sums and integrals

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## Matrices, cases and alignment

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## Accents, alphabets and symbols

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## Words inside a formula

Letters the maths fonts do not have, such as Arabic and Hebrew, are drawn in the page's font and join as they should:

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## Start from a list

Insert, then Equation, opens a list of equations to start from. Pick one and its TeX goes in at the caret.

If a formula is wrong, NoteCraft shows its text instead of hiding it.

Next: [Tag a line](../tag-a-line/).
