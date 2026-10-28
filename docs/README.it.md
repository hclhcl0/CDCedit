<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit è un editor video gratuito e open source, un’alternativa a CapCut per macOS, Windows, Linux e Android. Copre ciò per cui la gente apre davvero CapCut: sottotitoli automatici, sintesi vocale, rimozione dello sfondo, animazione con fotogrammi chiave, effetti e titoli, montaggio multitraccia, esportazione 4K. Senza nessuna fregatura: niente filigrana, niente account, niente abbonamento, niente caricamenti.

Tutto gira in locale su un motore nativo in Rust con un compositore su GPU. Installalo, trascina il girato, taglia. I modelli di IA per sottotitoli, voci e scontorno si scaricano una volta dalle Impostazioni e poi funzionano offline. Il tuo girato non lascia mai il tuo disco.

**Adatto a:** TikTok, Reels e Shorts, video YouTube, tutorial e registrazioni dello schermo, clip di podcast, meme.

**Anche per le macchine:** un’API JSON-RPC, così anche script e agenti IA possono montare video con CDCedit.

## In evidenza

- 🚫 **Niente filigrane. Niente account. Niente paywall.** Mai.
- 🔒 **100 % locale.** Niente viene caricato. Funziona offline.
- 💬 **Sottotitoli automatici.** Whisper in locale. Scegli la dimensione del modello e ottieni sottotitoli stilizzati sulla timeline.
- 🗣️ **Sintesi vocale + clonazione della voce.** Voci locali gratuite, o qualsiasi voce da pochi secondi di registrazione.
- 🧍 **Rimozione dello sfondo.** Persone, oggetti, o dipingi la maschera tu stesso.
- 🎞️ **Fotogrammi chiave.** Posizione, scala, rotazione, opacità, volume, parametri degli effetti. Editor di curve integrato.
- ✨ **100 effetti, filtri e transizioni.** Resi dalla GPU, dal vivo nell’anteprima.
- ✂️ **Taglia in fretta.** Dividi, rifila, ripple, unisci, fermo immagine, velocità. Timeline magnetica se la vuoi.
- 🎚️ **Multitraccia, multi-timeline.** Più montaggi in un progetto. Metodi di fusione, ritaglio, ribaltamenti.
- 📝 **Titoli.** Font, contorno, ombra, placca di sfondo. Preset da cui partire.
- 🎙️ **Pulizia della voce con un interruttore.** Riduci il rumore, migliora la voce, livella il volume. Più scoiattolo, robot, telefono e compagnia.
- 📤 **Esportazione.** H.264, HEVC, AV1. Fino a 4K 60, colore a 10 bit.
- 🦀 **Motore nativo in Rust.** Compositore su GPU, proxy, decodifica hardware. Il 4K scorre fluido.
- 🤖 **Scriptabile.** Un’API JSON-RPC con cui script e agenti IA possono montare video. Compilando dai sorgenti si aggiungono gRPC e uno strumento da riga di comando.
- 🖥️ **macOS, Windows, Linux, Android.** 26 lingue. Stessa app, stessi file di progetto.

## Download

Due strade:

1. **[Il sito web](https://concatenate.pages.dev/#download)** ti dà la build giusta per la tua macchina. Parti da qui.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** ha ogni build per ogni piattaforma, con installer, pacchetti e checksum. Per quando vuoi scegliere.

CDCedit è in **beta**: funziona, e ha ancora qualche spigolo. [Segnalalo](https://github.com/jub0t/Concat/issues) quando ne trovi uno.

**Piattaforme**

- ✅ **Windows** · x86_64 (le build ARM sono in pausa, vedi [#326](https://github.com/jub0t/Concat/issues/326)). Un setup e un `.msi`. Se SmartScreen blocca una build non firmata: **Ulteriori informazioni** › **Esegui comunque**
- ✅ **macOS** · Intel e Apple Silicon. Se macOS si rifiuta di aprire una build non firmata: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 e ARM. `.deb`, `.rpm`, `.AppImage` e un pacchetto Arch. Anche un Flatpak su **[Flatpark](https://flatpark.org/apps/app.concat.editor/)**, un remote Flatpak della community che impacchetta il `.deb` x86_64 di ogni versione e si aggiorna con essa
- ✅ **Android** · telefoni e tablet
- ✅ **iOS / iPadOS** · iPhone e iPad, con sideload

✅ Supportato · 🚧 In lavorazione · 🧪 Da testare

I **requisiti di sistema** e le dimensioni dei modelli opzionali sono sul [sito web](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Come contribuire

> [!IMPORTANT]
> Il modo migliore per contribuire è prendere una build dalla pagina [Releases](https://github.com/jub0t/Concat/releases) e usarla: trova dove si rompe, e di’ dove potrebbe essere migliore.
>
> Pronto a scrivere codice? [CONTRIBUTING.md](../CONTRIBUTING.md) copre la configurazione, la struttura dell’albero, i controlli da eseguire e come vengono licenziati i contributi. Guidi CDCedit da uno script, un servizio o un agente? [La documentazione per sviluppatori](https://concatenate.pages.dev/docs) copre l’API di CDCedit e i suoi trasporti: JSON-RPC e gRPC. [Questa Discussion](https://github.com/jub0t/Concat/discussions/3) è dove il progetto è stato annunciato.
> 
> I contributori possono ottenere un ruolo `@Contributor` sul server Discord: basta chiederlo.

## Storico delle stelle

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Contributori

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Se ritieni che qualcosa in questo repository, nelle sue release o in un pacchetto o modello che distribuisce violi il tuo diritto d’autore, scrivi a [jub0trd@gmail.com](mailto:jub0trd@gmail.com) indicando una descrizione dell’opera di cui sei titolare, un link al punto in cui il materiale contestato compare qui, i tuoi recapiti, una dichiarazione in cui affermi di ritenere in buona fede che l’uso non sia autorizzato da te, dal tuo rappresentante o dalla legge, e una dichiarazione, sotto pena di spergiuro, che la segnalazione è accurata e che sei il titolare o sei autorizzato ad agire per suo conto, con la tua firma autografa o elettronica. Le segnalazioni valide vengono gestite tempestivamente, e chiunque si veda rimuovere del materiale può inviare una contro-notifica allo stesso indirizzo.
