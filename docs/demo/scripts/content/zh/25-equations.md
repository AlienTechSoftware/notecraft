---
title: 公式
---

# 公式

NoteCraft 自己绘制 TeX 数学公式，使用真正的数学字体。本页的每个公式都是纯文本：按 `< >` 按钮即可看到它的 TeX。

## 行内与独立成行

行内：$E = mc^2$, $\frac{a}{b}$ 和 $\sqrt{x^2+y^2}$ 与文字排在一起。独立的块排得更大：

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## 分数、根式、求和与积分

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## 矩阵、分段与对齐

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## 重音、字母表与符号

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## 公式里的文字

数学字体里没有的字母，例如阿拉伯文和希伯来文，会用页面的字体绘制，并正确连写：

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## 从列表开始

依次点击「插入」和「公式」，会打开一个可供起步的公式列表。选一个，它的 TeX 就会放到光标处。

公式写错时，NoteCraft 会显示它的文字，而不是把它藏起来。

下一页：[给一行加标签](../tag-a-line/)。

*本页由人工智能模型翻译。欢迎指正。*
