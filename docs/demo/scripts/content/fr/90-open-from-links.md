---
title: Ouvrir une page depuis un lien
---

# Ouvrir une page depuis un lien

## Prévu : des liens qui ouvrent NoteCraft

NoteCraft devrait permettre d'ouvrir une page précise d'un carnet depuis un navigateur, un courriel, une messagerie ou une autre application. Le système d'exploitation utilisera le type de lien `notecraft:` pour lancer NoteCraft et lui indiquer la page à ouvrir. Selon votre système et votre navigateur, une confirmation peut vous être demandée avant l'ouverture de l'application.

Cette fonctionnalité **n'est pas encore disponible**. NoteCraft n'enregistre pas encore `notecraft:` auprès du système d'exploitation et n'ouvre pas la page demandée lorsqu'il reçoit un lien externe. Copier un lien vers une page NoteCraft ne signifie donc pas qu'il est possible de l'ouvrir depuis un navigateur aujourd'hui.

## Enregistrement des liens envisagé

- **Installateur Windows :** enregistrer NoteCraft pendant l'installation et supprimer cet enregistrement lors de la désinstallation.
- **Archive ZIP portable pour Windows :** proposer une action séparée **Enregistrer les liens NoteCraft**, à lancer volontairement, ainsi qu'une action pour annuler l'enregistrement. L'extraction de l'archive ne suffira pas à enregistrer le lien.
- **macOS :** déclarer le type de lien dans le paquet de l'application NoteCraft.
- **Linux :** installer une association d'application de bureau pour ce type de lien.

L'enregistrement indique seulement au système d'exploitation quelle application peut recevoir un lien. NoteCraft devra aussi retrouver le carnet et la page, traiter le lien si l'application est déjà ouverte et expliquer clairement si le carnet ou la page est introuvable. Ces étapes font également partie de la fonctionnalité prévue.

*Cette page a été traduite par un modèle d'IA. Les corrections sont les bienvenues.*
