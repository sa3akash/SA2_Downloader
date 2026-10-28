//! IDM-style "Download all links" window (`--links <file>`).
//!
//! The browser extension collects every link on a page (or in the selection)
//! and the native host writes them to `links_<ms>.json` in the config dir.
//! This window lists them with a checkbox each, fills in type and size with
//! background HEAD probes, and queues the checked ones straight into the
//! shared `queue.json` (+ `wake.json` to start them), like the confirm dialog.
//!
//! Layout follows `redesign/download_all_links.html`: a page header with
//! counts, a toolbar (search, type chips, "Hide web pages"), a painted table
//! (type icons, two-line names, type pills, sortable and resizable columns,
//! Shift-click ranges) and a footer (selection, Save to, actions).

use crate::app::pl;
use crate::dm::*;
use crate::engine::{self, EngineOpts};
use crate::fonts::{icon, reg, semi};
use crate::model::{category_of, config_dir, filename_from_url, format_bytes, DownloadItem};
use crate::native_host::LinksBatch;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::mpsc::Receiver;

/// Probes in flight at once (one per host would be politer, but pages that
/// list files usually serve them all from one host anyway).
const PROBE_CONCURRENCY: usize = 6;
/// Links beyond this are listed but not probed (type from the name only).
const PROBE_MAX: usize = 1000;

const MI_GLOBE: &str = "\u{e80b}";
const MI_CHECK: &str = "\u{e5ca}";
const MI_DOWNLOAD: &str = "\u{e2c4}";
const MI_QUEUE: &str = "\u{e03b}";
const MI_FOLDER: &str = "\u{e2c7}";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Probe {
    Pending,
    Done,
    Failed,
}

/// What a link is, for its icon, colour, pill and type chip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Video,
    Audio,
    Image,
    Doc,
    Zip,
    App,
    Page,
    Other,
}

impl Kind {
    const ALL: [Kind; 8] = [Kind::Video, Kind::Audio, Kind::Image, Kind::Doc, Kind::Zip, Kind::App, Kind::Page, Kind::Other];
    fn of(name: &str, html: bool, image: bool) -> Kind {
        if html {
            return Kind::Page;
        }
        if image {
            return Kind::Image;
        }
        match category_of(name) {
            "Video" => Kind::Video,
            "Music" => Kind::Audio,
            "Image" => Kind::Image,
            "Document" => Kind::Doc,
            "Compressed" => Kind::Zip,
            "Apps" => Kind::App,
            _ => Kind::Other,
        }
    }
    /// Chip label (plural) and pill label (singular).
    fn labels(self) -> (&'static str, &'static str) {
        match self {
            Kind::Video => ("Video", "Video"),
            Kind::Audio => ("Audio", "Audio"),
            Kind::Image => ("Images", "Image"),
            Kind::Doc => ("Documents", "Document"),
            Kind::Zip => ("Archives", "Archive"),
            Kind::App => ("Programs", "Program"),
            Kind::Page => ("Web pages", "Page"),
            Kind::Other => ("Other", "File"),
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Kind::Video => crate::app::MI_SLIDESHOW,
            Kind::Audio => crate::app::MI_MUSIC,
            Kind::Image => crate::app::MI_IMAGE,
            Kind::Doc => crate::app::MI_DOC,
            Kind::Zip => crate::app::MI_ARCHIVE,
            Kind::App => crate::app::MI_GRID,
            Kind::Page => MI_GLOBE,
            Kind::Other => crate::app::MI_INSERT_FILE,
        }
    }
    /// The mock-up's per-type colours, brighter on the dark theme.
    fn color(self, dark: bool) -> Color32 {
        let (d, l) = match self {
            Kind::Video => (0xa78bfa, 0x7c3aed),
            Kind::Audio => (0xf472b6, 0xdb2777),
            Kind::Image => (0x22d3ee, 0x0891b2),
            Kind::Doc => (0xfb923c, 0xea580c),
            Kind::Zip => (0xfacc15, 0xca8a04),
            Kind::App => (0x4ade80, 0x16a34a),
            Kind::Page | Kind::Other => (0x94a3b8, 0x64748b),
        };
        let v: u32 = if dark { d } else { l };
        Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
}

struct Row {
    url: String,
    text: String,
    name: String,
    /// The name is still our guess from the URL (a server name may replace it).
    guessed_name: bool,
    kind: Kind,
    size: Option<u64>,
    checked: bool,
    probe: Probe,
}

/// What the file is, from its name and (once probed) its Content-Type:
/// (label, is a web page, is an image).
fn type_of(name: &str, ctype: Option<&str>) -> (String, bool, bool) {
    let mime = ctype
        .map(|c| c.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
        .filter(|c| !c.is_empty() && c != "application/octet-stream");
    if let Some(m) = mime.as_deref() {
        if m == "text/html" || m == "application/xhtml+xml" {
            return ("Web page".to_string(), true, false);
        }
        if m.starts_with("image/") {
            return (format!("{} image", m[6..].to_ascii_uppercase()), false, true);
        }
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "htm" | "html" | "php" | "asp" | "aspx" | "jsp" | "cgi" | "shtml" => {
            return ("Web page".to_string(), true, false)
        }
        "" if mime.is_none() => return ("Web page".to_string(), true, false),
        _ => {}
    }
    let cat = category_of(name);
    if cat == "Image" {
        return (format!("{} image", ext.to_ascii_uppercase()), false, true);
    }
    if !ext.is_empty() && cat != "Other" {
        return (format!("{} {}", ext.to_ascii_uppercase(), cat.to_ascii_lowercase()), false, false);
    }
    match mime {
        Some(m) => (m, false, false),
        None => (format!("{} file", ext.to_ascii_uppercase()), false, false),
    }
}

/// Same host (cookies of the page are only sent back to it).
fn same_host(a: &str, b: &str) -> bool {
    let host = |u: &str| reqwest::Url::parse(u).ok().and_then(|x| x.host_str().map(|h| h.to_ascii_lowercase()));
    matches!((host(a), host(b)), (Some(x), Some(y)) if x == y)
}

/// Host and the last folder of a link ("download.sunplex.net", "S01").
fn host_and_folder(url: &str) -> (String, String) {
    let Ok(u) = reqwest::Url::parse(url) else { return (url.to_string(), String::new()) };
    let host = u.host_str().unwrap_or("").to_string();
    let segs: Vec<&str> = u.path_segments().map(|s| s.filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    let folder = if segs.len() >= 2 { segs[segs.len() - 2] } else { "" };
    let folder = percent_decode(folder);
    (host, folder)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Read the host's links file, but only from our own config dir and only
/// under the name the host gives it; the file is removed once read.
fn read_batch(path: &std::path::Path) -> Result<LinksBatch, String> {
    let dir = config_dir().ok_or("no config dir")?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let inside = path.parent().map(|p| p == dir).unwrap_or(false);
    if !inside || !name.starts_with("links_") || !name.ends_with(".json") {
        return Err("not a links file from the browser extension".into());
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(path);
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

/// Sortable columns.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortBy {
    Name,
    Type,
    Size,
    From,
}

struct LinksApp {
    page_url: String,
    title: String,
    cookie: String,
    rows: Vec<Row>,
    filter: String,
    /// Type chip: None = all.
    chip: Option<Kind>,
    hide_pages: bool,
    sort: Option<(SortBy, bool)>,
    /// Row index of the last plain click (Shift-click checks a range).
    anchor: Option<usize>,
    /// true = every file into its category folder (Settings rules);
    /// false = all into `one_dir`.
    by_category: bool,
    one_dir: String,
    prefs: crate::confirm::UiPrefs,
    /// Widths of the sized columns (see `COLS`), dragged by their dividers.
    cols: [f32; 3],
    probe_rx: Option<Receiver<(usize, Option<u64>, Option<String>, Option<String>)>>,
    probes_left: usize,
    _rt: std::sync::Arc<tokio::runtime::Runtime>,
    notice: String,
    centered: bool,
    focused: bool,
    done: bool,
    exited: bool,
    quality_pref: String,
    q_open: bool,
}

impl LinksApp {
    fn visible(&self, r: &Row) -> bool {
        if let Some(k) = self.chip {
            if r.kind != k {
                return false;
            }
        } else if self.hide_pages && r.kind == Kind::Page {
            return false;
        }
        let f = self.filter.trim().to_lowercase();
        f.is_empty()
            || r.name.to_lowercase().contains(&f)
            || r.url.to_lowercase().contains(&f)
            || r.text.to_lowercase().contains(&f)
            || r.kind.labels().0.to_lowercase().contains(&f)
    }

    /// Indexes of the rows shown, in display order.
    fn shown(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.rows.len()).filter(|&i| self.visible(&self.rows[i])).collect();
        if let Some((by, asc)) = self.sort {
            let r = &self.rows;
            v.sort_by(|&a, &b| {
                let o = match by {
                    SortBy::Name => r[a].name.to_lowercase().cmp(&r[b].name.to_lowercase()),
                    SortBy::Type => r[a].kind.labels().0.cmp(r[b].kind.labels().0),
                    SortBy::Size => r[a].size.unwrap_or(0).cmp(&r[b].size.unwrap_or(0)),
                    SortBy::From => r[a].url.cmp(&r[b].url),
                };
                if asc { o } else { o.reverse() }
            });
        }
        v
    }

    fn start_probes(&mut self, ctx: egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.probe_rx = Some(rx);
        let jobs: Vec<(usize, String)> = self
            .rows
            .iter()
            .enumerate()
            .take(PROBE_MAX)
            .map(|(i, r)| (i, r.url.clone()))
            .collect();
        self.probes_left = jobs.len();
        for r in self.rows.iter_mut().skip(PROBE_MAX) {
            r.probe = Probe::Failed;
        }
        let base = EngineOpts {
            user_agent: if self.prefs.user_agent.trim().is_empty() {
                format!("sa2_downloader/{}", crate::model::APP_VERSION)
            } else {
                self.prefs.user_agent.clone()
            },
            timeout_secs: self.prefs.timeout_secs.min(20),
            proxy_url: self.prefs.proxy_url.clone(),
            referer: self.page_url.clone(),
            ..Default::default()
        };
        let with_cookie = EngineOpts { cookie: self.cookie.clone(), ..base.clone() };
        let page = self.page_url.clone();
        self._rt.spawn(async move {
            use futures_util::StreamExt;
            let (Ok(plain), Ok(session)) = (base.build_client(), with_cookie.build_client()) else {
                for (i, _) in jobs {
                    let _ = tx.send((i, None, None, None));
                }
                ctx.request_repaint();
                return;
            };
            futures_util::stream::iter(jobs)
                .for_each_concurrent(PROBE_CONCURRENCY, |(i, url)| {
                    let (client, opts) = if same_host(&url, &page) {
                        (&session, &with_cookie)
                    } else {
                        (&plain, &base)
                    };
                    let tx = tx.clone();
                    let ctx = ctx.clone();
                    async move {
                        let (total, _, ctype, disp) = engine::probe(client, opts, &url).await;
                        let _ = tx.send((i, total, ctype, disp));
                        ctx.request_repaint();
                    }
                })
                .await;
        });
    }

    fn collect_probes(&mut self) {
        let Some(rx) = self.probe_rx.as_ref() else { return };
        while let Ok((i, total, ctype, disp)) = rx.try_recv() {
            self.probes_left = self.probes_left.saturating_sub(1);
            let Some(r) = self.rows.get_mut(i) else { continue };
            if total.is_none() && ctype.is_none() {
                r.probe = Probe::Failed;
                continue;
            }
            r.probe = Probe::Done;
            let is_vid = is_video_page_url(&r.url);
            if !is_vid {
                if r.guessed_name {
                    if let Some(d) = disp.filter(|d| !d.trim().is_empty()) {
                        r.name = engine::sanitize_filename(&d);
                    } else if !r.name.contains('.') {
                        if let Some(ext) = ctype.as_deref().and_then(crate::model::ext_for_mime) {
                            r.name.push_str(ext);
                        }
                    }
                }
                let (_, html, image) = type_of(&r.name, ctype.as_deref());
                r.kind = Kind::of(&r.name, html, image);
                // A page's length is its HTML, not a download size.
                r.size = total.filter(|s| *s > 0 && !html);
            } else {
                r.kind = Kind::Video;
            }
        }
    }

    fn dir_for(&self, name: &str) -> String {
        if !self.by_category {
            return self.one_dir.clone();
        }
        let mut p = std::path::PathBuf::from(&self.prefs.save_dir);
        match self.prefs.folder_rules.iter().find(|r| r.matches(name)) {
            Some(rule) if !rule.folder.trim().is_empty() => p.push(rule.folder.trim()),
            _ => p.push(category_of(name)),
        }
        p.to_string_lossy().to_string()
    }

    /// Queue the checked, visible rows; `start` also starts them.
    fn commit(&mut self, start: bool) {
        let picked: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.rows[i].checked && self.visible(&self.rows[i]))
            .collect();
        if picked.is_empty() {
            self.notice = "Check the links to download first.".into();
            return;
        }
        let (mut added, mut already, mut failed) = (Vec::new(), 0usize, None);
        for i in picked {
            let r = &self.rows[i];
            let name = crate::model::apply_rename_template(
                &self.prefs.rename_template,
                &engine::sanitize_filename(r.name.trim()),
                &r.url,
            );
            let mut final_name = engine::sanitize_filename(&name);
            if self.quality_pref == "audio" {
                if !final_name.ends_with(".mp3") {
                    let stem = std::path::Path::new(&final_name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&final_name);
                    final_name = format!("{stem}.mp3");
                }
            }
            let mut item = DownloadItem::new(
                0,
                r.url.clone(),
                final_name.clone(),
                self.dir_for(&final_name),
                self.prefs.connections,
            );
            item.referer = self.page_url.clone();
            if same_host(&r.url, &self.page_url) {
                item.cookie = self.cookie.clone();
            }
            item.description = r.text.clone();
            item.total_bytes = r.size;
            item.quality_pref = self.quality_pref.clone();
            item.expects_audio = true;
            match crate::model::append_to_queue(&mut item) {
                crate::model::AppendResult::Added(id) => added.push(id),
                crate::model::AppendResult::AlreadyQueued(_) => already += 1,
                crate::model::AppendResult::Error(e) => failed = Some(e),
            }
        }
        if start && !added.is_empty() {
            crate::confirm::wake_ids(&added);
        }
        if let Some(e) = failed {
            self.notice = format!("Queued {}; could not update the queue: {e}", added.len());
            return;
        }
        if added.is_empty() {
            self.notice = format!("All {already} checked link(s) are already in the queue.");
            return;
        }
        self.done = true;
    }

    /// Click on a row: toggle it, or with Shift every row between it and the
    /// last clicked one (to the clicked row's new state).
    fn click_row(&mut self, i: usize, shift: bool, shown: &[usize]) {
        let on = !self.rows[i].checked;
        match (shift, self.anchor.and_then(|a| shown.iter().position(|&x| x == a)), shown.iter().position(|&x| x == i)) {
            (true, Some(a), Some(b)) => {
                for &k in &shown[a.min(b)..=a.max(b)] {
                    self.rows[k].checked = on;
                }
            }
            _ => self.rows[i].checked = on,
        }
        self.anchor = Some(i);
    }

    // ------------------------------------------------------------- header --

    fn header(&mut self, ui: &mut egui::Ui) {
        let t = pl();
        let files: Vec<&Row> = self.rows.iter().filter(|r| r.kind != Kind::Page).collect();
        let found: u64 = files.iter().map(|r| r.size.unwrap_or(0)).sum();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
        let p = ui.painter();
        // Faceted 3D mark
        let fav = Rect::from_min_size(rect.min + Vec2::new(0.0, 3.0), Vec2::splat(34.0));
        crate::dm::dm_mark(p, Rect::from_center_size(fav.center(), Vec2::splat(22.0)));
        // Stats, right to left.
        let mut right = rect.max.x;
        let stats = [
            (format_bytes(found), " found"),
            (files.len().to_string(), " files"),
            (self.rows.len().to_string(), " links"),
        ];
        for (n, label) in stats {
            let wn = crate::shaped::width(p, &n, &semi(11.5));
            let wl = crate::shaped::width(p, label, &reg(11.5));
            let w = wn + wl + 18.0;
            let r = Rect::from_min_size(Pos2::new(right - w, rect.center().y - 12.0), Vec2::new(w, 24.0));
            p.rect(r, 6.0, t.surface, Stroke::new(1.0_f32, t.card_bd), egui::StrokeKind::Inside);
            p.text(Pos2::new(r.min.x + 9.0, r.center().y), Align2::LEFT_CENTER, &n, semi(11.5), t.txt);
            p.text(Pos2::new(r.min.x + 9.0 + wn, r.center().y), Align2::LEFT_CENTER, label, reg(11.5), t.dim);
            right = r.min.x - 6.0;
        }
        // Title and address: matching playlist.PNG "New Download Task"
        let x = fav.max.x + 12.0;
        let max_w = (right - x - 12.0).max(80.0);
        let title = if self.title.trim().is_empty() || self.title.trim() == "Download all links" {
            "New Download Task".to_string()
        } else {
            self.title.clone()
        };
        let title = crate::app::truncate_to_width(p, &title, semi(14.5), t.txt, max_w);
        crate::shaped::draw(p, Pos2::new(x, rect.min.y + 4.0), Align2::LEFT_TOP, &title, semi(14.5), t.txt);
        let url = crate::app::truncate_to_width(p, &self.page_url, reg(11.0), t.dim, max_w);
        p.text(Pos2::new(x, rect.min.y + 25.0), Align2::LEFT_TOP, url, reg(11.0), t.dim);
    }

    // ------------------------------------------------------------ toolbar --

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let t = pl();
        let dark = ui.visuals().dark_mode;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            // Search field.
            // Narrower on a small window, so the type chips keep their room.
            let search_w = (ui.available_width() * 0.3).clamp(150.0, 250.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(search_w, 30.0), Sense::hover());
            let focused = ui.memory(|m| m.focused() == Some(egui::Id::new("links-q")));
            ui.painter().rect(r, 6.0, t.bg, Stroke::new(1.0_f32, if focused { t.primary } else { t.card_bd }), egui::StrokeKind::Inside);
            ui.painter().text(Pos2::new(r.min.x + 16.0, r.center().y), Align2::CENTER_CENTER, crate::app::MI_SEARCH, icon(15.0), t.dim);
            ui.put(
                Rect::from_min_max(Pos2::new(r.min.x + 28.0, r.min.y + 4.0), Pos2::new(r.max.x - 6.0, r.max.y - 4.0)),
                egui::TextEdit::singleline(&mut self.filter)
                    .id(egui::Id::new("links-q"))
                    .frame(false)
                    .font(reg(12.0))
                    .hint_text("Filter by name, type or address")
                    .vertical_align(egui::Align::Center),
            );
            // Type chips with counts (only the types present).
            let mut counts = [0usize; 8];
            for r in &self.rows {
                counts[Kind::ALL.iter().position(|k| *k == r.kind).unwrap_or(7)] += 1;
            }
            let mut chips: Vec<(Option<Kind>, String, usize)> = vec![(None, "All".into(), self.rows.len())];
            for (k, n) in Kind::ALL.iter().zip(counts) {
                if n > 0 {
                    chips.push((Some(*k), k.labels().0.into(), n));
                }
            }
            let toggle_w = 132.0;
            let avail = ui.available_width() - toggle_w - 8.0;
            egui::ScrollArea::horizontal()
                .id_salt("links-chips")
                .max_width(avail)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (k, label, n) in chips {
                            let on = self.chip == k;
                            let num = n.to_string();
                            let dot = if k.is_some() { 12.0 } else { 0.0 };
                            let w = dot + crate::shaped::width(ui.painter(), &label, &reg(11.5)) + crate::shaped::width(ui.painter(), &num, &reg(10.5)) + 26.0;
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
                            let p = ui.painter();
                            let (fill, bd, fg) = if on {
                                (t.primary.gamma_multiply(0.14), t.primary, t.primary)
                            } else if resp.hovered() {
                                (t.surface_lt, t.card_bd, t.txt)
                            } else {
                                (t.surface, t.card_bd, t.dim)
                            };
                            p.rect(r, 14.0, fill, Stroke::new(1.0_f32, bd), egui::StrokeKind::Inside);
                            let mut x = r.min.x + 10.0;
                            if let Some(k) = k {
                                p.circle_filled(Pos2::new(x + 3.5, r.center().y), 3.5, k.color(dark));
                                x += dot;
                            }
                            let lw = crate::shaped::width(p, &label, &reg(11.5));
                            p.text(Pos2::new(x, r.center().y), Align2::LEFT_CENTER, &label, reg(11.5), fg);
                            p.text(Pos2::new(x + lw + 5.0, r.center().y), Align2::LEFT_CENTER, &num, reg(10.5), if on { t.primary } else { t.dim });
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.chip = k;
                            }
                        }
                    });
                });
            // "Hide web pages" switch, right-aligned.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(toggle_w, 28.0), Sense::click());
                let p = ui.painter();
                let sw = Rect::from_min_size(Pos2::new(r.min.x, r.center().y - 8.0), Vec2::new(28.0, 16.0));
                p.rect_filled(sw, 8.0, if self.hide_pages { t.primary } else { t.surface_lt });
                let kx = if self.hide_pages { sw.max.x - 8.0 } else { sw.min.x + 8.0 };
                p.circle_filled(Pos2::new(kx, sw.center().y), 6.0, Color32::WHITE);
                p.text(Pos2::new(sw.max.x + 8.0, r.center().y), Align2::LEFT_CENTER, "Hide web pages", reg(11.5), if resp.hovered() { t.txt } else { t.dim });
                if resp.on_hover_text("Folder listings, site pages and social links").clicked() {
                    self.hide_pages = !self.hide_pages;
                }
            });
        });
    }

    // -------------------------------------------------------------- table --

    fn table(&mut self, ui: &mut egui::Ui) {
        let t = pl();
        let _dark = ui.visuals().dark_mode;
        let shown = self.shown();
        let card = ui.available_rect_before_wrap();
        ui.painter().rect(card, 8.0, t.bg, Stroke::new(1.0_f32, t.card_bd), egui::StrokeKind::Inside);
        let inner = card.shrink(1.0);
        let widths = col_widths(&self.cols, inner.width() - CB_W - crate::app::SB_SPACE);
        let xs = {
            let mut x = inner.min.x + CB_W;
            let mut v = [0.0; 4];
            for (k, w) in widths.iter().enumerate() {
                v[k] = x;
                x += w;
            }
            v
        };
        // ---- header ----
        let hdr = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HDR_H));
        let p = ui.painter().clone();
        p.rect_filled(hdr, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, t.surface);
        p.hline(hdr.x_range(), hdr.max.y, Stroke::new(1.0_f32, t.card_bd));
        // Tri-state check over what is shown.
        let n_on = shown.iter().filter(|&&i| self.rows[i].checked).count();
        let cbr = Rect::from_center_size(Pos2::new(inner.min.x + CB_W / 2.0, hdr.center().y), Vec2::splat(15.0));
        let resp = ui.interact(cbr.expand(4.0), egui::Id::new("links-all"), Sense::click()).on_hover_text("Check or uncheck everything shown");
        paint_check(&p, cbr, n_on > 0 && n_on == shown.len(), n_on > 0 && n_on < shown.len(), resp.hovered());
        if resp.clicked() {
            let on = n_on != shown.len();
            for &i in &shown {
                self.rows[i].checked = on;
            }
        }
        for (k, (label, by)) in COLS.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(xs[k], hdr.min.y), Vec2::new(widths[k], HDR_H));
            let resp = ui.interact(r, egui::Id::new(("links-sort", k)), Sense::click());
            let sorted = self.sort.map(|(b, _)| b == *by).unwrap_or(false);
            let fg = if sorted || resp.hovered() { t.txt } else { t.dim };
            let arrow = match self.sort {
                Some((b, asc)) if b == *by => if asc { " ▲" } else { " ▼" },
                _ => "",
            };
            let text = format!("{}{arrow}", label.to_uppercase());
            if k == 3 {
                p.text(Pos2::new(r.max.x - 10.0, r.center().y), Align2::RIGHT_CENTER, text, semi(10.5), fg);
            } else {
                p.text(Pos2::new(r.min.x + 10.0, r.center().y), Align2::LEFT_CENTER, text, semi(10.5), fg);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.sort = Some((*by, if sorted { !self.sort.map(|(_, a)| a).unwrap_or(true) } else { true }));
            }
            // Resize strip on the right edge of every column but the last:
            // a faint line always, an accent bar while hovered or dragged.
            if k + 1 < COLS.len() {
                let zone = Rect::from_center_size(Pos2::new(r.max.x, hdr.center().y), Vec2::new(8.0, HDR_H));
                let g = ui
                    .interact(zone, egui::Id::new(("links-col", k)), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeColumn)
                    .on_hover_text("Drag to resize");
                let active = g.hovered() || g.dragged();
                let bar = if active {
                    Rect::from_center_size(zone.center(), Vec2::new(3.0, HDR_H - 10.0))
                } else {
                    Rect::from_center_size(zone.center(), Vec2::new(1.0, HDR_H - 18.0))
                };
                p.rect_filled(bar, 1.5, if active { t.primary } else { t.card_bd });
                if g.dragged() && g.drag_delta().x != 0.0 {
                    self.cols = drag_divider(&self.cols, k, g.drag_delta().x);
                }
                if g.drag_stopped() {
                    save_cols(&self.cols);
                }
            }
        }
        // ---- rows ----
        let body = Rect::from_min_max(Pos2::new(inner.min.x, hdr.max.y + 1.0), inner.max);
        if shown.is_empty() {
            p.text(body.center(), Align2::CENTER_CENTER, "Nothing matches — clear the search or pick another type.", reg(12.0), t.dim);
            ui.allocate_rect(card, Sense::hover());
            return;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let mut clicked: Option<usize> = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        // Slim bars, always shown (as in the main list).
        crate::app::modern_scrollbars(&mut child);
        egui::ScrollArea::both()
            .id_salt("links-rows")
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show_rows(&mut child, ROW_H, shown.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let time = ui.input(|i| i.time) as f32;
                for &i in &shown[range] {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(body.width() - crate::app::SB_SPACE, ROW_H), Sense::click());
                    let row = &self.rows[i];
                    let p = ui.painter();
                    if row.checked {
                        p.rect_filled(r, 0.0, t.sel_bg);
                    } else if resp.hovered() {
                        p.rect_filled(r, 0.0, t.hover_bg);
                    }
                    p.hline(r.x_range(), r.max.y - 0.5, Stroke::new(1.0_f32, t.card_bd.gamma_multiply(0.5)));
                    let dim = if row.kind == Kind::Page { 0.6 } else { 1.0 };
                    let cy = r.center().y;
                    paint_check(p, Rect::from_center_size(Pos2::new(r.min.x + CB_W / 2.0, cy), Vec2::splat(15.0)), row.checked, false, resp.hovered());
                    // Column 0: File Type pill badge (matching playlist.PNG)
                    let ext = row.name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_else(|| row.kind.labels().1.to_ascii_uppercase());
                    let pw = (crate::shaped::width(p, &ext, &semi(11.0)) + 16.0).min(widths[0] - 16.0);
                    let pill = Rect::from_center_size(Pos2::new(xs[0] + widths[0] / 2.0, cy), Vec2::new(pw.max(52.0), 22.0));
                    p.rect_filled(pill, 4.0, COLOR_CHARCOAL);
                    p.rect_stroke(pill, 4.0, Stroke::new(1.0_f32, COLOR_VAMPIRE_GREY), egui::StrokeKind::Inside);
                    p.text(pill.center(), Align2::CENTER_CENTER, &ext, semi(11.0), Color32::WHITE);

                    // Column 1: File Name + link text
                    let nx = xs[1] + 10.0;
                    let nw = (widths[1] - 20.0).max(20.0);
                    let name = crate::app::truncate_to_width(p, &row.name, semi(12.0), t.txt, nw);
                    crate::shaped::draw(p, Pos2::new(nx, cy - 1.0), Align2::LEFT_BOTTOM, &name, semi(12.0), t.txt.gamma_multiply(dim));
                    let sub = crate::app::truncate_to_width(p, &row.text, reg(10.5), t.dim, nw);
                    crate::shaped::draw(p, Pos2::new(nx, cy + 1.0), Align2::LEFT_TOP, &sub, reg(10.5), t.dim);

                    // Column 2: Download From: host + last folder tag
                    let (host, folder) = host_and_folder(&row.url);
                    let hx = xs[2] + 10.0;
                    let hw = (widths[2] - 20.0).max(20.0);
                    let host_s = crate::app::truncate_to_width(p, &host, reg(12.0), t.dim, hw * 0.65);
                    let host_w = crate::shaped::width(p, &host_s, &reg(12.0));
                    p.text(Pos2::new(hx, cy), Align2::LEFT_CENTER, &host_s, reg(12.0), t.dim);
                    if !folder.is_empty() {
                        let room = hw - host_w - 14.0;
                        if room > 24.0 {
                            let f = crate::app::truncate_to_width(p, &folder, reg(9.5), t.dim, room - 10.0);
                            let fw = crate::shaped::width(p, &f, &reg(9.5)) + 10.0;
                            let tag = Rect::from_min_size(Pos2::new(hx + host_w + 6.0, cy - 8.0), Vec2::new(fw, 16.0));
                            p.rect_filled(tag, 4.0, t.surface_lt);
                            crate::shaped::draw(p, Pos2::new(tag.min.x + 5.0, cy), Align2::LEFT_CENTER, &f, reg(9.5), t.dim);
                        }
                    }

                    // Column 3: Size
                    let sx = xs[3] + widths[3] - 10.0;
                    match (row.size, row.probe) {
                        (Some(s), _) => {
                            p.text(Pos2::new(sx, cy), Align2::RIGHT_CENTER, format_bytes(s), reg(12.0), t.txt.gamma_multiply(dim));
                        }
                        (None, Probe::Pending) => {
                            let bar = Rect::from_min_size(Pos2::new(sx - 58.0, cy - 5.0), Vec2::new(58.0, 10.0));
                            let a = 0.35 + 0.25 * (time * 3.0 + i as f32 * 0.4).sin().abs();
                            p.rect_filled(bar, 4.0, t.surface_lt.gamma_multiply(a + 0.4));
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
                        }
                        _ => {
                            p.text(Pos2::new(sx, cy), Align2::RIGHT_CENTER, "—", reg(12.0), t.dim);
                        }
                    }
                    if resp.on_hover_text(&row.url).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        if let Some(i) = clicked {
            self.click_row(i, shift, &shown);
        }
        ui.allocate_rect(card, Sense::hover());
    }

    // ------------------------------------------------------------- footer --

    fn footer(&mut self, ui: &mut egui::Ui) {
        let t = pl();
        let checked: Vec<&Row> = self.rows.iter().filter(|r| r.checked && self.visible(r)).collect();
        let n = checked.len();
        let bytes: u64 = checked.iter().map(|r| r.size.unwrap_or(0)).sum();
        let total: u64 = self.rows.iter().filter(|r| r.kind != Kind::Page).map(|r| r.size.unwrap_or(0)).sum();
        let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.0), Sense::hover());
        let p = ui.painter().clone();
        // Selection line + bar.
        let line_y = area.min.y + 10.0;
        if n > 0 {
            let a = format!("{n}");
            let b = format!(" of {} checked · ", self.rows.len());
            let c = format_bytes(bytes);
            let mut x = area.min.x;
            for (s, f, col) in [(a, semi(12.0), t.txt), (b, reg(12.0), t.dim), (c, semi(12.0), t.txt)] {
                let w = crate::shaped::width(&p, &s, &f);
                p.text(Pos2::new(x, line_y), Align2::LEFT_CENTER, s, f, col);
                x += w;
            }
        } else {
            p.text(Pos2::new(area.min.x, line_y), Align2::LEFT_CENTER, "Nothing checked yet — tick the files you want.", reg(12.0), t.dim);
        }
        let bar = Rect::from_min_size(Pos2::new(area.min.x, line_y + 12.0), Vec2::new(260.0, 4.0));
        p.rect_filled(bar, 2.0, t.surface_lt);
        if total > 0 && bytes > 0 {
            let f = (bytes as f32 / total as f32).clamp(0.0, 1.0);
            p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * f, 4.0)), 2.0, t.primary);
        }
        if self.probes_left > 0 {
            p.text(Pos2::new(bar.max.x + 12.0, bar.center().y), Align2::LEFT_CENTER, format!("checking {} …", self.probes_left), reg(10.5), t.dim);
        }

        // Save To row: label + Auto sorting radio pill + All files to one directory radio pill + path + Available disk space
        let row_y = area.min.y + 44.0;
        p.text(Pos2::new(area.min.x, row_y), Align2::LEFT_CENTER, "Save To:", semi(11.5), t.txt);

        // Auto sorting capsule button
        let opt0_w = 114.0;
        let opt0_r = Rect::from_min_size(Pos2::new(area.min.x + 64.0, row_y - 13.0), Vec2::new(opt0_w, 26.0));
        let resp0 = ui.interact(opt0_r, egui::Id::new("links-opt-auto"), Sense::click());
        let on0 = self.by_category;
        p.rect(opt0_r, 13.0, if on0 { t.primary.gamma_multiply(0.12) } else { t.surface }, Stroke::new(1.2_f32, if on0 { t.primary } else { t.card_bd }), egui::StrokeKind::Inside);
        p.circle_filled(Pos2::new(opt0_r.min.x + 13.0, opt0_r.center().y), 4.0, if on0 { t.primary } else { t.card_bd });
        if on0 { p.circle_filled(Pos2::new(opt0_r.min.x + 13.0, opt0_r.center().y), 2.0, Color32::WHITE); }
        p.text(Pos2::new(opt0_r.min.x + 24.0, opt0_r.center().y), Align2::LEFT_CENTER, "Auto sorting", semi(11.0), if on0 { t.primary } else { t.dim });
        if resp0.clicked() { self.by_category = true; }

        // All files to one directory capsule button
        let opt1_w = 164.0;
        let opt1_r = Rect::from_min_size(Pos2::new(opt0_r.max.x + 8.0, row_y - 13.0), Vec2::new(opt1_w, 26.0));
        let resp1 = ui.interact(opt1_r, egui::Id::new("links-opt-dir"), Sense::click());
        let on1 = !self.by_category;
        p.rect(opt1_r, 13.0, if on1 { t.primary.gamma_multiply(0.12) } else { t.surface }, Stroke::new(1.2_f32, if on1 { t.primary } else { t.card_bd }), egui::StrokeKind::Inside);
        p.circle_filled(Pos2::new(opt1_r.min.x + 13.0, opt1_r.center().y), 4.0, if on1 { t.primary } else { t.card_bd });
        if on1 { p.circle_filled(Pos2::new(opt1_r.min.x + 13.0, opt1_r.center().y), 2.0, Color32::WHITE); }
        p.text(Pos2::new(opt1_r.min.x + 24.0, opt1_r.center().y), Align2::LEFT_CENTER, "All files to one directory", semi(11.0), if on1 { t.primary } else { t.dim });
        if resp1.clicked() { self.by_category = false; }

        // Path box + Browse button
        let path_x = opt1_r.max.x + 10.0;
        let avail_space = crate::dm::available_disk_space(&self.one_dir);
        let avail_str = match avail_space {
            Some(b) => format!("Available: {}", crate::dm::format_disk_space(b)),
            None => String::new(),
        };
        let avail_w = if avail_str.is_empty() { 0.0 } else { crate::shaped::width(&p, &avail_str, &semi(11.0)) + 12.0 };
        let path_w = (area.max.x - path_x - avail_w - 8.0).clamp(110.0, 310.0);
        let path = Rect::from_min_size(Pos2::new(path_x, row_y - 14.0), Vec2::new(path_w, 28.0));
        p.rect(path, 6.0, t.bg, Stroke::new(1.0_f32, t.card_bd), egui::StrokeKind::Inside);
        let browse = Rect::from_min_size(Pos2::new(path.max.x - 64.0, path.min.y + 3.0), Vec2::new(60.0, 22.0));

        if self.by_category {
            let shown = format!("{}\\<category>", self.prefs.save_dir.trim_end_matches(['\\', '/']));
            let s = crate::app::truncate_to_width(&p, &shown, reg(11.0), t.dim, path.width() - 20.0);
            p.text(Pos2::new(path.min.x + 8.0, path.center().y), Align2::LEFT_CENTER, s, reg(11.0), t.dim.gamma_multiply(0.7));
        } else {
            ui.put(
                Rect::from_min_max(Pos2::new(path.min.x + 6.0, path.min.y + 4.0), Pos2::new(browse.min.x - 4.0, path.max.y - 4.0)),
                egui::TextEdit::singleline(&mut self.one_dir).frame(false).font(reg(11.0)).vertical_align(egui::Align::Center),
            );
            let resp = ui.interact(browse, egui::Id::new("links-browse"), Sense::click());
            p.rect(browse, 4.0, if resp.hovered() { t.surface_lt } else { t.surface }, Stroke::new(1.0_f32, t.card_bd), egui::StrokeKind::Inside);
            p.text(Pos2::new(browse.min.x + 6.0, browse.center().y), Align2::LEFT_CENTER, MI_FOLDER, icon(12.0), t.dim);
            p.text(Pos2::new(browse.min.x + 22.0, browse.center().y), Align2::LEFT_CENTER, "Browse", reg(10.5), t.txt);
            if resp.clicked() {
                if let Some(d) = rfd::FileDialog::new().set_directory(&self.one_dir).pick_folder() {
                    self.one_dir = d.to_string_lossy().to_string();
                }
            }
        }

        // Available disk space
        if !avail_str.is_empty() {
            p.text(Pos2::new(path.max.x + 10.0, row_y), Align2::LEFT_CENTER, &avail_str, semi(11.0), t.dim);
        }

        // Quality / Resolution selection combo box
        let q_r = Rect::from_min_size(Pos2::new(area.max.x - 410.0, area.max.y - 30.0), Vec2::new(175.0, 28.0));
        let cur_q = match self.quality_pref.as_str() {
            "" | "best" => "Best Quality",
            "2160p" => "4K (2160p)",
            "1440p" => "2K (1440p)",
            "1080p" => "1080p Full HD",
            "720p" => "720p HD",
            "480p" => "480p SD",
            "360p" => "360p SD",
            "audio" => "Audio only (MP3)",
            other => other,
        };
        let q_opts = vec![
            "Best Quality".to_string(),
            "4K (2160p)".to_string(),
            "2K (1440p)".to_string(),
            "1080p Full HD".to_string(),
            "720p HD".to_string(),
            "480p SD".to_string(),
            "360p SD".to_string(),
            "Audio only (MP3)".to_string(),
        ];
        if let Some(pick) = crate::confirm::combo(ui, q_r, "links_q", cur_q, &q_opts, &mut self.q_open) {
            match pick.as_str() {
                "Best Quality" => self.quality_pref = "best".to_string(),
                "4K (2160p)" => self.quality_pref = "2160p".to_string(),
                "2K (1440p)" => self.quality_pref = "1440p".to_string(),
                "1080p Full HD" => self.quality_pref = "1080p".to_string(),
                "720p HD" => self.quality_pref = "720p".to_string(),
                "480p SD" => self.quality_pref = "480p".to_string(),
                "360p SD" => self.quality_pref = "360p".to_string(),
                "Audio only (MP3)" => self.quality_pref = "audio".to_string(),
                other => self.quality_pref = other.to_string(),
            }
        }

        // Bottom row: Action pill buttons [Cancel ✕] and [Download ⬇]
        let cancel_r = Rect::from_min_size(Pos2::new(area.max.x - 224.0, area.max.y - 30.0), Vec2::new(98.0, 28.0));
        let resp_c = ui.interact(cancel_r, egui::Id::new("links-btn-cancel"), Sense::click());
        btn_pill(&p, cancel_r, resp_c.hovered(), "Cancel", t.surface, t.card_bd, t.txt, Some(DmIcon::Close));
        if resp_c.clicked() { self.done = true; }

        let dl_label = if n > 0 { format!("Download ({n})") } else { "Download".to_string() };
        let dl_r = Rect::from_min_size(Pos2::new(area.max.x - 116.0, area.max.y - 30.0), Vec2::new(116.0, 28.0));
        let resp_dl = ui.interact(dl_r, egui::Id::new("links-btn-dl"), if n > 0 { Sense::click() } else { Sense::hover() });
        btn_spring_breeze(&p, dl_r, resp_dl.hovered() && n > 0, &dl_label, Some(DmIcon::ArrowDown));
        if resp_dl.clicked() && n > 0 { self.commit(true); }

        if !self.notice.is_empty() {
            p.text(Pos2::new(area.min.x, area.max.y - 14.0), Align2::LEFT_CENTER, &self.notice, reg(11.0), t.warning);
        }
    }
}

/// Checkbox in the mock-up's style: accent fill with a tick, a bar when only
/// some rows are checked.
fn paint_check(p: &egui::Painter, r: Rect, on: bool, mixed: bool, hover: bool) {
    let t = pl();
    if on || mixed {
        p.rect_filled(r, 4.0, t.primary);
        if on {
            p.text(r.center(), Align2::CENTER_CENTER, MI_CHECK, icon(13.0), Color32::WHITE);
        } else {
            p.rect_filled(Rect::from_center_size(r.center(), Vec2::new(7.0, 2.0)), 1.0, Color32::WHITE);
        }
    } else {
        p.rect(r, 4.0, t.bg, Stroke::new(1.5_f32, if hover { t.primary } else { t.card_bd }), egui::StrokeKind::Inside);
    }
}

const CB_W: f32 = 40.0;
const HDR_H: f32 = 32.0;
const ROW_H: f32 = 40.0;
/// Column titles and what clicking them sorts by matching playlist.PNG:
/// File Type, File Name, Download From, Size.
const COLS: [(&str, SortBy); 4] = [
    ("File Type", SortBy::Type),
    ("File Name", SortBy::Name),
    ("Download From", SortBy::From),
    ("Size", SortBy::Size),
];
const COL_MIN: f32 = 60.0;
/// Stored widths of the three sized columns (type, name, size).
const COLS_DEFAULT: [f32; 3] = [100.0, 320.0, 96.0];

/// All four widths for a table `avail` wide: the stored three, and the rest
/// for "Download from" (never under 140).
fn col_widths(stored: &[f32; 3], avail: f32) -> [f32; 4] {
    let [t, n, s] = *stored;
    let over = (t + n + s + 140.0 - avail).max(0.0);
    let n = (n - over).max(160.0);
    let flex = (avail - (t + n + s)).max(140.0);
    [t, n, flex, s]
}

#[cfg(test)]
#[test]
fn narrow_windows_shrink_the_name_column() {
    let w = col_widths(&[100.0, 320.0, 96.0], 620.0);
    assert!(w.iter().sum::<f32>() <= 620.0 + 0.5, "{w:?}");
    assert!(w[2] >= 140.0 && w[1] >= 160.0);
    assert_eq!(col_widths(&[100.0, 320.0, 96.0], 1000.0), [100.0, 320.0, 484.0, 96.0]);
}

fn drag_divider(stored: &[f32; 3], i: usize, dx: f32) -> [f32; 3] {
    let mut out = *stored;
    if i == 0 {
        let dx = dx.clamp(COL_MIN - out[0], out[1] - COL_MIN);
        out[0] += dx;
        out[1] -= dx;
    } else if i == 1 {
        out[1] = (out[1] + dx).clamp(COL_MIN, 900.0);
    } else if i >= 2 {
        out[2] = (out[2] + dx).clamp(COL_MIN, 900.0);
    }
    out
}

/// Column widths kept in ui.json (`links_cols3`), so the window reopens as left.
fn load_cols() -> [f32; 3] {
    let v: serde_json::Value = config_dir()
        .and_then(|d| std::fs::read_to_string(d.join("ui.json")).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut out = COLS_DEFAULT;
    if let Some(a) = v.get("links_cols3").and_then(|x| x.as_array()) {
        for (i, w) in a.iter().take(3).enumerate() {
            if let Some(w) = w.as_f64().filter(|w| w.is_finite()) {
                out[i] = (w as f32).clamp(COL_MIN, 900.0);
            }
        }
    }
    out
}

fn save_cols(cols: &[f32; 3]) {
    let Some(path) = config_dir().map(|d| d.join("ui.json")) else { return };
    let mut v: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("links_cols3".into(), serde_json::json!(cols.iter().map(|w| w.round()).collect::<Vec<_>>()));
        if let Ok(json) = serde_json::to_string_pretty(&v) {
            let _ = crate::model::atomic_write(&path, json.as_bytes());
        }
    }
}

pub fn is_video_page_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.contains("youtu.be/")
        || lower.contains("youtube.com/watch")
        || lower.contains("youtube.com/shorts")
        || lower.contains("youtube.com/embed")
        || lower.contains("youtube.com/v/")
        || (lower.contains("youtube.com") && (lower.contains("?v=") || lower.contains("&v=")))
        || lower.starts_with("/watch?v=")
        || lower.contains("watch?v=")
    {
        return true;
    }
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    let path = u.path().to_ascii_lowercase();
    if host.contains("youtu.be") {
        let seg = path.trim_matches('/');
        return !seg.is_empty();
    }
    if host.contains("youtube.com") {
        return path.contains("/watch")
            || path.contains("/shorts/")
            || path.contains("/embed/")
            || path.contains("/v/")
            || u.query_pairs().any(|(k, _)| k == "v");
    }
    if host.contains("vimeo.com")
        || host.contains("dailymotion.com")
        || host.contains("twitch.tv")
        || host.contains("bilibili.com")
        || host.contains("soundcloud.com")
    {
        return true;
    }
    crate::engine::social_video_page(url)
}

/// A name for a link whose URL names no file (a folder listing, a page):
/// its link text, else the last part of the URL, as an .html page.
fn page_name(url: &str, text: &str) -> String {
    let from_url = || {
        let u = reqwest::Url::parse(url).ok()?;
        let tail = u
            .query_pairs()
            .last()
            .map(|(_, v)| v.into_owned())
            .or_else(|| u.path_segments().and_then(|mut s| s.next_back().map(|x| x.to_string())))
            .filter(|t| !t.trim().is_empty())
            .or_else(|| u.host_str().map(|h| h.to_string()))?;
        Some(tail.rsplit('/').find(|p| !p.trim().is_empty()).unwrap_or(&tail).to_string())
    };
    let t = text.trim();
    let base = if !t.is_empty() && t.chars().count() <= 80 && !t.contains('{') {
        t.to_string()
    } else {
        from_url().unwrap_or_else(|| "page".into())
    };
    engine::sanitize_filename(&format!("{base}.html"))
}

/// Entry point for `--links <file>`.
pub fn run() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .iter()
        .position(|a| a == "--links")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let batch = match read_batch(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("--links: {e}");
            std::process::exit(2);
        }
    };
    let prefs = crate::confirm::load_prefs();
    let rows = batch
        .links
        .into_iter()
        .map(|l| {
            let is_vid = is_video_page_url(&l.url);
            let mut name = engine::sanitize_filename(&filename_from_url(&l.url));
            if is_vid {
                let t = l.text.trim();
                let base = if !t.is_empty() && t.chars().count() <= 120 && !t.contains('{') {
                    t.to_string()
                } else if !crate::model::is_fallback_filename(&name) && name.contains('.') {
                    name.clone()
                } else {
                    let from_u = reqwest::Url::parse(&l.url).ok()
                        .and_then(|u| {
                            if u.host_str().unwrap_or("").to_ascii_lowercase().contains("youtu.be") {
                                let seg = u.path().trim_matches('/');
                                if !seg.is_empty() { return Some(seg.to_string()); }
                            }
                            u.query_pairs().find(|(k, _)| k == "v").map(|(_, v)| v.into_owned())
                        })
                        .unwrap_or_else(|| "video".to_string());
                    format!("video_{from_u}")
                };
                name = engine::sanitize_filename(&if base.ends_with(".mp4") || base.ends_with(".mkv") || base.ends_with(".webm") {
                    base
                } else {
                    format!("{base}.mp4")
                });
            } else if crate::model::is_fallback_filename(&name) {
                name = page_name(&l.url, &l.text);
            }
            let (_, html, image) = if is_vid {
                ("Video".to_string(), false, false)
            } else {
                type_of(&name, None)
            };
            let kind = if is_vid { Kind::Video } else { Kind::of(&name, html, image) };
            // Files and videos start checked; pages and links to other sites do not.
            let checked = kind != Kind::Page && kind != Kind::Other && kind != Kind::Image;
            Row { url: l.url, text: l.text, name, guessed_name: !is_vid, kind, size: None, checked, probe: Probe::Pending }
        })
        .collect();
    let rt = std::sync::Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime"),
    );
    let has_non_pages = rows.iter().any(|r| r.kind != Kind::Page);
    let mut app = LinksApp {
        page_url: batch.page_url,
        title: batch.title,
        cookie: batch.cookie,
        rows,
        filter: String::new(),
        chip: None,
        hide_pages: has_non_pages,
        sort: None,
        anchor: None,
        by_category: true,
        one_dir: prefs.save_dir.clone(),
        cols: load_cols(),
        prefs,
        probe_rx: None,
        probes_left: 0,
        _rt: rt,
        notice: String::new(),
        centered: false,
        focused: false,
        done: false,
        exited: false,
        quality_pref: "best".to_string(),
        q_open: false,
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Download all links — SA2 Downloader")
        .with_inner_size([LINKS_W, LINKS_H])
        .with_min_inner_size([640.0, 380.0])
        .with_resizable(true)
        .with_always_on_top();
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    eframe::run_native(
        "Download all links",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| {
            crate::app::setup_fonts(cc);
            crate::app::apply_theme(&cc.egui_ctx, &crate::app::stored_theme());
            app.start_probes(cc.egui_ctx.clone());
            Ok(Box::new(app))
        }),
    )
}

const LINKS_W: f32 = 860.0;
const LINKS_H: f32 = 560.0;

/// The window's opening size: the design size, but never more than about
/// half of the screen's free area (on a small or zoomed screen the full
/// size covered everything behind it).
fn links_size(work: Option<egui::Rect>) -> egui::Vec2 {
    let (w, h) = match work {
        Some(r) if r.width() > 120.0 && r.height() > 120.0 => (r.width() * 0.55, r.height() * 0.62),
        _ => (LINKS_W, LINKS_H),
    };
    egui::vec2(LINKS_W.min(w).max(640.0), LINKS_H.min(h).max(380.0))
}

impl eframe::App for LinksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(text) = crate::i18n::take_font_text() {
            crate::fonts::ensure_text(ctx, &text);
        }
        crate::fonts::ensure_text(ctx, &self.title);
        for r in &self.rows {
            crate::fonts::ensure_text(ctx, &r.name);
            crate::fonts::ensure_text(ctx, &r.text);
        }
        if self.done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            if !self.exited {
                self.exited = true;
                // Let wake_ids launch the main window before this exits.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    std::process::exit(0);
                });
            }
            return;
        }
        if !self.centered {
            let size = links_size(crate::model::work_area_rect(ctx));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            self.centered = crate::model::center_window(ctx, size);
        }
        if self.centered && !self.focused {
            self.focused = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.collect_probes();
        let frame = egui::Frame::new().fill(pl().bg).inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 12 });
        egui::TopBottomPanel::top("links_top").frame(frame).show_separator_line(false).show(ctx, |ui| {
            self.header(ui);
            ui.add_space(10.0);
            self.toolbar(ui);
        });
        egui::TopBottomPanel::bottom("links_bottom").frame(frame).show_separator_line(false).show(ctx, |ui| {
            self.footer(ui);
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pl().bg).inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 0 }))
            .show(ctx, |ui| self.table(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_come_from_content_type_then_name() {
        assert_eq!(type_of("A.Knight.S01E01.mp4", None), ("MP4 video".to_string(), false, false));
        assert_eq!(type_of("index.php", None).1, true);
        assert_eq!(type_of("S01", None).1, true, "no extension, not probed: a directory page");
        assert_eq!(type_of("S01", Some("text/html; charset=utf-8")).0, "Web page");
        assert!(type_of("logo.png", None).2);
        assert!(type_of("photo", Some("image/jpeg")).2);
        assert_eq!(type_of("file.bin", Some("application/octet-stream")).0, "BIN file");
        assert_eq!(type_of("watch", Some("video/mp4")).0, "video/mp4");
        // Kinds for the chips, icons and colours.
        assert_eq!(Kind::of("a.mp4", false, false), Kind::Video);
        assert_eq!(Kind::of("a.mp3", false, false), Kind::Audio);
        assert_eq!(Kind::of("a.zip", false, false), Kind::Zip);
        assert_eq!(Kind::of("S01.html", true, false), Kind::Page);
        assert_eq!(Kind::of("x", false, true), Kind::Image);
    }

    #[test]
    fn dividers_follow_the_pointer_and_name_pages() {
        let c = COLS_DEFAULT;
        // Name | Type: both sized, sum kept.
        let d = drag_divider(&c, 0, 40.0);
        assert_eq!((d[0], d[1]), (c[0] + 40.0, c[1] - 40.0));
        // Size | Download from: Size grows, the flexible column gives way.
        assert_eq!(drag_divider(&c, 2, 30.0)[2], c[2] + 30.0);
        // Minimums hold.
        assert_eq!(drag_divider(&c, 0, 900.0)[1], COL_MIN);
        let w = col_widths(&c, 1000.0);
        assert_eq!(w.iter().sum::<f32>(), 1000.0);
        assert_eq!(col_widths(&c, 100.0)[2], 140.0);
        // Folder links name a page, not "download_<time>.bin".
        assert_eq!(page_name("https://d.example/?dir=Series/English/A", "A"), "A.html");
        assert_eq!(page_name("https://d.example/?dir=Series/English/A", ""), "A.html");
        assert_eq!(page_name("https://d.example/", ".cls-1{fill:url(#g);}"), "d.example.html");
        assert!(type_of(&page_name("https://d.example/?dir=S01", "S01"), None).1);
    }

    #[test]
    fn host_and_folder_for_the_from_column() {
        assert_eq!(
            host_and_folder("https://download.sunplex.net/Series/English/A/A.Knight.of.the.Seven.Kingdoms/S01/E01.mp4"),
            ("download.sunplex.net".to_string(), "S01".to_string())
        );
        assert_eq!(host_and_folder("https://sunplex.net/"), ("sunplex.net".to_string(), String::new()));
        assert_eq!(host_and_folder("https://h.example/My%20Show/x.mp4").1, "My Show");
    }

    #[test]
    fn page_cookies_stay_on_the_page_host() {
        assert!(same_host("https://download.example.net/a.mp4", "https://download.example.net/?dir=x"));
        assert!(!same_host("https://twitter.com/x", "https://download.example.net/"));
        assert!(!same_host("not a url", "https://download.example.net/"));
    }

    #[test]
    fn links_file_must_come_from_the_config_dir() {
        assert!(read_batch(std::path::Path::new("C:/Windows/links_1.json")).is_err());
        if let Some(dir) = config_dir() {
            assert!(read_batch(&dir.join("queue.json")).is_err());
        }
    }

    #[test]
    fn video_page_urls_are_detected() {
        assert!(is_video_page_url("https://www.youtube.com/watch?v=P7yRYiBiV3g"));
        assert!(is_video_page_url("https://youtu.be/P7yRYiBiV3g"));
        assert!(is_video_page_url("https://www.youtube.com/shorts/aqz-KE-bpKQ"));
        assert!(is_video_page_url("https://vimeo.com/12345678"));
        assert!(!is_video_page_url("https://example.com/index.html"));
    }
}
