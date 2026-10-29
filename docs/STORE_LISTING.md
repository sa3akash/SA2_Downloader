# Store listing: Send to Download SA2

The text and answers to paste into each browser's extension store. The extension is the folder `browser-extension/`; the Firefox build comes from `browser-extension/build-firefox.ps1`.

## Short description (132 characters max)

> Send downloads and videos to Download SA2, the free open-source download manager. It can also download them right in the browser.

## Full description

Download SA2 is a fast, advanced, open-source (MIT) download manager for Windows, macOS, and Linux. This extension connects it seamlessly to your browser:

- **Ultra-fast parallel downloads.** Files you download go to Download SA2 instead of the browser. It splits each file over up to 32 parallel connections with dynamic work-stealing, crash-safe resume, and expired link refresh.
- **Saves Ultra HD videos (2K, 4K, 8K).** A sleek download pill appears on playing videos, supporting YouTube, Facebook, Vimeo, BongoBD, JustWatch, Bilibili, TikTok, and millions of other sites. Select any resolution or audio-only extraction with automatic stream merging.
- **Single Instant Loading (0ms).** Video formats and sizes are detected instantly without waiting for slow server probes or experiencing duplicate popups.
- **Complete Playlist Downloader.** One-click batch downloading for entire video playlists, series, albums, and channel lists with unified quality inheritance.
- **Works without the app.** If Download SA2 is not running, the extension can download, join, and save files directly within the browser.
- **Right-click menu.** Download links, batches of links, media streams, and BitTorrent magnet links with a single click.
- **Preserves your session.** Forwards page cookies, authentication tokens, referrers, and headers to bypass bot verification (429 errors) and access members-only content safely on your local device.

No account, no ads, no telemetry. The source code is available at https://github.com/sa3akash/SA2_Downloader.

## Category

Productivity (Chrome / Edge / Opera); Download Management (Firefox).

## Single purpose (Chrome Web Store)

> Hand browser downloads and page videos to the Download SA2 desktop app, or download them in the browser when the app is not available.

## Permission justifications

| Permission | Why it is needed |
|---|---|
| `nativeMessaging` | Talks to the Download SA2 desktop app on the same computer. This is the extension's main job. |
| `downloads`, `downloads.open`, `downloads.ui` | Notices new browser downloads so they can be handed to the app. Starts browser downloads itself when the app is not available, and opens finished files. Hides the browser's download bar when the app took over. |
| `contextMenus` | Adds the "Download with Download SA2" right-click items. |
| `cookies` | Reads the cookies of the site a download comes from, so the app can download files that need a login. The cookies go only to the local app, never to a server. |
| `webRequest` | Reads the headers and server address of download and media requests, so the app's request matches the browser's. Also finds video streams (HLS/DASH manifests) as the page loads them. Nothing is blocked or changed. |
| `proxy` | Reads (does not change) the browser's proxy setting, so the app uses the same proxy for a handed-off download. |
| `scripting` | Adds the video button to tabs that were already open when the extension was installed or updated. |
| `tabs` | Reads the current tab's URL and title for the right-click menu, the popup's link list, and file naming. |
| `offscreen` | Joins the video and audio parts of an in-browser video download into one file. |
| `storage` | Saves the extension's settings. |
| `notifications` | Says when an in-browser download finished or failed. |
| Host access `http://*/*`, `https://*/*` | Downloads and videos can come from any site. The content scripts look for video players and download links on the page; they do not read or send page content anywhere else. |

## Data use (Chrome "Privacy practices" tab)

- **Data collected:** none. The extension sends nothing to any server run by the project.
- **Website content and authentication information:** the URL, cookies and request headers of a download are passed to the Download SA2 app **on the same computer** through native messaging, only to perform that download.
- Certify: no sale of data, no use beyond the single purpose, no use for credit decisions.

## Privacy policy (paste as a page or into the listing)

> Send to Download SA2 does not collect, store or send any personal data to the developers or to third parties. When you download a file, the extension passes the file's address, the site's cookies and the request headers to the Download SA2 program installed on your own computer, so it can download that file the way your browser would. This data never leaves your computer except in the download request itself, which goes to the website you were downloading from. Settings are saved in your browser's extension storage. The extension has no analytics and no ads.

## Assets

- Icon: `browser-extension/icons/icon.png` (128×128; Edge and Opera also take 300×300, so export it again at that size).
- Screenshots (1280×800 or 640×400): crop from `docs/screenshot-extension-popup.png`, `docs/screenshot-video-bar.jpg`, `docs/screenshot-extension-downloads.png`, `docs/screenshot-main-window.png`.

## Per store

### Chrome Web Store
- Upload a ZIP of `browser-extension/` **without** the `*.test.js` files, `testdata/`, `manifest-firefox.json` or `build-firefox.ps1`.
- The `key` in `manifest.json` fixes the extension ID that the app's native host allows (`allowed_origins`). The Web Store keeps an uploaded item's ID only if the same key is used. Once the item exists, check that its ID matches `EXTENSION_ID` in the app.

### Microsoft Edge Add-ons
- Same ZIP. Edge gives the item its own ID: add that ID to `STORE_EXTENSION_IDS` in `src/app.rs` (the native host then allows it) and ship that app build before the listing goes live.

### Opera Add-ons
- Same ZIP. Opera reads the native host from Chrome's registry key, which the app registers at every start, so no extra install step is needed. Opera also assigns its own ID: add it to `STORE_EXTENSION_IDS` as for Edge.
- Opera users can also install the Chrome Web Store version through "Install Chrome Extensions".

### Firefox (AMO)
- Build with `build-firefox.ps1` and upload the result. The add-on ID comes from `manifest-firefox.json` (`browser_specific_settings.gecko.id`) and must match `allowed_extensions` in the Firefox host manifest (`install_native_host` in `src/app.rs`). It is `send-to-download-sa2@example` today. Change both to an address you own (for example `send-to-download-sa2@<your domain>`) before the first upload, because AMO fixes the ID for good.
- AMO asks for the source of minified code. None of the code is minified, so upload as is.

## Review notes (for the reviewer field)

> The extension needs the Download SA2 desktop app (free, MIT, https://github.com/sa3akash/SA2_Downloader) for its main feature. Without the app it still works: downloads and videos are saved by the browser itself. To test: install the extension, open any page with a video or a file link, and use the button or the right-click menu.
