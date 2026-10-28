<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit je besplatan videouređivač otvorenog koda i alternativa CapCutu za macOS, Windows, Linux i Android. Pokriva ono zbog čega ljudi zapravo otvaraju CapCut: automatske titlove, pretvorbu teksta u govor, uklanjanje pozadine, animaciju ključnim kadrovima, efekte i naslove, višetračno rezanje, izvoz u 4K. Bez ikakvih kvaka: bez vodenog žiga, bez računa, bez pretplate, bez učitavanja.

Sve se izvodi lokalno na nativnom Rust motoru s GPU kompozitorom. Instalirajte, ubacite snimke, režite. AI modeli za titlove, glasove i izrezivanje preuzimaju se jednom iz Postavki i nakon toga rade izvan mreže. Vaše snimke nikada ne napuštaju vaš disk.

**Dobro za:** TikTok, Reels i Shorts, YouTube videe, tutorijale i snimke zaslona, isječke podcasta, memeove.

**I za strojeve:** JSON-RPC API, pa skripte i AI agenti također mogu njime rezati video.

## Istaknuto

- 🚫 **Bez vodenih žigova. Bez računa. Bez naplate.** Nikad.
- 🔒 **100 % lokalno.** Ništa se ne učitava. Radi izvan mreže.
- 💬 **Automatski titlovi.** Lokalni Whisper. Odaberite veličinu modela i dobit ćete stilizirane titlove na vremenskoj crti.
- 🗣️ **Tekst u govor + kloniranje glasa.** Besplatni lokalni glasovi ili bilo koji glas iz nekoliko sekundi snimke.
- 🧍 **Uklanjanje pozadine.** Osobe, predmeti ili sami nacrtajte masku.
- 🎞️ **Ključni kadrovi.** Položaj, skala, rotacija, prozirnost, glasnoća, parametri efekata. Ugrađeni uređivač krivulja.
- ✨ **100 efekata, filtara i prijelaza.** Iscrtano na GPU-u, uživo u pregledu.
- ✂️ **Brzo rezanje.** Podijeli, skrati, ripple, spoji, zamrzni kadar, brzina. Magnetska vremenska crta ako je želite.
- 🎚️ **Više traka, više vremenskih crta.** Više montaža u jednom projektu. Načini stapanja, obrezivanje, zrcaljenja.
- 📝 **Naslovi.** Fontovi, obrub, sjena, pozadinska pločica. Predlošci za početak.
- 🎙️ **Čišćenje glasa jednim prekidačem.** Uklanjanje šuma, poboljšanje glasa, izjednačavanje glasnoće. Plus vjeverica, robot, telefon i društvo.
- 📤 **Izvoz.** H.264, HEVC, AV1. Do 4K 60, 10-bitna boja.
- 🦀 **Nativni Rust motor.** GPU kompozitor, proxyji, hardversko dekodiranje. 4K se pregledava glatko.
- 🤖 **Skriptabilan.** JSON-RPC API kojim skripte i AI agenti mogu rezati video. Inačica izgrađena iz izvornog koda dodaje gRPC i alat za naredbeni redak.
- 🖥️ **macOS, Windows, Linux, Android.** 26 jezika. Ista aplikacija, iste datoteke projekta.

## Preuzimanje

Dva ulaza:

1. **[Web-stranica](https://concatenate.pages.dev/#download)** daje vam pravu inačicu za vaše računalo. Počnite ovdje.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** ima svaku inačicu za svaku platformu, s instalacijama, paketima i kontrolnim zbrojevima. Za kad želite birati.

CDCedit je u **beti**: radi, a još ima rubova. [Javite](https://github.com/jub0t/Concat/issues) kad naiđete na jedan.

**Platforme**

- ✅ **Windows** · x86_64 (ARM inačice su pauzirane, vidi [#326](https://github.com/jub0t/Concat/issues/326)). Instalacija i `.msi`. Ako SmartScreen zaustavi nepotpisanu inačicu: **Više informacija** › **Svejedno pokreni**
- ✅ **macOS** · Intel i Apple Silicon. Ako macOS odbije otvoriti nepotpisanu inačicu: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 i ARM. `.deb`, `.rpm`, `.AppImage` i Arch paket. Također Flatpak na **[Flatparku](https://flatpark.org/apps/app.concat.editor/)**, Flatpak repozitoriju zajednice koji omata x86_64 `.deb` svakog izdanja i ažurira se s njim
- ✅ **Android** · telefoni i tableti
- ✅ **iOS / iPadOS** · iPhone i iPad, sideloadom

✅ Podržano · 🚧 U izradi · 🧪 Za testiranje

**Zahtjevi sustava** i veličine neobaveznih modela nalaze se na [web-stranici](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Kako doprinijeti

> [!IMPORTANT]
> Najbolji način doprinosa jest uzeti inačicu sa stranice [Releases](https://github.com/jub0t/Concat/releases) i koristiti je: pronaći gdje puca i reći gdje bi mogla biti bolja.
>
> Spremni pisati kod? [CONTRIBUTING.md](../CONTRIBUTING.md) pokriva postavljanje, raspored stabla, provjere koje treba pokrenuti i kako se doprinosi licenciraju. Upravljate Concatom iz skripte, servisa ili agenta? [Dokumentacija za razvojne programere](https://concatenate.pages.dev/docs) pokriva CDCedit API i njegove prijenose: JSON-RPC i gRPC. [U ovoj raspravi](https://github.com/jub0t/Concat/discussions/3) projekt je najavljen.
> 
> Suradnici mogu zatražiti ulogu `@Contributor` na Discord poslužitelju: samo pitajte.

## Povijest zvjezdica

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Suradnici

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Ako smatrate da nešto u ovom repozitoriju, njegovim izdanjima ili paketu ili modelu koji distribuira krši vaša autorska prava, pošaljite e-poruku na [jub0trd@gmail.com](mailto:jub0trd@gmail.com) s opisom djela čiji ste vlasnik, poveznicom na mjesto na kojem se ovdje pojavljuje sporni materijal, svojim podacima za kontakt, izjavom da u dobroj vjeri smatrate da tu upotrebu nisu odobrili ni vi, ni vaš zastupnik, ni zakon, te izjavom, pod prijetnjom kaznene odgovornosti za lažni iskaz, da je obavijest točna i da ste vlasnik ili ste ovlašteni djelovati u ime vlasnika, potpisanom vašim vlastoručnim ili elektroničkim potpisom. Valjane obavijesti obrađuju se bez odgode, a svatko čiji je materijal uklonjen može poslati protuobavijest na istu adresu.
