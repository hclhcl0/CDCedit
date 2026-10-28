<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit adalah penyunting video gratis dan sumber terbuka, alternatif CapCut untuk macOS, Windows, Linux, dan Android. Ia mencakup hal-hal yang sebenarnya dicari orang saat membuka CapCut: teks otomatis, teks ke suara, penghapusan latar, animasi keyframe, efek dan judul, pemotongan multi-trek, ekspor 4K. Tanpa jebakan apa pun: tanpa tanda air, tanpa akun, tanpa langganan, tanpa unggahan.

Semuanya berjalan lokal di mesin Rust native dengan kompositor GPU. Pasang, jatuhkan rekaman, potong. Model AI untuk teks, suara, dan pemotongan latar diunduh sekali dari Pengaturan dan setelah itu bekerja luring. Rekaman Anda tidak pernah meninggalkan disk Anda.

**Cocok untuk:** TikTok, Reels, dan Shorts, video YouTube, tutorial dan rekaman layar, klip podcast, meme.

**Juga untuk mesin:** API JSON-RPC, sehingga skrip dan agen AI juga bisa memotong video dengannya.

## Sorotan

- 🚫 **Tanpa tanda air. Tanpa akun. Tanpa paywall.** Selamanya.
- 🔒 **100 % lokal.** Tidak ada yang diunggah. Bekerja luring.
- 💬 **Teks otomatis.** Whisper lokal. Pilih ukuran model, dapatkan teks bergaya di lini masa.
- 🗣️ **Teks ke suara + kloning suara.** Suara lokal gratis, atau suara apa pun dari rekaman beberapa detik.
- 🧍 **Penghapusan latar.** Orang, objek, atau lukis maskernya sendiri.
- 🎞️ **Keyframe.** Posisi, skala, rotasi, opasitas, volume, parameter efek. Penyunting kurva bawaan.
- ✨ **100 efek, filter, dan transisi.** Dirender GPU, langsung di pratinjau.
- ✂️ **Potong cepat.** Belah, pangkas, ripple, gabung, bekukan bingkai, kecepatan. Lini masa magnetik kalau mau.
- 🎚️ **Multi-trek, multi-lini masa.** Beberapa potongan dalam satu proyek. Mode campur, crop, balik.
- 📝 **Judul.** Font, garis tepi, bayangan, pelat latar. Preset untuk memulai.
- 🎙️ **Pembersihan suara satu saklar.** Hilangkan derau, tingkatkan suara, ratakan kenyaringan. Plus tupai, robot, telepon, dan kawan-kawan.
- 📤 **Ekspor.** H.264, HEVC, AV1. Hingga 4K 60, warna 10-bit.
- 🦀 **Mesin Rust native.** Kompositor GPU, proxy, dekode perangkat keras. 4K digeser dengan mulus.
- 🤖 **Bisa diskrip.** API JSON-RPC yang bisa dipakai skrip dan agen AI untuk memotong video. Build dari kode sumber menambahkan gRPC dan alat baris perintah.
- 🖥️ **macOS, Windows, Linux, Android.** 26 bahasa. Aplikasi yang sama, berkas proyek yang sama.

## Unduh

Dua jalan masuk:

1. **[Situs web](https://concatenate.pages.dev/#download)** memberi Anda build yang tepat untuk mesin Anda. Mulai dari sini.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** punya setiap build untuk setiap platform, dengan pemasang, paket, dan checksum. Untuk saat Anda ingin memilih sendiri.

CDCedit masih **beta**: ia bekerja, dan masih ada sisi kasarnya. [Beri tahu](https://github.com/jub0t/Concat/issues) saat Anda menemukannya.

**Platform**

- ✅ **Windows** · x86_64 (build ARM sedang dijeda, lihat [#326](https://github.com/jub0t/Concat/issues/326)). Sebuah setup dan `.msi`. Jika SmartScreen menghentikan build tanpa tanda tangan: **More info** › **Run anyway**
- ✅ **macOS** · Intel dan Apple Silicon. Jika macOS menolak membuka build tanpa tanda tangan: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 dan ARM. `.deb`, `.rpm`, `.AppImage`, dan paket Arch. Juga Flatpak di **[Flatpark](https://flatpark.org/apps/app.concat.editor/)**, remote Flatpak komunitas yang membungkus `.deb` x86_64 setiap rilis dan ikut memperbarui bersamanya
- ✅ **Android** · ponsel dan tablet
- ✅ **iOS / iPadOS** · iPhone dan iPad, lewat sideload

✅ Didukung · 🚧 Dalam pengerjaan · 🧪 Perlu diuji

**Kebutuhan sistem** dan ukuran model opsional ada di [situs web](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Cara berkontribusi

> [!IMPORTANT]
> Cara terbaik berkontribusi adalah mengambil build dari halaman [Releases](https://github.com/jub0t/Concat/releases) dan memakainya: temukan di mana ia rusak, dan katakan di mana ia bisa lebih baik.
>
> Siap menulis kode? [CONTRIBUTING.md](../CONTRIBUTING.md) membahas penyiapan, tata letak pohon, pemeriksaan yang dijalankan, dan bagaimana kontribusi dilisensikan. Mengendalikan CDCedit dari skrip, layanan, atau agen? [Dokumentasi pengembang](https://concatenate.pages.dev/docs) membahas API CDCedit dan transportnya: JSON-RPC dan gRPC. [Diskusi ini](https://github.com/jub0t/Concat/discussions/3) adalah tempat proyek diumumkan.
> 
> Kontributor bisa mendapatkan peran `@Contributor` di server Discord: cukup minta saja.

## Riwayat bintang

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Kontributor

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Jika Anda yakin ada sesuatu di repositori ini, di rilisnya, atau di paket maupun model yang didistribusikannya yang melanggar hak cipta Anda, kirim email ke [jub0trd@gmail.com](mailto:jub0trd@gmail.com) berisi deskripsi karya yang Anda miliki, tautan ke tempat materi yang melanggar itu muncul di sini, detail kontak Anda, pernyataan bahwa Anda dengan iktikad baik meyakini penggunaan tersebut tidak diizinkan oleh Anda, kuasa Anda, atau hukum, serta pernyataan, dengan ancaman sanksi atas sumpah palsu, bahwa pemberitahuan itu akurat dan bahwa Anda adalah pemilik atau berwenang bertindak atas nama pemilik, yang ditandatangani dengan tanda tangan fisik atau elektronik Anda. Pemberitahuan yang sah segera ditindaklanjuti, dan siapa pun yang materinya dihapus dapat mengirim pemberitahuan balasan (counter-notice) ke alamat yang sama.
