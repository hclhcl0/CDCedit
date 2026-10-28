<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit is een gratis, open-source video-editor en een alternatief voor CapCut op macOS, Windows, Linux en Android. Het doet waarvoor mensen CapCut eigenlijk openen: automatische ondertitels, tekst-naar-spraak, achtergrond verwijderen, keyframe-animatie, effecten en titels, monteren op meerdere sporen, exporteren in 4K. En dat zonder addertjes onder het gras: geen watermerk, geen account, geen abonnement, geen upload.

Alles draait lokaal, op een native Rust-engine met een GPU-compositor. Installeren, beeldmateriaal erin slepen, knippen. De AI-modellen voor ondertitels, stemmen en uitsnijden download je één keer via Instellingen, daarna werken ze offline. Je beeldmateriaal verlaat je schijf nooit.

**Geschikt voor:** TikTok, Reels en Shorts, YouTube-video’s, tutorials en schermopnames, podcastfragmenten, memes.

**Ook voor machines:** een JSON-RPC-API, zodat ook scripts en AI-agents er video mee kunnen knippen.

## Hoogtepunten

- 🚫 **Geen watermerken. Geen account. Geen betaalmuur.** Nooit.
- 🔒 **100% lokaal.** Er wordt niets geüpload. Werkt offline.
- 💬 **Automatische ondertitels.** Lokale Whisper. Kies een modelgrootte en je krijgt opgemaakte ondertitels op de tijdlijn.
- 🗣️ **Tekst-naar-spraak + stemklonen.** Gratis lokale stemmen, of elke stem op basis van een paar seconden opname.
- 🧍 **Achtergrond verwijderen.** Mensen, objecten, of teken het masker zelf.
- 🎞️ **Keyframes.** Positie, schaal, rotatie, dekking, volume, effectparameters. Met ingebouwde curve-editor.
- ✨ **100 effecten, filters en overgangen.** Gerenderd op de GPU, live in het voorbeeld.
- ✂️ **Snel knippen.** Splitsen, trimmen, ripple, samenvoegen, stilstaand beeld, snelheid. Een magnetische tijdlijn als je dat wilt.
- 🎚️ **Meerdere sporen, meerdere tijdlijnen.** Verschillende montages in één project. Overvloeimodi, bijsnijden, spiegelen.
- 📝 **Titels.** Lettertypen, omlijning, schaduw, achtergrondvlak. Presets om mee te beginnen.
- 🎙️ **Stem opschonen met één schakelaar.** Ruis verwijderen, stem verbeteren, luidheid gelijktrekken. Plus chipmunk, robot, telefoon en meer.
- 📤 **Exporteren.** H.264, HEVC, AV1. Tot 4K 60, 10-bit kleur.
- 🦀 **Native Rust-engine.** GPU-compositor, proxy’s, hardwaredecodering. Zelfs 4K scrubt soepel.
- 🤖 **Scriptbaar.** Een JSON-RPC-API waarmee scripts en AI-agents video kunnen knippen. Een build vanaf de broncode voegt gRPC en een opdrachtregeltool toe.
- 🖥️ **macOS, Windows, Linux, Android.** 26 talen. Dezelfde app, dezelfde projectbestanden.

## Downloaden

Twee manieren:

1. **[De website](https://concatenate.pages.dev/#download)** geeft je de juiste build voor je computer. Begin hier.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** heeft elke build voor elk platform, met installers, pakketten en checksums. Voor als je zelf wilt kiezen.

CDCedit is in **bèta**: het werkt, maar er zitten nog ruwe randjes aan. [Laat het weten](https://github.com/jub0t/Concat/issues) als je er een tegenkomt.

**Platforms**

- ✅ **Windows** · x86_64 (ARM-builds zijn gepauzeerd, zie [#326](https://github.com/jub0t/Concat/issues/326)). Een setup en een `.msi`. Als SmartScreen een niet-ondertekende build tegenhoudt: **Meer informatie** › **Toch uitvoeren**
- ✅ **macOS** · Intel en Apple silicon. Als macOS een niet-ondertekende build niet wil openen: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 en ARM. `.deb`, `.rpm`, `.AppImage` en een Arch-pakket. Ook als Flatpak op **[Flatpark](https://flatpark.org/apps/app.concat.editor/)**, een Flatpak-remote uit de community die de x86_64-`.deb` van elke release verpakt en met elke release meegaat
- ✅ **Android** · telefoons en tablets
- ✅ **iOS / iPadOS** · iPhone en iPad, via sideloading

✅ Ondersteund · 🚧 In ontwikkeling · 🧪 Nog te testen

**Systeemvereisten** en de groottes van de optionele modellen staan op [de website](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Bijdragen

> [!IMPORTANT]
> De beste manier om bij te dragen is een build van de [Releases](https://github.com/jub0t/Concat/releases)-pagina te halen en die te gebruiken: zoek uit waar hij stukgaat, en zeg wat er beter kan.
>
> Klaar om code te schrijven? [CONTRIBUTING.md](../CONTRIBUTING.md) beschrijft de installatie, de indeling van de broncode, de controles die je moet draaien en onder welke licentie bijdragen vallen. Stuur je CDCedit aan vanuit een script, een service of een agent? [De ontwikkelaarsdocumentatie](https://concatenate.pages.dev/docs) beschrijft de CDCedit-API en de transports ervan: JSON-RPC en gRPC. In [deze Discussion](https://github.com/jub0t/Concat/discussions/3) werd het project aangekondigd.
> 
> Bijdragers kunnen een `@Contributor`-rol krijgen op de Discord-server: vraag er gewoon om.

## Sterrengeschiedenis

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Bijdragers

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Als je denkt dat iets in deze repository, in de releases ervan, of in een pakket of model dat ermee wordt verspreid inbreuk maakt op jouw auteursrecht, stuur dan een e-mail naar [jub0trd@gmail.com](mailto:jub0trd@gmail.com) met een beschrijving van het werk waarvan je de rechthebbende bent, een link naar de plek waar het inbreukmakende materiaal hier staat, je contactgegevens, een verklaring dat je te goeder trouw meent dat het gebruik niet is toegestaan door jou, je vertegenwoordiger of de wet, en een verklaring, op straffe van meineed, dat de melding juist is en dat je de rechthebbende bent of bevoegd bent namens de rechthebbende op te treden, ondertekend met je handgeschreven of elektronische handtekening. Geldige meldingen worden snel afgehandeld, en iedereen van wie materiaal is verwijderd, kan een tegenmelding (counter-notice) naar hetzelfde adres sturen.
