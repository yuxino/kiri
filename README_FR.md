<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri">
  <h1>Kiri</h1>
  <p>Captures d’écran, reconnaissance de texte et enregistrement d’écran. Vos captures restent locales.</p>
</div>

[简体中文](README.md) · [English](README_EN.md) · [繁體中文](README_ZH_TW.md) · [日本語](README_JA.md) · [Deutsch](README_DE.md) · [한국어](README_KO.md) · **Français**

[Site web](https://kiri.yuxino.cn) · [Télécharger la dernière version](https://github.com/yuxino/kiri/releases/latest)

Kiri est une application de capture et d’enregistrement d’écran pour macOS, Windows et Linux. Annotez vos captures, copiez du texte depuis des images et conservez vos images et vidéos sur votre ordinateur.

![Interface d’annotation de Kiri](docs/assets/readme-preview.png)

## Fonctions

- Capturez une fenêtre ou une zone, recadrez-la, ajoutez des dessins, du texte ou une mosaïque, et épinglez les captures comme références flottantes.
- Copiez le texte avec la reconnaissance locale et lisez les codes QR des images.
- Enregistrez un MP4 avec le son du système et le microphone en option, ou créez un GIF sans son. La conversion des vidéos enregistrées en GIF vérifie d’abord le décodage et peut être annulée pendant la vérification ou la conversion. macOS affiche la progression par image et conserve les détails des erreurs.
- Coupez et réorganisez les clips d’une même vidéo, puis exportez un nouveau MP4.
- Recherchez, étiquetez, ajoutez aux favoris et exportez vos captures locales. Récupérez les suppressions accidentelles dans la corbeille.

Sélectionnez une annotation existante pour modifier son style. La mosaïque propose le dessin libre, le rectangle et l’ellipse, avec pixellisation ou flou et intensité réglable. Filigrane (W) permet de saisir un motif répété directement sur l’image et de régler l’opacité, l’angle et l’espacement. Sélectionnez à nouveau l’outil pour modifier le texte existant, même après enregistrement.

## Premiers pas

1. Appuyez sur `⇧⌘A` sous macOS ou `Shift+Ctrl+A` sous Windows / Linux X11, puis sélectionnez une fenêtre ou faites glisser une zone.
2. Choisissez Capture d’écran, Enregistrer ou OCR.
3. Appuyez sur `Entrée` ou double-cliquez sur une partie sans annotation de la sélection pour terminer une capture ; `Échap` annule. Pendant l’annotation, revenez d’abord à l’outil de sélection. Un double-clic sur du texte sert toujours à le modifier. Les captures sont copiées dans le presse-papiers et enregistrées dans la bibliothèque. L’icône d’épingle de la barre d’outils enregistre et épingle la capture en un clic. « Épingler » reste disponible sur la carte de fin. La référence sans bordure se déplace par glissement et se redimensionne proportionnellement depuis son coin inférieur droit. Survolez-la pour afficher les boutons de désépinglage et de fermeture.

En mode Capture, survolez l’écran pour agrandir les pixels d’origine et voir leurs coordonnées et leur couleur HEX. `⌘C` sur macOS ou `Ctrl+C` copie la couleur tout en conservant la sélection. La loupe disparaît pendant l’annotation.

Avant d’annoter ou d’enregistrer, cliquez sur le bouton des curseurs pour saisir la largeur et la hauteur en pixels directement au bord de la sélection.

« Réglages → Général → Langue » propose sept langues d’interface. Votre choix s’applique à toutes les fenêtres de Kiri et reste mémorisé après un redémarrage. Au premier lancement, Kiri suit la langue du système.

Les captures restent locales. La reconnaissance distante est facultative et demande confirmation avant chaque envoi. La configuration Linux, l’enregistrement audio MP4, les raccourcis Wayland et les limites sont décrits dans le [guide Linux](docs/linux.md).

[Installation et aide](docs/usage.fr.md) · [Montage vidéo](docs/video-editing.fr.md) · [Signaler un problème](https://github.com/yuxino/kiri/issues) · [Contribuer](CONTRIBUTING.md)

## Contributeurs

Merci à toutes les personnes qui écrivent du code, signalent des problèmes, essaient Kiri ou le partagent.

Merci en particulier à [@kerwin2046](https://github.com/kerwin2046) pour la [prise en charge initiale de Linux](https://github.com/yuxino/kiri/pull/20), et à [@LLLin000](https://github.com/LLLin000) pour la [correction de l’alignement des captures avec la mise à l’échelle du texte Windows](https://github.com/yuxino/kiri/pull/61).

[Tous les contributeurs](https://github.com/yuxino/kiri/graphs/contributors)

## Communauté

Merci également aux communautés [V2EX](https://www.v2ex.com/), [LINUX DO](https://linux.do/), [Appinn](https://www.appinn.com/) et [NodeLoc](https://www.nodeloc.com/) pour leurs essais, retours et partages.

[MIT](LICENSE) © 2026 yuxino
