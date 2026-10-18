---
title: De derecha a izquierda
---

# De derecha a izquierda

El árabe, el hebreo, el persa y el urdu se leen desde la derecha. NoteCraft los dibuja así: las
letras se unen, una línea cuelga del borde derecho, y una línea que mezcla las dos direcciones
se ordena con el algoritmo Unicode, de modo que un precio conserva su número del derecho:

السعر 123 ريال

Las fuentes que lo hacen posible vienen con la aplicación, así que la línea de arriba se ve
igual en una máquina sin fuente árabe instalada. El hebreo y el hindi también tienen sus fuentes
incluidas:

שלום עולם

नमस्ते दुनिया

## Tres direcciones, tres controles

| Control | Dónde | Qué fija |
|---|---|---|
| **Página** | **Inicio**, luego **Dirección** | En qué sentido se lee toda la página. Una línea sin letras propias la sigue |
| **Párrafo** | El menú de alineación en **Texto básico** | En qué sentido se lee el párrafo donde está el cursor |
| **Tramo** | **Más...** en el grupo Dirección | En qué sentido se lee la selección, o lo que escribas a continuación |

La dirección de página es propia de NoteCraft; las aplicaciones de las que viene la gente no
tienen ese ajuste, y un cuaderno escrito en árabe lo necesita.

## Dónde se guarda el ajuste

En el propio texto. La dirección de página es `dir: rtl` en la cabecera de una página Markdown
o una etiqueta meta en una página HTML. La dirección de párrafo es un carácter invisible al
principio, la marca Unicode de derecha a izquierda o de izquierda a derecha. Ambos sobreviven en
cualquier otro editor.

Los grupos en árabe, persa, urdu y hebreo de este cuaderno están escritos de derecha a izquierda
de principio a fin. Abre uno desde [la página de inicio](../../../00-start/welcome/) y compara.

*Esta página la tradujo un modelo de inteligencia artificial. Las correcciones son bienvenidas.*
