# Download SA2: what is still open

The one roadmap. It lists only what is not done yet, along with recently completed milestones.

Updated 29 Oct 2026, for 1.1.6.

**Priority:** 🔴 do first · 🟠 worth doing soon · 🟢 when there is time.

---

## Recently Completed (v1.1.6)

| Milestone | Resolution |
|---|---|
| ✅ **Ultra HD 2K / 4K / 8K Dynamic Downloads** | Resolved 360p fallback; streams now download at native max resolution (8K 4320p, 4K 2160p, 2K 1440p, 1080p, 60fps AV1/VP9). |
| ✅ **Universal Multi-Website Video Support** | Dynamic stream extraction across millions of video websites (YouTube, Facebook, Vimeo, BongoBD, JustWatch, Bilibili, TikTok, and any DASH/HLS stream). |
| ✅ **Browser Cookie Forwarding** | Forward login cookies from Chrome, Edge, Firefox, Brave, Opera, and Vivaldi to bypass bot checks and HTTP 429 errors. |
| ✅ **Single Instant IDM-Style Video Loading (0ms)** | Real stream formats and calculated sizes pre-fetched by the browser extension into memory cache, eliminating 5–10s blocking probes and duplicate popup loops. |
| ✅ **Automated Daily Background Updates** | Automatically checks and updates managed `yt-dlp` and `FFmpeg` binaries once every 24 hours silently in the background with atomic replacements. |
| ✅ **Smooth 60 FPS Lerp Progress Bar** | Progress window height oscillation eliminated with locked layout geometry and fluid 60fps progress bar interpolation. |
| ✅ **Playlist Quality & Format Inheritance** | Selecting Audio or a Video quality applies instantly to all items in a playlist batch. |

---

## Before listing in the browser stores

| | Item | Notes |
|---|---|---|
| 🔴 | Firefox add-on ID | `browser-extension/manifest-firefox.json` and the Firefox host manifest in `src/app.rs` use `send-to-download-sa2@example`. Change both to an address you own before the first AMO upload: AMO fixes the ID for good. |
| 🟠 | Store IDs for Edge and Opera | Once the extension is listed there, add the IDs those stores assign to `STORE_EXTENSION_IDS` in `src/app.rs` (see `docs/STORE_LISTING.md`). |

## Not tested against the real thing yet

| | Item | Notes |
|---|---|---|
| 🟠 | NTLM / Kerberos logins | Built on Windows' WinHTTP and tested with a local login server; never run against a real company (NTLM/Negotiate) server. |
| 🟠 | PAC proxy scripts | Tested with Windows' auto-detect only; no PAC server was available. |
| 🟠 | Browser hand-off on real sites | Extension tests and the native host pass, and Facebook and YouTube were used day to day; pause/resume, POST downloads, Alt-click and page-fetch have not all been clicked through on real sites. |

## Engine and protocols

| | Item | Notes |
|---|---|---|
| 🟢 | SFTP | Needs an SSH library; do not hand-write SSH. |
| 🟢 | More error recovery | IDM handles many rare server faults ("restart types"). Add them as real downloads turn them up; the common ones (416, 429/503 with Retry-After, expired links, HTTP/2, IPv6, NTLM hand-over, FTP behind NAT) are done. |

## Browser extension

| | Item | Notes |
|---|---|---|
| 🟠 | Instagram, TikTok, X, Reddit on real accounts | The button finds the post under it and the app fetches it with yt-dlp (checked on TikTok). Instagram and X need a signed-in browser; their page layouts were not checked live, and Reddit could not be opened in the test browser. The extension's self-check reports a missing post link to the app's log. |
| 🟢 | Permission audit | Check every permission in `manifest.json` is still used before a store review; the reasons are in `docs/STORE_LISTING.md`. |
| 🟢 | Welcome page per browser | The page covers private-window access; wording for Edge / Opera / Firefox and an "app too old" state are not there. |
| 🟢 | Binary native messaging | IDM sends binary frames; JSON works and is fast enough, so only if a measurement says otherwise. |

## Decided against

- A native overlay panel over the video player (IDM's): the in-page button works on every system and is kept.
- DRM-protected streams (Widevine, FairPlay): never.
