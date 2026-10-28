<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit macOS, Windows, Linux və Android üçün pulsuz, açıq mənbəli video redaktorudur və CapCut-a alternativdir. İnsanların CapCut-dan ən çox istifadə etdiyi imkanların hamısı burada var: avtomatik subtitrlər, mətnin səsləndirilməsi, fonun silinməsi, açar kadrlarla animasiya, effektlər və yazılar, bir neçə trekdə montaj, 4K ixrac. Üstəlik heç bir gizli şərt yoxdur: nə su nişanı, nə hesab, nə abunəlik, nə də faylların serverə yüklənməsi.

Hər şey Rust ilə yazılmış öz mühərriki üzərində, birbaşa kompüterinizdə işləyir; görüntünün birləşdirilməsi (kompozitinq) GPU-da aparılır. Proqramı quraşdırın, videonuzu əlavə edin və montaja başlayın. Subtitr, səsləndirmə və fon silmə üçün süni intellekt modelləri Parametrlər bölməsindən bir dəfə cihazınıza endirilir, sonra internetsiz işləyir. Videolarınız heç vaxt kompüterinizdən kənara çıxmır.

**Nə üçün uyğundur:** TikTok, Reels və Shorts, YouTube videoları, dərslər və ekran yazıları, podkastlardan kəsiklər, memlər.

**Avtomatlaşdırma üçün də:** süni intellekt agentləri üçün daxili MCP serveri (`cdcedit --mcp`, [quraşdırma](MCP.md)) və skriptlər üçün JSON-RPC API. Belə ki, onlar da CDCedit ilə video montaj edə bilər.

## Əsas imkanlar

- 🚫 **Su nişanı yoxdur. Hesab tələb olunmur. Pullu məhdudiyyət yoxdur.** Heç vaxt.
- 🔒 **100% lokal.** Heç bir fayl serverə göndərilmir. İnternetsiz işləyir.
- 💬 **Avtomatik subtitrlər.** Kompüterinizdə işləyən Whisper. Model ölçüsünü seçin, zaman xəttində hazır üslubu olan subtitrlər alın.
- 🗣️ **Mətnin səsləndirilməsi və səs klonlama.** Pulsuz lokal səslər, ya da cəmi bir neçə saniyəlik səs yazısından klonlanmış istənilən səs.
- 🧍 **Fonun silinməsi.** İnsanları və obyektləri avtomatik ayırır, istəsəniz maskanı özünüz çəkə bilərsiniz.
- 🎞️ **Açar kadrlar.** Mövqe, miqyas, fırlanma, görünürlük, səs səviyyəsi və effekt parametrləri. Daxili əyri redaktoru ilə.
- ✨ **100 effekt, filtr və keçid.** GPU-da render olunur, önizləmədə dərhal görünür.
- ✂️ **Sürətli montaj.** Bölmə, kəsib qısaltma, boşluğu avtomatik bağlayan ripple redaktə, birləşdirmə, kadrı dondurma, sürət. İstəsəniz, maqnitli zaman xətti də var.
- 🎚️ **Bir neçə trek, bir neçə zaman xətti.** Bir layihədə bir neçə montaj variantı. Qarışdırma rejimləri, kəsib çərçivələmə, çevirmə.
- 📝 **Yazılar.** Şriftlər, kontur, kölgə, fon lövhəsi. Başlamaq üçün hazır şablonlar.
- 🎙️ **Bir düymə ilə səs təmizləmə.** Küyü azaldır, nitqi aydınlaşdırır, səs gücünü bərabərləşdirir. Üstəlik sincab, robot, telefon və digər səs effektləri.
- 📤 **İxrac.** H.264, HEVC, AV1. 4K 60 kadr/san-a qədər, 10-bit rəng.
- 🦀 **Rust ilə yazılmış öz mühərriki.** GPU-da kompozitinq, proxy fayllar, aparat səviyyəsində dekodlama. 4K videolarda da zaman xətti boyunca axıcı hərəkət edin.
- 🤖 **Skriptlərlə idarə oluna bilər.** Daxili MCP serveri sayəsində Claude və digər süni intellekt agentləri CDCedit ilə montaj edə bilər ([bir sətirlə quraşdırılır](MCP.md)), skriptlər üçün isə JSON-RPC API var. Mənbə kodundan yığılan versiyaya əlavə olaraq gRPC və komanda sətri aləti daxildir.
- 🖥️ **macOS, Windows, Linux, Android.** 26 dil. Eyni proqram, eyni layihə faylları.

## Yükləmə

CDCedit-ı iki yolla əldə edə bilərsiniz:

1. **[Veb sayt](https://concatenate.pages.dev/#download)** cihazınıza uyğun versiyanı avtomatik təklif edir. Buradan başlamağınız tövsiyə olunur.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** bütün platformalar üçün bütün versiyaları quraşdırıcılar, paketlər və yoxlama cəmləri (checksum) ilə birlikdə təqdim edir. Versiyanı özünüz seçmək istəyirsinizsə, bura baxın.

CDCedit hələ **beta** mərhələsindədir: işləyir, amma hələ də bəzi çatışmazlıqları var. Problemlə qarşılaşsanız, [bizə bildirin](https://github.com/jub0t/Concat/issues).

**Platformalar**

- ✅ **Windows** · x86_64 (ARM versiyalarının buraxılışı müvəqqəti dayandırılıb, bax: [#326](https://github.com/jub0t/Concat/issues/326)). Quraşdırıcı və `.msi` paketi. SmartScreen imzasız versiyanın açılmasının qarşısını alsa: **More info** › **Run anyway**
- ✅ **macOS** · Intel və Apple silicon. macOS imzasız versiyanı açmaq istəməsə: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 və ARM. `.deb`, `.rpm`, `.AppImage` və Arch paketi. Bundan əlavə, Flatpak versiyası da var: o, **[Flatpark](https://flatpark.org/apps/app.concat.editor/)** icma deposundadır. Bu depo hər buraxılışın x86_64 `.deb` paketini Flatpak formatına salır və onunla birlikdə yenilənir
- ✅ **Android** · telefonlar və planşetlər
- ✅ **iOS / iPadOS** · iPhone və iPad, App Store-dan kənar quraşdırma (sideload) ilə

✅ Dəstəklənir · 🚧 Üzərində işlənir · 🧪 Test olunmalıdır

**Sistem tələbləri** və əlavə model ölçüləri haqqında məlumat [veb saytdadır](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Layihəyə necə töhfə vermək olar

> [!IMPORTANT]
> Ən faydalı töhfə [Releases](https://github.com/jub0t/Concat/releases) səhifəsindən son versiyanı yükləyib istifadə etməkdir: rastlaşdığınız xətaları bildirin və nəyin daha yaxşı ola biləcəyini təklif edin.
>
> Kod yazmağa hazırsınız? [CONTRIBUTING.md](../CONTRIBUTING.md) mühitin qurulmasını, layihənin fayl strukturunu, işə salınmalı olan yoxlamaları və töhfələrin hansı lisenziya ilə qəbul edildiyini izah edir. CDCedit-ı skriptdən, servisdən və ya agentdən idarə etmək istəyirsiniz? [Developer sənədləri](https://concatenate.pages.dev/docs) CDCedit API-ni və onun əlaqə protokollarını, yəni JSON-RPC və gRPC-ni izah edir; [docs/MCP.md](MCP.md) isə süni intellekt agentini MCP vasitəsilə necə qoşmağı göstərir. Layihə ilk dəfə [bu müzakirədə](https://github.com/jub0t/Concat/discussions/3) elan olunub.
> 
> Töhfə verənlər Discord serverində `@Contributor` rolu ala bilər: sadəcə bunu istəyin.

## Ulduzların tarixçəsi

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Töhfə verənlər

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Bu depoda, onun buraxılışlarında və ya onun yaydığı hər hansı paket və ya modeldə müəllif hüququnuzun pozulduğunu düşünürsünüzsə, [jub0trd@gmail.com](mailto:jub0trd@gmail.com) ünvanına e-poçt göndərin. Müraciətdə aşağıdakılar olmalıdır: sizə məxsus əsərin təsviri; hüququ pozan materialın burada yerləşdiyi yerə keçid; əlaqə məlumatlarınız; bu istifadəyə sizin, nümayəndənizin və ya qanunun icazə vermədiyinə vicdanla inandığınızı bildirən bəyanat; həmçinin bildirişin doğru olduğunu və hüquq sahibi olduğunuzu və ya onun adından çıxış etməyə səlahiyyətiniz olduğunu yalan ifadəyə görə məsuliyyəti qəbul edərək təsdiq edən bəyanat. Müraciət fiziki və ya elektron imzanızla imzalanmalıdır. Düzgün tərtib olunmuş bildirişlərə tez bir zamanda baxılır. Materialı silinən şəxs eyni ünvana əks-bildiriş göndərə bilər.
