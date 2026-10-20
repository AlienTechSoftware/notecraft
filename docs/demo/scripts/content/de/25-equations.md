---
title: Formeln
---

# Formeln

NoteCraft zeichnet TeX-Mathematik selbst, mit echten Mathematikschriften. Jede Formel auf dieser Seite ist einfacher Text: Mit der Schaltfläche `< >` sehen Sie ihr TeX.

## In der Zeile oder für sich

In der Zeile: $E = mc^2$, $\frac{a}{b}$ und $\sqrt{x^2+y^2}$ laufen mit den Wörtern. Ein eigener Block wird größer gesetzt:

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## Brüche, Wurzeln, Summen und Integrale

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## Matrizen, Fallunterscheidungen und Ausrichtung

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## Akzente, Alphabete und Symbole

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## Wörter in einer Formel

Buchstaben, die die Mathematikschriften nicht haben, etwa Arabisch und Hebräisch, werden in der Schrift der Seite gezeichnet und verbinden sich, wie es sein soll:

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## Mit einer Liste anfangen

Einfügen, dann Gleichung, öffnet eine Liste von Gleichungen zum Anfangen. Wählen Sie eine, und ihr TeX steht an der Textmarke.

Ist eine Formel falsch, zeigt NoteCraft ihren Text, statt sie zu verbergen.

Weiter: [Eine Zeile markieren](../tag-a-line/).

*Diese Seite wurde von einem KI-Modell übersetzt. Korrekturen sind willkommen.*
