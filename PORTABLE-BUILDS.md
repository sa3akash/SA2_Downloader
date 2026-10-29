# Download SA2 1.2.0

Use [the current release instructions](RELEASE-NOTES.md) for Windows, universal
Mac, Fedora/RPM, Debian-family DEB and AppImage installation and media helpers.

`VERSION` contains the public app version. Cargo, UI, CLI and browser versions
are aligned at 1.2.0.

### Automated GitHub Releases
All builds across Windows, macOS, and Linux are automatically compiled, packaged,
and published to GitHub Releases by `.github/workflows/portable.yml`.
After all builds complete successfully, release assets are attached to the GitHub
Release page ready for instant download and use:
- `SA2Downloader-1.2.0-windows-x64.exe` (Windows 64-bit standalone executable)
- `SA2Downloader-1.2.0-windows-x86.exe` (Windows 32-bit standalone executable)
- `SA2Downloader-1.2.0-windows-x64-portable.zip` (Windows 64-bit portable)
- `SA2Downloader-1.2.0-windows-x86-portable.zip` (Windows 32-bit portable)
- `SA2Downloader-1.2.0-macos-Universal.zip` (macOS Apple Silicon & Intel)
- `SA2Downloader-1.2.0-x86_64.AppImage` (Universal Linux AppImage)
- `SA2Downloader_1.2.0_amd64.deb` (Debian, Ubuntu, Mint)
- `SA2Downloader-1.2.0-1.x86_64.rpm` (Fedora, RHEL)
- `SA2Downloader-Chrome-1.2.0.zip` (Chrome, Edge, Brave browser extension)
- `SA2Downloader-Firefox-1.2.0.xpi` (Firefox add-on)
- `SHA256SUMS.txt` (Integrity checksums)

### Automatic Daily Media Helpers (`yt-dlp` & `FFmpeg`)
Portable builds feature automated 24-hour background helper updates:
- Managed `yt-dlp` and `FFmpeg` are stored locally in the application's portable data directory.
- On launch, if at least 24 hours have elapsed since the last check, a background thread checks GitHub releases and atomically updates older helper binaries.
- The UI remains completely non-blocking with 0ms startup delay.
- The feature can be toggled in **Settings → Plugins** or **Settings → Updates**.

### Local & CI Build Instructions
Build native binaries with Cargo. Package with `scripts/package.py`, or build
Linux glibc-2.28 binaries using cargo-zigbuild and run `scripts/package_linux.py`.
Mac cross-packaging accepts `--signer <rcodesign>` to sign the app's resources as
well as its universal executable. Native Mac packaging uses system codesign.

Paused/failed downloads may retain `.dcmeta` for resuming; successful jobs
remove it. What is still open is listed in [docs/ROADMAP.md](docs/ROADMAP.md).
