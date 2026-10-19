---
id: 3d3ca34a2b7108986920
title: "Mirar dentro de una página"
order: 75
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

# Mirar dentro de una página

Muchas aplicaciones de notas guardan un cuaderno entero en un único archivo binario, y un editor de texto solo muestra ruido. Una página de NoteCraft es una carpeta de archivos normales, y la vista de código te enseña esa carpeta, archivo por archivo. Pruébalo ahora: pulsa el botón `< >` junto a esta página.

## Qué verás

A la izquierda, **Archivos de esta página**: primero el archivo de la propia página, `index.md`, y después todo lo demás que haya en la carpeta. Un archivo que nada en la página usa aparece marcado como *no usado por la página*. A la derecha está el archivo que has elegido. Esta es la página que estás leyendo, así que el visor muestra su propio texto.

El archivo de la página muestra su **cabecera** en gris: el id, el título, el orden y las fechas de la página. No puedes escribir en ella, así que una edición a mano no puede estropear el id de la página. Debajo está el **cuerpo**, que sí puedes editar. Pulsa el mismo botón otra vez para cerrar la vista, y la página se dibuja de nuevo a partir de lo que hay en el cuerpo.

| Archivo | Qué muestra el visor | ¿Se puede escribir en él? |
|---|---|---|
| El archivo de la página, `index.md` o `index.html` | La cabecera y el cuerpo tal como se guardó | El cuerpo |
| Un dibujo, `ink.svg` | El texto SVG, con las lecturas propias del lápiz | Sí, con cuidado |
| Una imagen | La imagen | No |
| Otro texto | Su texto | No |
| Cualquier otra cosa | Su nombre, tamaño y tipo | No |

Un archivo de más de 256 KB muestra su primera parte y lo dice.

## Qué te enseña

- Una página organizada en cuadros tiene un `div` con `class="nc-box"` alrededor de cada cuadro.
- Un dibujo es un archivo, `ink.svg`, y el texto de la página lo nombra en una línea.
- Las etiquetas y los enlaces son texto sencillo.

Nada está oculto: si la página lo muestra, hay un archivo en esta carpeta que lo contiene.

Siguiente: [De derecha a izquierda](../right-to-left/).

*Esta página la tradujo un modelo de inteligencia artificial. Las correcciones son bienvenidas.*
