---
id: 5b46fc7ee4d1d2d3e783
title: "משוואות"
order: 25
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
dir: rtl
tags: []
---

NoteCraft מצייר בעצמו מתמטיקה ב-TeX, בגופנים מתמטיים אמיתיים. כל נוסחה בעמוד הזה היא טקסט פשוט: לחצו על הכפתור `< >` כדי לראות את ה-TeX שלה.

## בתוך שורה, או לבד

בתוך שורה: $E = mc^2$, $\frac{a}{b}$ זורמים עם המילים. בלוק נפרד נערך גדול יותר:

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## שברים, שורשים, סכומים ואינטגרלים

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

$$
\int_0^{\infty} e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## מטריצות, מקרים ויישור

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}
$$

$$
\begin{aligned} (a+b)^2 &= a^2 + 2ab + b^2 \\ (a-b)^2 &= a^2 - 2ab + b^2 \end{aligned}
$$

## סימני הטעמה, אלפביתים וסמלים

$$
\hat{x} + \vec{v} + \bar{y} \qquad x \in \mathbb{R}, \; \mathcal{L}\{f\}(s) = \int_0^\infty f(t)e^{-st}\,dt
$$

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0} \qquad i\hbar\frac{\partial}{\partial t}\Psi = \hat{H}\Psi
$$

## מילים בתוך נוסחה

אותיות שאין בגופנים המתמטיים, כמו ערבית ועברית, מצוירות בגופן של העמוד ומתחברות כראוי:

$$
\text{المعادلة} \; x^2 = 4 \qquad \text{שלום} \; \pi r^2
$$

## להתחיל מרשימה

הוספה, ואז משוואה, פותחת רשימת משוואות להתחיל מהן. בחרו אחת וה-TeX שלה ייכנס במקום הסמן.

אם נוסחה שגויה, NoteCraft מציג את הטקסט שלה במקום להסתיר אותה.

הבא: [תיוג שורה](../tag-a-line/).

*העמוד הזה תורגם על ידי מודל בינה מלאכותית. תיקונים יתקבלו בברכה.*
