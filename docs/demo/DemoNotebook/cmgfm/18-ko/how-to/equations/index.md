---
id: 1e37ba7e97d397ca0bfc
title: "수식"
order: 25
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

NoteCraft는 TeX 수식을 진짜 수학 글꼴로 직접 그립니다. 이 페이지의 모든 수식은 일반 텍스트입니다. `< >` 버튼을 누르면 TeX를 볼 수 있습니다.

## 줄 안에서, 또는 따로

줄 안: $E = mc^2$, $\frac{a}{b}$ 는 글자와 함께 흐릅니다. 따로 놓은 블록은 더 크게 짜입니다:

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## 분수, 근호, 합, 적분

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## 행렬, 경우 나누기, 정렬

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## 악센트, 문자 체계, 기호

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## 수식 안의 글

수학 글꼴에 없는 글자, 예를 들어 아랍어와 히브리어는 페이지의 글꼴로 그려지고 제대로 이어집니다:

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## 목록에서 시작하기

삽입, 그다음 수식을 누르면 시작할 수식 목록이 열립니다. 하나를 고르면 그 TeX가 커서 위치에 들어갑니다.

수식이 잘못되어도 NoteCraft는 숨기지 않고 그 텍스트를 보여 줍니다.

다음: [줄에 태그 달기](../tag-a-line/).

*이 페이지는 AI 모델이 번역했습니다. 수정 제안을 환영합니다.*
