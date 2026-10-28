# Download Craft 1.1.0

Use [the current release instructions](RELEASE-NOTES.md) for Windows, universal
Mac, Fedora/RPM, Debian-family DEB and AppImage installation and media helpers.

`VERSION` contains the public app version. Cargo, UI, CLI and browser versions
are aligned at 1.1.0.

### Automated GitHub Releases
All builds across Windows, macOS, and Linux are automatically compiled, packaged,
and published to GitHub Releases by `.github/workflows/portable.yml`.
After all builds complete successfully, release assets are attached to the GitHub
Release page ready for instant download and use:
- `DownloadCraft-1.1.0-windows-x64-portable.zip` (Windows portable)
- `DownloadCraft-1.1.0-macos-Universal.zip` (macOS Apple Silicon & Intel)
- `DownloadCraft-1.1.0-x86_64.AppImage` (Universal Linux AppImage)
- `downloadcraft_1.1.0_amd64.deb` (Debian, Ubuntu, Mint)
- `downloadcraft-1.1.0-1.x86_64.rpm` (Fedora, RHEL)
- `DownloadCraft-Chrome-1.1.0.zip` (Chrome, Edge, Brave browser extension)
- `DownloadCraft-Firefox-1.1.0.xpi` (Firefox add-on)
- `SHA256SUMS.txt` (Integrity checksums)

### Local & CI Build Instructions
Build native binaries with Cargo. Package with `scripts/package.py`, or build
Linux glibc-2.28 binaries using cargo-zigbuild and run `scripts/package_linux.py`.
Mac cross-packaging accepts `--signer <rcodesign>` to sign the app's resources as
well as its universal executable. Native Mac packaging uses system codesign.

Paused/failed downloads may retain `.dcmeta` for resuming; successful jobs
remove it. What is still open is listed in [docs/ROADMAP.md](docs/ROADMAP.md).
