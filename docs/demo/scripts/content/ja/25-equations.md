---
title: 数式
---

# 数式

NoteCraft は TeX の数式を、本物の数式フォントで自分で描きます。このページの数式はどれもプレーンテキストです。`< >` ボタンを押すと TeX が見えます。

## 行の中と、単独で

行の中：$E = mc^2$, $\frac{a}{b}$ は文字と一緒に流れます。単独のブロックは大きく組まれます：

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## 分数、根号、総和、積分

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## 行列、場合分け、整列

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## アクセント、文字種、記号

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## 数式の中の言葉

数式フォントにない文字、たとえばアラビア語やヘブライ語は、ページのフォントで描かれ、正しくつながります：

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## 一覧から始める

［挿入］から［数式］を選ぶと、出発点にできる数式の一覧が開きます。ひとつ選ぶと、その TeX がカーソルの位置に入ります。

数式が間違っていても、NoteCraft は隠さずに、そのテキストを表示します。

次：[行にタグを付ける](../tag-a-line/)。

*このページは AI モデルが翻訳しました。修正を歓迎します。*
