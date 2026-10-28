<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit est un éditeur vidéo gratuit et open source, une alternative à CapCut pour macOS, Windows, Linux et Android. Il couvre ce pour quoi on ouvre vraiment CapCut : sous-titres automatiques, synthèse vocale, suppression d’arrière-plan, animation par images clés, effets et titres, montage multipiste, export 4K. Sans aucun piège : pas de filigrane, pas de compte, pas d’abonnement, pas d’envoi.

Tout tourne en local sur un moteur natif en Rust avec un compositeur GPU. Installez, déposez vos rushes, montez. Les modèles d’IA pour les sous-titres, les voix et le détourage se téléchargent une fois depuis les Réglages et fonctionnent ensuite hors ligne. Vos rushes ne quittent jamais votre disque.

**Idéal pour :** TikTok, Reels et Shorts, vidéos YouTube, tutoriels et enregistrements d’écran, extraits de podcast, mèmes.

**Aussi pour les machines :** une API JSON-RPC, pour que les scripts et les agents IA puissent eux aussi monter de la vidéo avec.

## Points forts

- 🚫 **Pas de filigrane. Pas de compte. Pas de paywall.** Jamais.
- 🔒 **100 % local.** Rien n’est envoyé. Fonctionne hors ligne.
- 💬 **Sous-titres automatiques.** Whisper en local. Choisissez une taille de modèle, obtenez des sous-titres stylés sur la timeline.
- 🗣️ **Synthèse vocale + clonage de voix.** Voix locales gratuites, ou n’importe quelle voix à partir de quelques secondes d’enregistrement.
- 🧍 **Suppression d’arrière-plan.** Personnes, objets, ou peignez le masque vous-même.
- 🎞️ **Images clés.** Position, échelle, rotation, opacité, volume, paramètres d’effets. Éditeur de courbes intégré.
- ✨ **100 effets, filtres et transitions.** Rendus par le GPU, en direct dans l’aperçu.
- ✂️ **Montez vite.** Couper, rogner, ripple, fusionner, arrêt sur image, vitesse. Timeline magnétique si vous la voulez.
- 🎚️ **Multipiste, multi-timeline.** Plusieurs montages dans un projet. Modes de fusion, recadrage, retournements.
- 📝 **Titres.** Polices, contour, ombre, plaque de fond. Des préréglages pour démarrer.
- 🎙️ **Nettoyage de voix d’un seul geste.** Débruiter, rehausser la voix, niveler le volume. Plus écureuil, robot, téléphone et compagnie.
- 📤 **Export.** H.264, HEVC, AV1. Jusqu’à 4K 60, couleur 10 bits.
- 🦀 **Moteur natif en Rust.** Compositeur GPU, proxys, décodage matériel. La 4K se parcourt sans accroc.
- 🤖 **Scriptable.** Une API JSON-RPC avec laquelle les scripts et les agents IA peuvent monter de la vidéo. Une compilation depuis les sources ajoute gRPC et un outil en ligne de commande.
- 🖥️ **macOS, Windows, Linux, Android.** 26 langues. La même appli, les mêmes fichiers de projet.

## Téléchargement

Deux portes d’entrée :

1. **[Le site web](https://concatenate.pages.dev/#download)** vous donne la bonne version pour votre machine. Commencez ici.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** propose chaque version pour chaque plateforme, avec installeurs, paquets et sommes de contrôle. Pour quand vous voulez choisir.

CDCedit est en **bêta** : ça marche, et il reste des angles. [Dites-le](https://github.com/jub0t/Concat/issues) quand vous en trouvez un.

**Plateformes**

- ✅ **Windows** · x86_64 (les versions ARM sont en pause, voir [#326](https://github.com/jub0t/Concat/issues/326)). Un installeur et un `.msi`. Si SmartScreen bloque une version non signée : **Informations complémentaires** › **Exécuter quand même**
- ✅ **macOS** · Intel et Apple Silicon. Si macOS refuse d’ouvrir une version non signée : `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 et ARM. `.deb`, `.rpm`, `.AppImage` et un paquet Arch. Aussi un Flatpak sur **[Flatpark](https://flatpark.org/apps/app.concat.editor/)**, un dépôt Flatpak communautaire qui empaquette le `.deb` x86_64 de chaque version et se met à jour avec elle
- ✅ **Android** · téléphones et tablettes
- ✅ **iOS / iPadOS** · iPhone et iPad, en sideload

✅ Pris en charge · 🚧 En cours · 🧪 À tester

La **configuration requise** et la taille des modèles optionnels sont sur [le site web](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Contribuer

> [!IMPORTANT]
> La meilleure façon de contribuer est de prendre une version sur la page [Releases](https://github.com/jub0t/Concat/releases) et de s’en servir : trouvez où ça casse, et dites où ça pourrait être mieux.
>
> Prêt à écrire du code ? [CONTRIBUTING.md](../CONTRIBUTING.md) couvre l’installation, l’organisation de l’arborescence, les vérifications à lancer et la licence des contributions. Vous pilotez CDCedit depuis un script, un service ou un agent ? [La documentation développeur](https://concatenate.pages.dev/docs) couvre l’API CDCedit et ses transports : JSON-RPC et gRPC. [Cette Discussion](https://github.com/jub0t/Concat/discussions/3) est celle où le projet a été annoncé.
> 
> Les contributeurs peuvent obtenir un rôle `@Contributor` sur le serveur Discord : il suffit de le demander.

## Historique des étoiles

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Contributeurs

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Si vous pensez que quelque chose dans ce dépôt, dans ses versions publiées, ou dans un paquet ou un modèle qu’il distribue porte atteinte à vos droits d’auteur, écrivez à [jub0trd@gmail.com](mailto:jub0trd@gmail.com) en indiquant une description de l’œuvre dont vous êtes titulaire, un lien vers l’endroit où le contenu litigieux apparaît ici, vos coordonnées, une déclaration selon laquelle vous estimez de bonne foi que cet usage n’est autorisé ni par vous, ni par votre mandataire, ni par la loi, et une déclaration, sous peine de parjure, attestant que la notification est exacte et que vous êtes le titulaire des droits ou habilité à agir en son nom, revêtue de votre signature manuscrite ou électronique. Les notifications valides sont traitées sans délai, et toute personne dont le contenu a été retiré peut envoyer une contre-notification à la même adresse.
