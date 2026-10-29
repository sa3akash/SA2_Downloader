# Third-party notices

Download SA2 is original Rust code written for this project. Its interface
design was inspired by modern download managers. The libraries and media
helpers below are built into or managed by the program; their own licences
apply to them.

## Managed Media Helpers

The program optionally downloads and manages standalone portable helper binaries
into the application's user directory to support high-resolution stream multiplexing
and media extraction.

### yt-dlp — The Unlicense
- Portable media extractor executable fetched from official releases.
- Source: https://github.com/yt-dlp/yt-dlp
- License: The Unlicense (Public Domain)

### FFmpeg and ffprobe — LGPL 2.1+ / GPL 2+
- Static audio/video multiplexing and stream transcoding utilities.
- Source: https://ffmpeg.org / https://github.com/eugeneware/ffmpeg-static
- License: GNU Lesser General Public License (LGPL) version 2.1 or later.
  Shipped as unmodified separate standalone binaries without static linkage into the main executable.

### Deno — MIT License
- Portable JavaScript runtime used for complex player challenge extraction.
- Source: https://github.com/denoland/deno
- License: MIT License

## UnRAR — UnRAR licence (freeware, not open source)

RAR archives are extracted with the UnRAR source code by Alexander Roshal,
built in through the `unrar` / `unrar_sys` crates (MIT / Apache-2.0 for the
Rust wrapper). UnRAR may be used to extract RAR archives free of charge; it
may not be used to re-create the RAR compression algorithm. Download SA2
only extracts. The full licence is in `licenses/UnRAR-license.txt` and ships
next to the program.

- Source: https://www.rarlab.com/rar_add.htm

## sevenz-rust2 — Apache License 2.0

7z archives are extracted with `sevenz-rust2` (© sevenz-rust contributors).

- Source: https://github.com/hasenbanck/sevenz-rust2

## zip — MIT

ZIP archives (and the portable update) use the `zip` crate (© zip-rs
contributors).

## Material Icons font — Apache License 2.0

`assets/fonts/MaterialIcons-Regular.ttf` (© Google) is bundled for UI glyphs.
Redistribution of this font requires keeping this attribution.

- Source: https://github.com/google/material-design-icons
- License: Apache-2.0 — https://www.apache.org/licenses/LICENSE-2.0

## Design inspiration

The layout and usability of this app were inspired by modern desktop
download managers (including the IDM V7.0 Redesign Style Guide, AB Download Manager,
and FluxDown). Inspiration means visual direction and usability ideas only: every line of
code here was written independently for this project, and no source files from those
projects were copied, adapted, or embedded.

## Rust crates

- `sha2` (© RustCrypto contributors) — MIT / Apache-2.0, used for optional
  SHA-256 file-integrity verification.
- `egui_extras` (© egui contributors) — MIT / Apache-2.0, used for the
  download table (`TableBuilder`: aligned headers/rows, resizable columns,
  full-row selection).
