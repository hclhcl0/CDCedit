<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit is a customized video editor derived from [Concat](https://github.com/jub0t/Concat). Original author and license notices are retained. Build this fork to use CDCedit features; upstream release links below refer to the original project.

CDCedit es un editor de vídeo gratuito y de código abierto, una alternativa a CapCut para macOS, Windows, Linux y Android. Cubre aquello para lo que la gente realmente abre CapCut: subtítulos automáticos, texto a voz, eliminación de fondo, animación por fotogramas clave, efectos y títulos, corte multipista, exportación 4K. Sin trampas: sin marca de agua, sin cuenta, sin suscripción, sin subir nada.

Todo corre en local sobre un motor nativo en Rust con un compositor por GPU. Instálalo, suelta el material, corta. Los modelos de IA para subtítulos, voces y recorte se descargan una vez desde Ajustes y después funcionan sin conexión. Tu material nunca sale de tu disco.

**Bueno para:** TikTok, Reels y Shorts, vídeos de YouTube, tutoriales y grabaciones de pantalla, clips de pódcast, memes.

**También para máquinas:** una API JSON-RPC, para que los scripts y los agentes de IA también puedan cortar vídeo con él.

## Lo más destacado

- 🚫 **Sin marcas de agua. Sin cuenta. Sin muro de pago.** Nunca.
- 🔒 **100 % local.** Nada se sube. Funciona sin conexión.
- 💬 **Subtítulos automáticos.** Whisper en local. Elige un tamaño de modelo y obtén subtítulos con estilo en la línea de tiempo.
- 🗣️ **Texto a voz + clonación de voz.** Voces locales gratuitas, o cualquier voz a partir de unos segundos de grabación.
- 🧍 **Eliminación de fondo.** Personas, objetos, o pinta la máscara tú mismo.
- 🎞️ **Fotogramas clave.** Posición, escala, rotación, opacidad, volumen, parámetros de efectos. Editor de curvas integrado.
- ✨ **100 efectos, filtros y transiciones.** Renderizados por GPU, en vivo en la vista previa.
- ✂️ **Corta rápido.** Dividir, recortar, ripple, unir, congelar fotograma, velocidad. Línea de tiempo magnética si la quieres.
- 🎚️ **Multipista, multilínea de tiempo.** Varios montajes en un proyecto. Modos de fusión, recorte, volteos.
- 📝 **Títulos.** Fuentes, trazo, sombra, placa de fondo. Preajustes para empezar.
- 🎙️ **Limpieza de voz con un interruptor.** Reducir ruido, realzar la voz, nivelar el volumen. Más ardilla, robot, teléfono y compañía.
- 📤 **Exportación.** H.264, HEVC, AV1. Hasta 4K 60, color de 10 bits.
- 🦀 **Motor nativo en Rust.** Compositor por GPU, proxies, decodificación por hardware. El 4K se desplaza con fluidez.
- 🤖 **Programable.** Una API JSON-RPC con la que los scripts y los agentes de IA pueden cortar vídeo. Compilando desde el código fuente se añaden gRPC y una herramienta de línea de comandos.
- 🖥️ **macOS, Windows, Linux, Android.** 26 idiomas. La misma app, los mismos archivos de proyecto.

## Descarga

Dos formas de entrar:

1. **[El sitio web](https://concatenate.pages.dev/#download)** te da la compilación adecuada para tu máquina. Empieza aquí.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** tiene todas las compilaciones para todas las plataformas, con instaladores, paquetes y sumas de verificación. Para cuando quieras elegir.

CDCedit está en **beta**: funciona, y aún tiene aristas. [Cuéntalo](https://github.com/jub0t/Concat/issues) cuando encuentres una.

**Plataformas**

- ✅ **Windows** · x86_64 (las compilaciones para ARM están en pausa, ver [#326](https://github.com/jub0t/Concat/issues/326)). Un instalador y un `.msi`. Si SmartScreen detiene una compilación sin firmar: **Más información** › **Ejecutar de todas formas**
- ✅ **macOS** · Intel y Apple Silicon. Si macOS se niega a abrir una compilación sin firmar: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 y ARM. `.deb`, `.rpm`, `.AppImage` y un paquete para Arch. También un Flatpak en **[Flatpark](https://flatpark.org/apps/app.concat.editor/)**, un remoto Flatpak comunitario que envuelve el `.deb` x86_64 de cada versión y se actualiza con ella
- ✅ **Android** · teléfonos y tabletas
- ✅ **iOS / iPadOS** · iPhone y iPad, por sideload

✅ Compatible · 🚧 En desarrollo · 🧪 Por probar

Los **requisitos del sistema** y los tamaños de los modelos opcionales están en [el sitio web](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Cómo contribuir

> [!IMPORTANT]
> La mejor forma de contribuir es descargar una compilación de la página de [Releases](https://github.com/jub0t/Concat/releases) y usarla: encuentra dónde falla y di dónde podría ser mejor.
>
> ¿Listo para escribir código? [CONTRIBUTING.md](../CONTRIBUTING.md) cubre la configuración, la disposición del árbol, las comprobaciones que ejecutar y cómo se licencian las contribuciones. ¿Controlas CDCedit desde un script, un servicio o un agente? [La documentación para desarrolladores](https://concatenate.pages.dev/docs) cubre la API de CDCedit y sus transportes: JSON-RPC y gRPC. [Esta Discussion](https://github.com/jub0t/Concat/discussions/3) es donde se anunció el proyecto.
> 
> Los colaboradores pueden reclamar un rol `@Contributor` en el servidor de Discord: solo hay que pedirlo.

## Historial de estrellas

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Colaboradores

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Si crees que algo de este repositorio, de sus versiones publicadas o de un paquete o modelo que distribuye infringe tus derechos de autor, escribe a [jub0trd@gmail.com](mailto:jub0trd@gmail.com) con una descripción de la obra que te pertenece, un enlace al lugar donde aparece aquí el material infractor, tus datos de contacto, una declaración de que crees de buena fe que el uso no está autorizado por ti, por tu representante ni por la ley, y una declaración, bajo pena de perjurio, de que la notificación es exacta y de que eres el titular o estás autorizado para actuar en su nombre, firmada con tu firma física o electrónica. Las notificaciones válidas se atienden con prontitud, y cualquier persona cuyo material sea retirado puede enviar una contranotificación a la misma dirección.
