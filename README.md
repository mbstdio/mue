# Mue

Image and video conversion from the Windows 11 context menu, built with Rust, GPUI, and FFmpeg.

## Features

- Direct conversion to JPG, PNG, WebP, MP4 (H.264 / AAC), MP4 (H.265 / AAC), and WebM (VP9 / Opus).
- A final **Profiles** submenu containing saved image or video conversion profiles.
- GPUI settings for image quality, PNG compression, maximum dimensions, JPEG transparency
  background, video quality, CBR/VBR bitrate control (one or two passes), encoding speed,
  frame rate limit, audio, and metadata.
- H.264 and H.265 (HEVC) MP4 conversions each have their own direct conversion settings.
- Custom output filename templates with prefixes, suffixes, profile names, and source/output dimensions.
- Categorized settings with General, Image, and Video navigation and the application version.
- System/light/dark appearance, English/French interface languages, optional Windows sign-in
  startup, and configurable automatic hiding of completed conversions.
- One application instance, a Windows tray icon, and a sequential conversion queue.
- A compact, non-focusing progress popup on the cursor's monitor, above the taskbar, with an expandable conversion queue. When automatic hiding is enabled, it closes as soon as the queue finishes; errors remain visible. Open Conversions from the tray to view the history.
- Cancellation, estimated video time remaining, and revealing the output in File Explorer.
- Progress popup actions use icons with tooltips. Clear finished conversions removes completed,
  failed, and cancelled entries from the queue history while keeping active jobs and output files.
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
.\target\debug\mue.exe --convert mp4-h265 "C:\Videos\clip.mov"
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
default applications for media files. On repeat installation, it removes the existing registration
for the current user before registering the new manifest, allowing same-version development updates
without error `0x80073CFB`. Settings and converted files are preserved.
Enable Windows Developer Mode before loose registration. The script notifies Explorer to invalidate
its cached associations and context-menu handlers. Close and reopen the menu after installation.
If Windows still shows an old menu, restart **Windows Explorer** from Task Manager; if necessary,
sign out and back in to unload an old COM host. Do not move the distribution directory after registration.

For an image, the expected menu is:

```text
Mue
├── Convert to JPG
├── Convert to PNG
├── Convert to WebP
└── Profiles
    └── Your saved image profiles
```

Videos show direct MP4 (H.264 / AAC), MP4 (H.265 / AAC), and WebM (VP9 / Opus) commands instead.
The Profiles entry is disabled when no compatible profile
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

The settings window uses a compact category sidebar, with the application version pinned at
the bottom. **General** contains appearance, language, startup, conversion-window behavior,
and application/engine information. **Image** and **Video** each contain their direct conversion
defaults and saved profiles. Save, duplicate, delete, and conversion actions remain visible while
the profile editor scrolls.

General preferences are saved immediately. The theme defaults to **System** and follows desktop
appearance changes; **Light** and **Dark** apply to both settings and conversion windows. The
interface defaults to **English**, with **French** and **System** also available. Profile names
are user content and are not translated. On Windows, **Launch at sign-in** registers the current
executable with `--background` in the current user's Run key. Disable the option before moving
or removing that executable; re-enable it from the new installation to update its path.

**Automatically hide completed conversions** defaults to enabled: the conversion window hides
after six seconds once the queue is idle and no jobs have failed. Disable it to retain the window
until **Hide** is clicked. **Show conversions** reopens the current history without starting a job.

Profile edits remain drafts until **Save changes**. Switching categories or profiles preserves
drafts, including incomplete input, for the lifetime of the settings window. Creating, duplicating,
and deleting a profile update the saved profile list immediately. Existing settings files without
general preferences load with the defaults above and preserve their conversion settings.

User settings are stored in the platform's local data directory under `Mue/profiles.json`
(`%LOCALAPPDATA%\Mue\profiles.json` on Windows). The file is validated and replaced atomically;
an invalid file is reported instead of silently being overwritten.

### Output filename templates

Each saved profile and direct conversion default has an **Output filename template** field.
The default, including for existing profiles, is `{filename}.{ext}`. Templates are preserved
in drafts and copied when duplicating a profile. Available variables:

| Variable | Value |
| --- | --- |
| `{filename}` | Original filename without its last extension |
| `{ext}` | Output format extension, without the dot |
| `{profile}` | Profile name (invalid filename characters are replaced with `_`) |
| `{width}` / `{height}` | Actual encoded output dimensions in pixels, after resizing and rotation |
| `{source_width}` / `{source_height}` | Original stream dimensions in pixels, before resizing and rotation |

For example, `{filename}-web.{ext}` produces `image-web.jpg` for a JPG conversion.
`web-{filename}` produces `web-image.jpg`: the output extension is appended when the
rendered name does not already end with it (case-insensitive). An explicitly different
extension also receives the correct output extension, e.g. `image.png.jpg` for JPG.
`{filename}-{width}x{height}.{ext}` can produce `image-1280x720.jpg`. Output dimensions
are read from the encoded temporary file only when requested by the template.

Templates name a file, not a directory. Unknown variables, malformed braces, invalid filename
characters, reserved Windows device names, and names longer than 255 UTF-16 code units are
reported as errors. Substituted values are not interpreted as additional variables. Outputs
remain next to the source; collisions add a numeric suffix before the final extension, such
as `image-web (1).jpg`, without overwriting originals or previous conversions.
The stem is shortened when needed to fit a collision suffix within the filename length limit.
Source-dependent names are fully validated during conversion; fixed names and template syntax
are validated when saving the profile.

Image inputs: JPG/JPEG, PNG, WebP, BMP, TIFF. Only still images are in scope; animated inputs are
not offered as separate output formats. Video inputs: MP4/M4V, MOV, MKV, AVI, WebM, MPEG/MPG,
WMV, TS/MTS/M2TS. Decoding also depends on the bundled FFmpeg's capabilities.

- Maximum dimensions preserve aspect ratio and do not upscale. Video dimensions are even for
  H.264/H.265/VP9 compatibility. Display rotation is applied by FFmpeg.
- JPEG and WebP quality are lossy quality controls. PNG compression is lossless.
- Lower video CRF means higher quality. MP4 allows 0–51, WebM allows 0–63.
- MP4 H.264 (`libx264`) and MP4 H.265 (`libx265`) have separate direct commands and defaults,
  both with AAC audio. H.265 initially uses CRF 28, balanced speed, and 128 kbps AAC. The former
  automatically added **MP4 H.265** saved profile moves to direct conversion settings, preserving
  its edits. Other saved H.265 profiles remain available as custom profiles. Both MP4 variants
  still produce `.mp4` files; `mp4-h265` selects HEVC from the command line.
- Video rate control offers constant quality (CRF), CBR, one-pass VBR, and two-pass VBR.
  Target and maximum video bitrates use kbps (1–1000000) and exclude audio. VBR targets the
  average bitrate, with an optional maximum at or above the target. CBR sets the maximum to
  the target and regulates bitrate around it; it does not guarantee identical instantaneous
  bitrate or add strict CBR padding in MP4.
- H.264/H.265 support an optional maximum in CRF and both VBR modes using a two-second VBV
  buffer (H.264 lossless CRF 0 cannot be capped). VP9 supports a maximum in CBR and two-pass
  VBR only; its VBR maximum constrains average GOP bitrate rather than individual peaks.
  A blank maximum leaves rate control uncapped.
- Two-pass VBR analyzes video without audio in the first pass and writes the final video/audio
  in the second. Progress spans both passes; time remaining is approximate because pass speeds
  differ. Cancelling either pass removes its temporary output and statistics directory.
- A frame rate limit only reduces the known source frame rate; it never intentionally increases it.
- Videos use the first video stream and, when enabled, the first audio stream. Subtitles and
  additional audio tracks are not included in this first version. Metadata preservation is best
  effort and depends on the output container; it is not a byte-for-byte metadata copy.
- Outputs go next to the source. If the output already exists, including conversion to the same
  extension, a new name is selected without overwriting. Completed history is limited to 20 jobs
  for the current session. Temporary filenames start with `.mue-`.

### Manual settings validation

After rebuilding and relaunching Mue:

1. Open each category, resize to the minimum window size, and check scrolling, navigation icons,
   the pinned application version, and the profile action footer.
2. Switch between Light, Dark, and System with settings and conversions open. Change the Windows
   appearance while System is selected and verify both windows and their title bars follow it.
3. Switch English/French/System and verify the settings labels, conversion controls, and tray menu.
   Quit and reopen Mue to verify preference persistence.
4. Enable Launch at sign-in, then sign out and back in. Mue should start in the tray without opening
   settings. Disable the option and check that the next sign-in no longer launches Mue.
5. Edit an image profile without saving, switch to Video and back, and verify the draft is retained.
   Repeat with an incomplete numeric input. Save valid edits, create/duplicate/delete profiles,
   and reopen the Explorer menu to verify the saved lists and conversions.
6. Convert image and video files from settings. Verify the originals remain intact and the results
   use the edited settings. Check automatic hiding with the option enabled and disabled.
7. Check Open settings folder, Show conversions, and Quit Mue from General.
8. In Video, select each MP4 direct conversion and try CRF, CBR, one-pass VBR, and two-pass VBR;
   check mode-dependent fields, bitrate validation, draft retention, and persistence after saving.
   Repeat with WebM, checking that its optional maximum is offered only for two-pass VBR.
9. Convert with the direct **MP4 (H.265 / AAC)** command from settings and Explorer, and verify
   HEVC/AAC playback. Upgrade old settings and check that the initial H.265 profile moves to
   direct defaults while custom profiles retain their edits. Cancel a two-pass conversion during
   each pass and verify no `.mue-` output or `.mue-pass-` directory remains.
10. Check the queue popup: the upward chevron expands it and the downward chevron collapses it.
11. Save and duplicate image/video profiles with `{filename}-web.{ext}` and `web-{filename}`;
    convert from settings and Explorer, restart Mue, and verify persistence and numeric collision
    suffixes. Switch profiles without saving and check that the template draft is retained.
12. Convert resized images and rotated videos with
    `{filename}-{source_width}x{source_height}-to-{width}x{height}-{profile}.{ext}` and compare
    names with the source stream and actual output dimensions. Try profile names containing
    `/`, `:`, or braces, then invalid templates (unknown variables, unmatched braces, paths,
    `CON`, or blank input) and verify clear errors with no overwritten files.

## Cross-platform structure

```text
crates/mue-core     Platform-independent profiles, requests, queue, and FFmpeg subprocess engine
crates/mue-app      Shared GPUI settings and progress UI
  src/ipc.rs       Windows named pipes / Unix-domain sockets and a per-user instance lock
  src/settings.rs  Categorized settings and profile editing
  src/preferences.rs Theme application and interface translations
  src/progress.rs  Conversion history and progress UI
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
