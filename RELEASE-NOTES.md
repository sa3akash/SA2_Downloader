# Download SA2 1.1.3 — New Modern IDM 7.0 UI, Theme System & Automated Release Builds

## What's new in 1.1.3

### Modern IDM V7.0 Redesign & Official Color Theme
- **Official Brand Color System**:
  - Primary colors: Apple Green (`#60CD38`), Bleu De France (`#2D96F6`), Light Gray (`#F2F2F2`), and Quill Grey (`#A8A8A8`).
  - Secondary accents: Lightning Yellow (`#FFC225`), Dark Orchid (`#9542D6`), Vampire Grey (`#4D4D4D`), and Charcoal (`#333333`).
  - Signature gradients: Spring Breeze (`#4CE888` → `#60CD38`) and Calming Sea (`#56CCF2` → `#2D96F6`).
- **Brand 3D Faceted Logo**: High-fidelity vector rendering of the faceted green triangle-arrow emblem across the title bar, task creation dialogs, and progress monitors.
- **Redesigned Left Sidebar**:
  - Full navigation tree with vibrant category-specific vector icons (Musics `#FF6B6B`, Compressed `#FFC225`, Videos `#2D96F6`, Programs `#9542D6`, Documents `#56CCF2`, APKs `#60CD38`, Images `#22D3EE`).
  - Interactive status views: All Downloads, Downloading, Queued, Paused, Finished, and Unfinished.
  - Dedicated **Disk Space Card** at the bottom: features a smooth radial arc meter gauge, disk utilization percentage (e.g. `90%`), current download directory path, and a one-click **[Disk Cleaner]** gradient pill button that opens the downloads directory in File Explorer.
- **Redesigned Top Toolbar**:
  - Pill-shaped **[+ Add Url]** button featuring a circular cloud badge and a breathing Apple Green accent glow when a downloadable link is copied to the clipboard.
  - Action buttons with clean Material vector icons: `Resume`, `Stop`, `Stop All`, divider, `Delete`, `Options`, `Queues`, `Scheduler`, and `Share`.
- **Download List Table**:
  - 44px comfortable row height with clear visual hierarchy.
  - Two-line filename cell: Line 1 displays the file name in semi-bold text; Line 2 displays the category subtitle in muted tone.
  - Format-specific thumbnail badges for MP4, MP3, ZIP, EXE, APK, PNG, and more.
  - Status column featuring centered percentage and a rounded mini progress bar filled with the Spring Breeze gradient.
- **Status Bar**:
  - Left: Interactive **Select All** checkbox to toggle selection of all visible download items with live active/total counts.
  - Right: Live **Speed Test** throughput meter (`⬇ Speed   ⬆ Speed`) with speed gauge icon.
- **Context Menu**:
  - 14px rounded context menu with grouped actions: Open, Open with..., Open folder, Move, Rename, Resume, Stop, Redownload, Refresh URL, Delete, Properties.

### Multi-File & Playlist Downloader ("New Download Task")
- Modern window header with 3D faceted brand mark.
- Format pill badges for each file (`[WEBM]`, `[MKV]`, `[MP4]`, `[VTT]`).
- Clean table columns: `File Type`, `File Name`, `Download From`, `Size`.
- Flexible save destination controls:
  - `◉ Auto sorting` capsule pill
  - `○ All files to one directory` with path input, browse picker, and real-time `Available: <size>` disk space indicator.
- Action pill buttons: `[Cancel ✕]` and `[Download ⬇]` in Spring Breeze gradient.

### Browser Extension & Floating Video Sniffer
- Floating video sniffer download pill with faceted 3D play icon and close button.
- Two-toned quality capsules: `[WEBM | 1440p]`, `[MKV | 2460p]`, `[MP4 | 1080p]`, `[VTT | EN-US]`.
- Top thumbnail preview banner with video title and formatted file grids.
- Unified popup matching the Spring Breeze and Calming Sea palettes.

---

## Install & Download

All packages are compiled with full optimizations and attached directly to the GitHub Release for download and immediate use:

- **Windows 10/11 (64-bit)**: Download `SA2Downloader-1.1.0-windows-x64-portable.zip` — extract and run `sa2_downloader.exe`. Portable, zero-install, preserves settings and downloads.
- **macOS 12+ (Universal: Apple Silicon & Intel)**: Download `SA2Downloader-1.1.0-macos-Universal.zip` — unzip and move `Download SA2.app` to your Applications folder.
- **Linux (x86_64, glibc 2.28+)**:
  - Ubuntu, Debian, Mint: `SA2Downloader_1.1.0_amd64.deb` (`sudo apt install ./SA2Downloader_1.1.0_amd64.deb`)
  - Fedora, RHEL: `SA2Downloader-1.1.0-1.x86_64.rpm` (`sudo dnf install ./SA2Downloader-1.1.0-1.x86_64.rpm`)
  - Any distribution: `SA2Downloader-1.1.0-x86_64.AppImage` (make executable and run).
- **Browser Extension (1.1.0)**:
  - Chrome / Edge / Brave: Download `SA2Downloader-Chrome-1.1.0.zip`, extract, go to `chrome://extensions`, enable *Developer mode*, click *Load unpacked*, and select the extension folder.
  - Firefox: Download `SA2Downloader-Firefox-1.1.0.xpi`, open `about:debugging#/runtime/this-firefox`, and click *Load Temporary Add-on…*.

## Media helpers

Settings → Plugins downloads and updates **yt-dlp**, **FFmpeg + ffprobe** and a portable
**JavaScript runtime** (Deno) into the app's own folder — no installers, no PATH changes, no sudo.
System copies on PATH or a custom file can be chosen instead.

RAR archives are read with the UnRAR source code (freeware licence, extraction only); see `NOTICE.md`.
