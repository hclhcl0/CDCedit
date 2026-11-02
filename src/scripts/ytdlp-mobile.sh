#!/usr/bin/env bash
# Fetches youtubedl-android, python 3.11, quickjs, and ffmpeg runtime libraries for Android.
#
#   scripts/ytdlp-mobile.sh aarch64-linux-android [out-dir]
#   scripts/ytdlp-mobile.sh x86_64-linux-android  [out-dir]
#
# Places 16 KB page-aligned ELF binaries into jniLibs:
#   libpython.so, libqjs.so, libffmpeg.so, libffprobe.so
# And non-ELF zip assets into concat-android/assets:
#   ytdlp, python.zip
set -euo pipefail

target=${1:?target triple: aarch64-linux-android or x86_64-linux-android}
workspace=$(cd "$(dirname "$0")/.." && pwd)
out=${2:-$workspace/vendor/sherpa-onnx/jniLibs}
assets="$workspace/crates/concat-android/assets"

case "$target" in
  aarch64-linux-android)
    abi=arm64-v8a
    ;;
  x86_64-linux-android)
    abi=x86_64
    ;;
  *)
    echo "target $target does not use Android yt-dlp"
    exit 0
    ;;
esac

dest="$out/$abi"
mkdir -p "$dest"
mkdir -p "$assets"

tmp=${RUNNER_TEMP:-$workspace/target}/ytdlp-mobile-$abi
rm -rf "$tmp"
mkdir -p "$tmp"

echo "Fetching youtubedl-android runtime for $abi"
curl -fsSL --retry 5 --retry-all-errors -o "$tmp/library.aar" \
  "https://repo1.maven.org/maven2/io/github/junkfood02/youtubedl-android/library/0.18.1/library-0.18.1.aar"
curl -fsSL --retry 5 --retry-all-errors -o "$tmp/ffmpeg.aar" \
  "https://repo1.maven.org/maven2/io/github/junkfood02/youtubedl-android/ffmpeg/0.18.1/ffmpeg-0.18.1.aar"

unzip -qo "$tmp/library.aar" "jni/$abi/*" "res/raw/ytdlp" -d "$tmp/lib_extracted"
unzip -qo "$tmp/ffmpeg.aar" "jni/$abi/*" -d "$tmp/ff_extracted"

# 16 KB page-aligned native ELF binaries into jniLibs
cp "$tmp/lib_extracted/jni/$abi/libpython.so" "$dest/"
cp "$tmp/lib_extracted/jni/$abi/libqjs.so" "$dest/"
cp "$tmp/ff_extracted/jni/$abi/libffmpeg.so" "$dest/"
cp "$tmp/ff_extracted/jni/$abi/libffprobe.so" "$dest/"

# Non-ELF zip archives into assets
cp "$tmp/lib_extracted/res/raw/ytdlp" "$assets/ytdlp"
cp "$tmp/lib_extracted/jni/$abi/libpython.zip.so" "$assets/python.zip"

rm -rf "$tmp"
echo "Android native binaries installed to $dest:"
ls -lh "$dest"
echo "Assets installed to $assets:"
ls -lh "$assets"
