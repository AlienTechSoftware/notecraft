---
title: Sincronizar y copias de seguridad
---

# Sincronizar y copias de seguridad

NoteCraft no tiene sincronización propia ni cuenta. Un cuaderno es una carpeta de archivos
pequeños, así que pon la carpeta de cuadernos donde tu herramienta de sincronización ya mira:
OneDrive, Dropbox, Google Drive, iCloud Drive, Syncthing, una carpeta compartida de red o un
repositorio git. Ninguno necesita entender NoteCraft. Copian archivos.

## Por qué funciona

- **Un archivo por página.** Editar una página reescribe un archivo pequeño, así que dos
  máquinas rara vez tocan los mismos bytes.
- **Escrituras atómicas.** Una página se escribe en un archivo temporal que luego sustituye al
  antiguo, así que un cliente de sincronización nunca ve una página a medio escribir.
- **Nombres cortos y seguros.** Los nombres de carpeta tienen como mucho 64 caracteres, las
  rutas 255, y nunca se escribe nada que Windows rechace.
- **Las copias en conflicto se detectan.** Cuando una herramienta deja una copia en conflicto
  junto a una página, el comprobador lo informa: **Archivo**, luego **Comprobar**, o
  `notecraft-cli check` desde la terminal.

## Copia de seguridad

Una copia de seguridad es una copia de la carpeta. No hay nada más que exportar.

## Lo que no se maneja

NoteCraft no fusiona dos versiones de una página; tu herramienta crea una copia en conflicto y
tú decides. No tiene historial de páginas propio: usa una carpeta sincronizada que conserve
versiones, o git.

Siguiente: [Hazlo tuyo](../make-it-yours/).

*Esta página la tradujo un modelo de inteligencia artificial. Las correcciones son bienvenidas.*
