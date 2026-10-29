# Download SA2 1.1.8 — Ultra HD 2K/4K/8K Dynamic Downloader, Single Instant Loading & Daily Background Helper Updates

## What's new in 1.1.8

### Ultra HD (2K, 4K, 8K) Dynamic Downloads & Universal Site Support
- **Fixed 360p Resolution Lock**: Resolved the issue where higher video qualities reverted to low-resolution streams. The downloader now honors and delivers full Ultra HD streams (8K 4320p, 4K 2160p, 2K 1440p, 1080p Full HD, 60fps AV1/VP9).
- **Universal Multi-Site Compatibility**: Dynamic stream extraction now works seamlessly across millions of websites, including **YouTube**, **Facebook**, **Vimeo**, **BongoBD**, **JustWatch**, **Bilibili**, **TikTok**, and any site utilizing DASH or HLS manifests.
- **Browser Cookie Session Forwarding**: Added configurable browser cookie extraction (Chrome, Edge, Firefox, Brave, Opera, Vivaldi) to bypass bot verification and HTTP 429 Too Many Requests errors.

### Single Instant IDM-Style Loading (0ms Delay)
- **Zero-Latency Format & Size Resolution**: Video formats and duration-calculated file sizes are pre-extracted by the browser extension and cached in memory. Clicking the download pill opens the confirmation dialog instantly with accurate sizes without 5–10s blocking probes.
- **Duplicate Confirm Popup Prevention**: Added native host request deduplication to prevent multiple popups when clicking or navigating media players.

### Smooth Progress Monitor & 60 FPS Visual Interpolation
- **Locked Dialog Dimensions**: Eliminated progress window vibration, height jumping, and resizing oscillation.
- **Smooth 60 FPS Lerp Progress Bar**: Smooth linear interpolation for real-time progress bar rendering.
- **Clean Connection State**: Replaced preliminary placeholder rows with a single clear connection status (`# 1 | Connecting to stream...`) during stream handshakes.

### Playlist Dynamic Quality & Audio Inheritance
- Selecting **Audio** or a specific **Video Quality** dynamically propagates across all batch items in the playlist downloader, automatically setting file extensions (e.g. `.mp3`) and download modes.

### Automatic Daily Background Updates (`yt-dlp` & `FFmpeg`)
- **Daily Silent Check**: Automatically checks GitHub releases for newer `yt-dlp` and `FFmpeg` builds once every 24 hours in a non-blocking background thread.
- **Atomic Safe Replacement**: Updates download to a sidecar file, verifies binary integrity, and atomically swaps the executable without interrupting active downloads.
- **Activity Log Reporting**: Displays auto-update status directly in the main activity log (`Auto-update: Updated yt-dlp...`).
- **User Preference Toggle**: Configure daily auto-updates under **Settings → Plugins** and **Settings → Updates** (enabled by default).
- **Manual Forced Check**: "Check for updates" in the About window and Settings triggers both application and helper update checks simultaneously.

### CI & Build Pipeline Enhancements
- Synchronized package version in `Cargo.lock` with `Cargo.toml`.
- Improved CI portable packaging workflow resilience across Windows, macOS, and Linux.

---

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

- **Windows 10/11 (64-bit)**: Download `SA2Downloader-1.1.6-windows-x64-portable.zip` — extract and run `sa2_downloader.exe`. Portable, zero-install, preserves settings and downloads.
- **macOS 12+ (Universal: Apple Silicon & Intel)**: Download `SA2Downloader-1.1.6-macos-Universal.zip` — unzip and move `Download SA2.app` to your Applications folder.
- **Linux (x86_64, glibc 2.28+)**:
  - Ubuntu, Debian, Mint: `SA2Downloader_1.1.6_amd64.deb` (`sudo apt install ./SA2Downloader_1.1.6_amd64.deb`)
  - Fedora, RHEL: `SA2Downloader-1.1.6-1.x86_64.rpm` (`sudo dnf install ./SA2Downloader-1.1.6-1.x86_64.rpm`)
  - Any distribution: `SA2Downloader-1.1.6-x86_64.AppImage` (make executable and run).
- **Browser Extension (1.1.6)**:
  - Chrome / Edge / Brave: Download `SA2Downloader-Chrome-1.1.6.zip`, extract, go to `chrome://extensions`, enable *Developer mode*, click *Load unpacked*, and select the extension folder.
  - Firefox: Download `SA2Downloader-Firefox-1.1.6.xpi`, open `about:debugging#/runtime/this-firefox`, and click *Load Temporary Add-on…*.

## Media helpers

Settings → Plugins downloads and keeps **yt-dlp**, **FFmpeg + ffprobe** and a portable **JavaScript runtime** (Deno) automatically up to date into the app's own folder — no manual installers, no PATH changes, no administrator privileges needed.
System copies on PATH or custom executable paths can be chosen instead.

RAR archives are read with the UnRAR source code (freeware licence, extraction only); see `NOTICE.md`.
