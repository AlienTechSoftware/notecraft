---
title: Abrir una página desde un enlace
---

# Abrir una página desde un enlace

## En preparación: enlaces que abren NoteCraft

Está previsto que NoteCraft permita abrir una página concreta del cuaderno desde un navegador, correo, chat u otra aplicación. El sistema operativo usará el tipo de enlace `notecraft:` para iniciar NoteCraft e indicarle qué página abrir. Según el sistema y el navegador, quizá tengas que confirmar que quieres abrir la aplicación.

Esta función **aún no está disponible**. NoteCraft todavía no registra `notecraft:` en el sistema operativo ni abre la página indicada al recibir un enlace externo. Copiar un enlace a una página de NoteCraft no significa que hoy se pueda abrir desde un navegador.

## Cómo está previsto registrar los enlaces

- **Instalador de Windows:** registrará NoteCraft durante la instalación y eliminará el registro al desinstalarlo.
- **ZIP portátil de Windows:** incluirá una acción independiente **Registrar enlaces de NoteCraft**, que podrás ejecutar si lo deseas, y otra para deshacer el registro. Extraer el ZIP no registrará el enlace.
- **macOS:** el tipo de enlace se declarará en el paquete de la aplicación NoteCraft.
- **Linux:** se instalará una asociación de la aplicación de escritorio para ese tipo de enlace.

El registro solo indica al sistema operativo qué aplicación puede recibir el enlace. NoteCraft también debe encontrar el cuaderno y la página, gestionar el enlace si ya está abierto y explicar claramente si no encuentra el cuaderno o la página. Todo esto forma parte de la función prevista.

*Esta página se tradujo con un modelo de IA. Se agradecen las correcciones.*
