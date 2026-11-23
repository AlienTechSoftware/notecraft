---
id: cce691b423a510e55381
title: "Denklemler"
order: 25
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

NoteCraft TeX matematiğini kendisi, gerçek matematik yazı tipleriyle çizer. Bu sayfadaki her formül düz
metindir: TeX'ini görmek için `< >` düğmesine basın.

## Satır içinde ya da kendi başına

Satır içinde: $E = mc^2$, $\frac{a}{b}$ ve $\sqrt{x^2+y^2}$ sözcüklerle birlikte akar. Kendi bloğu
daha büyük yazılır:

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## Kesirler, kökler, toplamlar ve integraller

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## Matrisler, durumlar ve hizalama

$$
\begin{pmatrix} a & b \ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## Vurgular, alfabeler ve simgeler

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## Formülün içindeki sözcükler

Matematik yazı tiplerinde olmayan harfler, örneğin Arapça ve İbranice, sayfanın yazı tipiyle çizilir ve
gerektiği gibi birleşir:

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## Bir listeden başlamak

Ekle, sonra Denklem, başlamak için bir denklem listesi açar. Birini seçin; TeX'i imlecin olduğu yere
girer.

Bir formül yanlışsa NoteCraft onu gizlemek yerine metnini gösterir.

Sonraki: [Bir satırı etiketlemek](../tag-a-line/).

*Bu sayfa bir yapay zekâ modeli tarafından çevrildi. Düzeltmeler memnuniyetle karşılanır.*
