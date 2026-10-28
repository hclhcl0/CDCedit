# CDCedit

CDCedit is this fork's product name. The interface, translations, version information, executable (`cdcedit` / `cdcedit.exe`), desktop installers, Android APK and Apple bundles use the new name. The application identifier is `app.cdcedit.editor`; the separate Android native prototype uses `app.cdcedit.editor.spike`.

The vector icon is `assets/cdcedit-icon.svg`. Raster sizes and the Windows ICO are in `assets/icons/cdcedit_*`; Android launcher resources use the same mark. The upstream artwork is retained as historical source material and is not the active app icon.

Cargo package/library names (`concat`, `concat-*`), existing API identifiers, environment variables and the `concat.json` project filename remain compatible. Build the desktop application with the existing package selection, `cargo build -p concat`, from `src/`; the resulting executable is named `cdcedit`.

Settings and downloaded models migrate on first launch from `org.standalones.concat`, or the older `app.concat.editor`, into the new application directory. Existing migration logic leaves a link for media paths referring to the old directory. Portable builds continue using their existing `portable` folder. Project files remain readable.

CDCedit has no separate release repository configured. The app's upstream self-update actions are disabled to prevent installing an original Concat release over this fork. Original model download endpoints remain in use. Upstream source and issue links remain attribution/reference links rather than invented CDCedit destinations.

Original author, copyright, license, third-party notices and upstream trademark policy remain intact. CDCedit is derived from [Concat](https://github.com/jub0t/Concat), and the original AGPL-3.0-or-later licensing continues to apply.

CDCedit 0.2.8 has passed a local Windows x64 build with the quick profile, a dependency check with only Windows directories on PATH, startup through the first rendered frame, GPU initialization and Whisper CPU initialization. The packaged application also launched normally on the build PC. The Android arm64 APK passed the CI 16 KB alignment check and Android API 30/35 emulator startup checks. Cloud Gemini and ElevenLabs calls have not been verified with live API keys. Windows shipping-profile CI and installer packaging are separate from this local quick-profile preview build.
