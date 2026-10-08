# Mue

Image and video conversion from the Windows 11 context menu, built with Rust, GPUI, and FFmpeg.

## Features

- Direct conversion to JPG, PNG, WebP, MP4 (H.264 / AAC), and WebM (VP9 / Opus).
- A final **Profiles** submenu containing saved image or video conversion profiles.
- GPUI settings for image quality, PNG compression, maximum dimensions, JPEG transparency
  background, video quality and encoding speed, frame rate limit, audio, and metadata.
- One application instance, a Windows tray icon, and a sequential conversion queue.
- A compact, non-focusing progress window on the cursor's monitor, above the taskbar.
- Cancellation, estimated video time remaining, and revealing the output in File Explorer.
- Original files preserved; filename collisions receive a numeric suffix. Results become
  visible only after successful encoding. Cancelled and failed temporary outputs are removed.

This is the first implementation. Functional validation is performed manually by the project owner.

## Requirements

- Windows 11 x64 for the modern Explorer menu and the packaged development build.
- Rust with the `x86_64-pc-windows-msvc` toolchain.
- Visual Studio Build Tools with **Desktop development with C++** and the Windows SDK.
- Windows Developer Mode for local, unsigned application identity registration.

The bundled conversion engine is pinned in [`packaging/ffmpeg.json`](packaging/ffmpeg.json).
Both executables must match that version. The Windows archive has a pinned SHA-256 checksum;
Mue does not silently use an unrelated FFmpeg from `PATH`.

## Run the application

From the repository root in PowerShell:

```powershell
cargo build --workspace --locked
.\scripts\bundle-ffmpeg.ps1
.\target\debug\mue.exe
```

This opens settings and creates the tray icon. Closing settings leaves Mue in the tray.
Use the tray's **Quit Mue** command to stop the application and its conversions.

The standalone application can convert files through **Convert files…**, without registering
the Explorer extension. That action uses the currently edited settings; save them to use the
same settings from Explorer.

Command-line entry points:

```powershell
.\target\debug\mue.exe --background
.\target\debug\mue.exe --settings
.\target\debug\mue.exe --progress
.\target\debug\mue.exe --convert jpg "C:\Pictures\photo.png"
.\target\debug\mue.exe --convert mp4 "C:\Videos\clip.mov" "C:\Videos\other.mkv"
.\target\debug\mue.exe --quit
```

Subsequent launches forward their request to the existing instance. Conversion settings are
snapshotted when a job is queued; editing a profile does not change jobs already in the queue.

## Install the Windows 11 context menu locally

```powershell
.\scripts\build-windows.ps1 -Debug
.\scripts\install-windows.ps1
```

Omit `-Debug` to create a release build. The prepared application lives in `dist/windows-x64`.
The build script embeds the application identity in the copied executable, generates menu/package
icons, and includes the matching FFmpeg and ffprobe binaries and their license information.

The install script registers a sparse package for the current user. It does not change the
default applications for media files. Enable Windows Developer Mode before loose registration.
Reopen File Explorer if Windows has cached the old menu. If it still does not refresh, sign out
and sign back in. Do not move the distribution directory after registration.

For an image, the expected menu is:

```text
Mue
├── Convert to JPG
├── Convert to PNG
├── Convert to WebP
└── Profiles
    └── Your saved image profiles
```

Videos show MP4 and WebM instead. The Profiles entry is disabled when no compatible profile
exists. A selection must contain only supported images or only supported videos; mixed selections
have no compatible conversion and hide Mue. Reopen the menu after saving or deleting a profile.

Windows 11 supplies the application-level Mue group. Each direct format and Profiles is registered
as its own verb; only Profiles enumerates child commands. This avoids the unsupported arrangement
of an IExplorerCommand whose child IExplorerCommands also enumerate children. The exact grouping,
ordering, and Profiles flyout still require manual validation on the target Windows version.

To unregister:

```powershell
.\scripts\uninstall-windows.ps1
```

Quit Mue through the tray before replacing a build. Unregistration preserves the user's settings
and converted files. Windows can keep the extension DLL loaded until its COM host exits, so a
sign-out may be needed before replacing a previously registered DLL.

### Distribution

Local loose registration is a development workflow. Public distribution requires a signed identity
MSIX (or a full MSIX) registered by the installer. The publisher in the signing certificate must
match both `packaging/windows/AppxManifest.xml` and `packaging/windows/mue.manifest`.
The sparse identity files are prepared in `dist/windows-x64/identity`; the application binaries
and FFmpeg remain at the external installation location.

The Gyan Windows FFmpeg build is GPLv3. Keep its included license and build information with
redistributed binaries and satisfy its source-distribution requirements. GPUI and GPUI Component
have their own Apache-2.0 licenses. Application licensing and production signing are release decisions.

## Settings and supported media

User settings are stored in the platform's local data directory under `Mue/profiles.json`
(`%LOCALAPPDATA%\Mue\profiles.json` on Windows). The file is validated and replaced atomically;
an invalid file is reported instead of silently being overwritten.

Image inputs: JPG/JPEG, PNG, WebP, BMP, TIFF. Only still images are in scope; animated inputs are
not offered as separate output formats. Video inputs: MP4/M4V, MOV, MKV, AVI, WebM, MPEG/MPG,
WMV, TS/MTS/M2TS. Decoding also depends on the bundled FFmpeg's capabilities.

- Maximum dimensions preserve aspect ratio and do not upscale. Video dimensions are even for
  H.264/VP9 compatibility. Display rotation is applied by FFmpeg.
- JPEG and WebP quality are lossy quality controls. PNG compression is lossless.
- Lower video CRF means higher quality. MP4 allows 0–51, WebM allows 0–63.
- A frame rate limit only reduces the known source frame rate; it never intentionally increases it.
- Videos use the first video stream and, when enabled, the first audio stream. Subtitles and
  additional audio tracks are not included in this first version. Metadata preservation is best
  effort and depends on the output container; it is not a byte-for-byte metadata copy.
- Outputs go next to the source. If the output already exists, including conversion to the same
  extension, a new name is selected without overwriting. Completed history is limited to 20 jobs
  for the current session. Temporary filenames start with `.mue-`.

## Cross-platform structure

```text
crates/mue-core     Platform-independent profiles, requests, queue, and FFmpeg subprocess engine
crates/mue-app      Shared GPUI settings and progress UI
  src/ipc.rs       Windows named pipes / Unix-domain sockets and a per-user instance lock
  src/platform/    Native desktop integration (Windows implementation)
crates/mue-shell    Windows-only IExplorerCommand COM extension
packaging/         Pinned engine distribution and Windows application identities
scripts/           Build, bundling, and local registration workflows
```

The engine and settings use portable Rust. GUI and Unix IPC code are prepared for Linux and macOS;
their native builds have not been validated yet. On those systems, build `mue-app`, place matching
`ffmpeg` and `ffprobe` executables beside it in an `ffmpeg` directory, and use the settings window
or `--convert` entry point. `scripts/bundle-ffmpeg.sh` copies a supplied matching build.
Follow GPUI's platform build prerequisites (Linux graphics/Wayland/X11 development libraries;
macOS Xcode and Metal tooling).

The tray and Explorer menu are implemented on Windows. macOS Finder extensions and Linux file
manager integrations will be separate platform adapters; no Windows APIs enter the shared engine.
The non-Windows background entry point opens settings because those tray adapters are not yet present.

## Manual validation

Suggested first checks:

1. Open settings, save a custom image/video profile, close settings, and reopen through the tray.
2. Convert a transparent PNG to JPG and check the background and maximum dimensions.
3. Convert an image using a path with spaces and accented characters; repeat the conversion and
   verify that the original and previous outputs remain intact.
4. Convert a portrait video with audio to MP4 and WebM; inspect orientation, dimensions, and audio.
5. Queue several files, cancel a running job and a waiting job, and inspect temporary-file cleanup.
6. Register the Windows 11 menu, try both direct formats and saved profiles, and try multiple selection.
7. Launch another conversion while Mue is in the tray; check that there is only one application instance.
8. Try the progress window on a second monitor with different DPI and taskbar placement.
9. Quit during a conversion and verify that the encoder stops and the unfinished output is removed.

No automated tests are written or run unless explicitly requested. Development verification uses
compilation and formatting; functional fixes follow the owner's feedback.
