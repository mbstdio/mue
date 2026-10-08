#!/usr/bin/env sh
set -eu

if [ "$#" -ne 2 ]; then
    echo "Usage: scripts/bundle-ffmpeg.sh <directory-with-ffmpeg-and-ffprobe> <destination>" >&2
    exit 1
fi
source_directory=$1
destination=$2
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
version=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "$root/packaging/ffmpeg.json")
for tool in ffmpeg ffprobe; do
    case $("$source_directory/$tool" -version | head -n 1) in
        "$tool version $version "*|"$tool version $version-"*) ;;
        *) echo "$tool must match the pinned FFmpeg version $version" >&2; exit 1 ;;
    esac
done
mkdir -p "$destination"
cp "$source_directory/ffmpeg" "$source_directory/ffprobe" "$destination/"
cp "$root/packaging/ffmpeg.json" "$destination/build.json"
echo "Bundled FFmpeg $version. Include the build's license and redistribution information when packaging."
