//! IDM-style standalone "Download File Info" dialog.
//!
//! Launched as a separate OS window (always on top) when the browser
//! extension captures a download:
//!   `sa2_downloader.exe --confirm-download --url <url> [--filename ..]
//!    [--referer ..] [--cookie ..] [--mime ..]`
//!
//! Unlike the in-canvas Add modal in the main window, this pops in front of
//! whatever the user is doing — one window per captured download, like IDM.
//! It writes straight to the shared `queue.json` (+ `wake.json` for
//! Start Download), so it works even when the main app is closed.

use crate::app::pl;
use crate::dm::*;
use crate::engine::{self, EngineOpts};
use crate::model::{
    atomic_write, category_of, config_dir, filename_from_url, format_bytes, is_download_url,
    DownloadItem,
};
use std::sync::mpsc::Receiver;

/// Seconds → "3:26" badge for browser-known media durations.
fn format_dur(secs: u64) -> String {
    if secs == 0 || secs >= 86400 {
        return String::new();
    }
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn wake_path() -> Option<std::path::PathBuf> {
    config_dir().map(|p| p.join("wake.json"))
}

fn ui_path() -> Option<std::path::PathBuf> {
    config_dir().map(|p| p.join("ui.json"))
}

/// Claim (remove) our URL from the native inbox so the main window does not
/// prompt for it too. Runs at dialog open; Cancel therefore discards.
fn claim_inbox(url: &str) -> Option<crate::native_host::InboxItem> {
    let path=crate::native_host::inbox_path()?;
    let guard=crate::model::fs_lock("inbox");
    if !guard.owned() {return None;}
    claim_inbox_at(&path,url)
}
fn claim_inbox_at(path:&std::path::Path,url:&str) -> Option<crate::native_host::InboxItem> {
    let data=std::fs::read_to_string(path).ok()?;
    // Retain unreadable pending captures for recovery.
    let items:Vec<crate::native_host::InboxItem>=serde_json::from_str(&data).ok()?;
    let claimed=items.iter().find(|i|i.url==url)?.clone();
    let rest:Vec<_>=items.into_iter().filter(|i|i.url!=url).collect();
    if rest.is_empty() {std::fs::remove_file(path).ok()?;}
    else {atomic_write(path,serde_json::to_string_pretty(&rest).ok()?.as_bytes()).ok()?;}
    Some(claimed)
}

#[derive(Debug, Default)]
pub(crate) struct UiPrefs {
    pub(crate) save_dir: String,
    pub(crate) connections: usize,
    pub(crate) category_folders: bool,
    pub(crate) user_agent: String,
    pub(crate) proxy_url: String,
    pub(crate) timeout_secs: u64,
    pub(crate) rename_template: String,
    pub(crate) folder_rules: Vec<crate::model::FolderRule>,
}

pub(crate) fn load_prefs() -> UiPrefs {
    let mut p = UiPrefs {
        connections: 8,
        category_folders: true,
        timeout_secs: 60,
        ..Default::default()
    };
    let v: serde_json::Value = ui_path()
        .and_then(|x| std::fs::read_to_string(x).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    if let Some(s) = v.get("save_dir").and_then(|x| x.as_str()) {
        if !s.trim().is_empty() {
            p.save_dir = s.to_string();
        }
    }
    if p.save_dir.is_empty() {
        p.save_dir = dirs::download_dir()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| crate::model::default_download_dir().to_string_lossy().into_owned());
    }
    if let Some(n) = v.get("connections_default").and_then(|x| x.as_u64()) {
        p.connections = (n as usize).clamp(1, 32);
    }
    if let Some(b) = v.get("category_folders").and_then(|x| x.as_bool()) {
        p.category_folders = b;
    }
    if let Some(s) = v.get("user_agent").and_then(|x| x.as_str()) {
        p.user_agent = s.to_string();
    }
    if let Some(s) = v.get("proxy_url").and_then(|x| x.as_str()) {
        p.proxy_url = s.to_string();
    }
    if let Some(n) = v.get("timeout_secs").and_then(|x| x.as_u64()) {
        p.timeout_secs = n.clamp(5, 600);
    }
    if let Some(s) = v.get("rename_template").and_then(|x| x.as_str()) {
        if !s.trim().is_empty() {
            p.rename_template = s.to_string();
        }
    }
    if p.rename_template.trim().is_empty() {
        p.rename_template = "{name}".to_string();
    }
    if let Some(arr) = v.get("folder_rules").and_then(|x| x.as_array()) {
        let rules: Vec<crate::model::FolderRule> = arr
            .iter()
            .filter_map(|r| {
                Some(crate::model::FolderRule {
                    exts: r.get("exts")?.as_str()?.to_string(),
                    folder: r.get("folder")?.as_str()?.to_string(),
                })
            })
            .take(8)
            .collect();
        if !rules.is_empty() {
            p.folder_rules = rules;
        }
    }
    if p.folder_rules.is_empty() {
        p.folder_rules = crate::model::default_folder_rules();
    }
    p
}

fn save_remember_category(value: bool) {
    let Some(path) = ui_path() else { return };
    let mut v: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "category_folders".to_string(),
            serde_json::Value::Bool(value),
        );
        if let Ok(json) = serde_json::to_string_pretty(&v) {
            let _ = atomic_write(&path, json.as_bytes());
        }
    }
}

/// True when the main window is already running (fresh heartbeat).
/// Replaces process-name counting (two dialogs used to look "live").
fn main_app_running() -> bool {
    crate::model::is_main_live()
}

/// Launch the main window detached (fire-and-forget).
fn launch_main_app() {
    let Ok(exe) = crate::platform::launch_path() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS so the GUI outlives this dialog.
        cmd.creation_flags(0x00000008);
    }
    let _ = cmd.spawn();
}

struct ConfirmApp {
    url: String,
    filename: String,
    category: String,
    remember_path: bool,
    save_base: String,
    description: String,
    referer: String,
    cookie: String,
    /// Captured request headers of this browser capture (from the inbox).
    headers: std::collections::BTreeMap<String, String>,
    /// The browser's side of the capture (method/body, headers, IP, proxy).
    browser: Option<crate::model::BrowserContext>,
    pair_url: String,
    pair_mime: String,
    /// Page/media MIME from the extension (first hint for naming when the
    /// URL carries no usable name and no probe has run yet).
    page_mime: String,
    /// Media duration in seconds from the extension (best-effort badge).
    duration_secs: u64,
    expects_audio: bool,
    quality_pref: String,
    connections: usize,
    user_agent: String,
    proxy_url: String,
    timeout_secs: u64,
    rename_template: String,
    folder_rules: Vec<crate::model::FolderRule>,
    probing: bool,
    probe_size: Option<u64>,
    /// Size the browser measured in its own session (`--size`): what a
    /// refused off-session re-probe falls back to instead of "—".
    browser_size: Option<u64>,
    /// The name the dialog made up itself (no `--filename`), while the user
    /// has not changed it: a server/title name may replace exactly that.
    auto_name: Option<String>,
    probe_rx: Option<Receiver<(Option<u64>, Option<String>)>>,
    rt: Option<std::sync::Arc<tokio::runtime::Runtime>>,
    centered: bool,
    show_adv: bool,
    done: bool,
    exited: bool,
    notice: String,
    /// State for controls the concept does not show (dropdown + extras).
    cat_open: bool,
    /// Path field's recent-folders menu (same pattern as the category combo).
    recent_open: bool,
    win_h: f32,
    adding_cat: bool,
    new_cat: String,
    custom_cats: Vec<String>,
    is_playlist: bool,
    quality_open: bool,
    loading_playlist: bool,
    playlist_rx: Option<Receiver<anyhow::Result<(String, Vec<(String, String)>)>>>,
    real_formats: Vec<engine::RealVideoFormat>,
    formats_rx: Option<Receiver<Option<engine::RealVideoInfo>>>,
}

const CATEGORIES: &[&str] = &[
    "Video",
    "Music",
    "Image",
    "Document",
    "Compressed",
    "Apps",
    "Other",
];

// ---------- concept geometry (tokens live in `crate::dm`) ----------
// The capture popup is the same "Add download" card as the in-canvas modal,
// so it reuses `dm`'s tokens and row offsets verbatim: 562x253 at rest, with
// the header's window controls, the size chip and the gear panel in the same
// places.
/// Advanced (gear) panel height: category + description, referer + cookie,
/// connections, remember-path. The concept draws this collapsed.
const PANEL_H: f32 = 178.0;
/// Height of one dim note line inside the panel.
const NOTE_H: f32 = 12.0;

/// Loads Segoe UI — the family the concept asks for — plus a semibold cut for
/// its 600-weight title and primary button. Missing system fonts fall back to
/// the bundled defaults, so this is safe on any machine.
fn setup_popup_fonts(cc: &eframe::CreationContext<'_>) {
    // `true`: this whole window is the concept, so Segoe UI also becomes its
    // default proportional face.
    crate::fonts::install(cc, true);
}

/// Concept "+" square: accent fill, white plus (adds a custom category).
fn plus_square(ui: &mut egui::Ui, rect: egui::Rect, salt: u64) -> egui::Response {
    let r = ui.interact(rect, egui::Id::new(("popup-plus", salt)), egui::Sense::click());
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let p = ui.painter();
    p.rect(
        rect,
        DM_RAD,
        if r.hovered() { DM_ACC2 } else { DM_ACC },
        egui::Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let st = egui::Stroke::new(2.0_f32, egui::Color32::WHITE);
    p.line_segment([egui::pos2(c.x - 6.0, c.y), egui::pos2(c.x + 6.0, c.y)], st);
    p.line_segment([egui::pos2(c.x, c.y - 6.0), egui::pos2(c.x, c.y + 6.0)], st);
    r
}

pub(crate) fn cat_icon(cat: &str) -> Option<(DmIcon, egui::Color32)> {
    match cat {
        "Video" => Some((DmIcon::Video, COLOR_BLEU_DE_FRANCE)),
        "Music" => Some((DmIcon::Music, egui::Color32::from_rgb(0xEF, 0x44, 0x44))),
        "Image" => Some((DmIcon::Image, egui::Color32::from_rgb(0x06, 0xB6, 0xD4))),
        "Document" => Some((DmIcon::Doc, egui::Color32::from_rgb(0x64, 0x74, 0x8B))),
        "Compressed" => Some((DmIcon::Archive, egui::Color32::from_rgb(0xD9, 0x77, 0x06))),
        "Apps" => Some((DmIcon::AppWindow, egui::Color32::from_rgb(0x8B, 0x5C, 0xF6))),
        _ => None,
    }
}

/// Floating option list anchored under `resp` — the concept's dropdown body,
/// shared by the category combo and the path field's recent folders, so both
/// menus look and behave identically.
fn options_menu(
    resp: &egui::Response,
    options: &[String],
    open: &mut bool,
    selected: &str,
) -> Option<String> {
    if options.is_empty() {
        *open = false;
        return None;
    }
    let align = if resp.rect.left() > 200.0 || resp.rect.right() > 380.0 {
        egui::RectAlign::BOTTOM_END
    } else {
        egui::RectAlign::BOTTOM_START
    };
    let mut picked = None;
    egui::Popup::from_response(resp)
        .open_bool(&mut *open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(align)
        .gap(6.0)
        .frame(
            egui::Frame::new()
                .fill(pl().dm_card)
                .stroke(egui::Stroke::new(1.0_f32, pl().dm_line))
                .corner_radius(DM_RAD)
                .inner_margin(egui::Margin::symmetric(4, 4)),
        )
        .show(|ui| {
            let menu_w = resp.rect.width().clamp(240.0, 360.0);
            ui.set_min_width(menu_w);
            ui.set_max_width(380.0);
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                    let w = ui.available_width().max(menu_w - 8.0);
                    for o in options {
                        let (r, ir) =
                            ui.allocate_exact_size(egui::vec2(w, 24.0), egui::Sense::click());
                        if ir.hovered() {
                            ui.painter().rect_filled(r, 4.0, pl().dm_hover);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if let Some((ic, col)) = cat_icon(o) {
                            let ic_r = egui::Rect::from_center_size(egui::pos2(r.left() + 16.0, r.center().y), egui::Vec2::splat(12.0));
                            dm_icon(ui.painter(), ic_r, ic, col, 1.8);
                            ui.painter().text(
                                egui::pos2(r.left() + 28.0, r.center().y),
                                egui::Align2::LEFT_CENTER,
                                o.as_str(),
                                reg(11.0),
                                if o.as_str() == selected { DM_ACC } else { pl().dm_txt },
                            );
                        } else {
                            ui.painter().text(
                                egui::pos2(r.left() + 8.0, r.center().y),
                                egui::Align2::LEFT_CENTER,
                                o.as_str(),
                                reg(11.0),
                                if o.as_str() == selected { DM_ACC } else { pl().dm_txt },
                            );
                        }
                        if ir.clicked() {
                            picked = Some(o.clone());
                        }
                    }
                });
        });
    if picked.is_some() {
        *open = false;
    }
    picked
}

/// Concept dropdown: field box, current value, trailing caret, option list.
pub(crate) fn combo(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &str,
    current: &str,
    options: &[String],
    open: &mut bool,
) -> Option<String> {
    let resp = ui.interact(rect, egui::Id::new(("popup-combo", id)), egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let p = ui.painter();
    dm_box(p, rect);
    let text_x = if id.contains("cat") {
        if let Some((ic, col)) = cat_icon(current) {
            let ic_r = egui::Rect::from_center_size(egui::pos2(rect.left() + 18.0, rect.center().y), egui::Vec2::splat(14.0));
            dm_icon(p, ic_r, ic, col, 2.0);
            rect.left() + 32.0
        } else {
            rect.left() + 13.0
        }
    } else {
        rect.left() + 13.0
    };
    p.text(
        egui::pos2(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        elide_left(current, 26),
        reg(11.0),
        pl().dm_txt,
    );
    dm_icon(
        p,
        egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, rect.center().y),
            egui::Vec2::splat(11.0),
        ),
        DmIcon::Chevron,
        pl().dm_dim,
        2.0,
    );
    if resp.clicked() {
        *open = !*open;
    }
    options_menu(&resp, options, open, current)
}

/// Concept checkbox row: 16px box plus label.
fn check_row(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &str,
    checked: bool,
    label: &str,
) -> egui::Response {
    let label = crate::i18n::t(label);
    let r = ui.interact(rect, egui::Id::new(("popup-check", id)), egui::Sense::click());
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let box_r = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - 8.0),
        egui::Vec2::splat(16.0),
    );
    dm_check(ui.painter(), box_r, checked);
    ui.painter().text(
        egui::pos2(box_r.right() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        reg(11.0),
        if r.hovered() { pl().dm_txt } else { pl().dm_dim },
    );
    r
}

/// Concept slider with its drag handling on the same rect, so painting and
/// input can never disagree. Returns the new fraction when it changed.
fn slider_row(ui: &mut egui::Ui, rect: egui::Rect, id: &str, frac: f32) -> Option<f32> {
    let r = ui.interact(rect, egui::Id::new(("popup-slider", id)), egui::Sense::click_and_drag());
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut out = None;
    if r.dragged() || r.clicked() {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let f = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            if (f - frac).abs() > 0.001 {
                out = Some(f);
            }
        }
    }
    let shown = out.unwrap_or(frac);
    slider(ui.painter(), rect, r.hovered(), shown, rect.left(), rect.width());
    out
}

impl ConfirmApp {
    fn effective_dir(&self) -> String {
        if self.remember_path {
            // Ordered user rules first, category fallback second (mirrors app).
            if let Some(rule) = self
                .folder_rules
                .iter()
                .find(|r| r.matches(&self.filename))
            {
                if !rule.folder.trim().is_empty() {
                    let mut p = std::path::PathBuf::from(&self.save_base);
                    p.push(rule.folder.trim());
                    return p.to_string_lossy().to_string();
                }
            }
            let mut p = std::path::PathBuf::from(&self.save_base);
            p.push(self.category.clone());
            p.to_string_lossy().to_string()
        } else {
            self.save_base.clone()
        }
    }

    /// Recent folders for the path field's caret: the shell folders first,
    /// then whatever the field currently holds (mirrors the main window).
    fn recent_dirs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for d in [
            dirs::download_dir(),
            dirs::desktop_dir(),
            dirs::document_dir(),
            dirs::picture_dir(),
            dirs::video_dir(),
            dirs::audio_dir(),
        ]
        .into_iter()
        .flatten()
        {
            let s = d.to_string_lossy().to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
        let cur = self.save_base.trim().to_string();
        if !cur.is_empty() && !out.contains(&cur) {
            out.push(cur);
        }
        out
    }

    /// Extra one-line facts the concept has no place for; shown under the
    /// hidden extras panel only.
    fn extra_notes(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.pair_url.starts_with("http") {
            v.push("video-only + audio — merged on completion (needs ffmpeg)".to_string());
        }
        if !self.quality_pref.trim().is_empty() {
            v.push(format!("Quality: {}", self.quality_pref.trim()));
        }
        v
    }

    fn start_probe(&mut self) {
        if !is_download_url(&self.url) {
            return;
        }
        let Some(rt) = self.rt.clone() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.probe_rx = Some(rx);
        self.probing = true;
        self.probe_size = self.browser_size;
        let url = self.url.clone();
        let referer = self.referer.clone();
        // Quality choice feeds the page resolve so the dialog measures the
        // chosen stream's size, not just the page's (IDM shows size upfront).
        let quality_pref = self.quality_pref.trim().to_string();
        // Bare URL guess to correct with the MIME type when the server
        // sends no disposition (P1: `download_123.bin` + video/mp4).
        let guess_name = self.filename.clone();
        let page_mime = self.page_mime.clone();
        let opts = EngineOpts {
            user_agent: if self.user_agent.trim().is_empty() {
                format!("sa2_downloader/{}", crate::model::APP_VERSION)
            } else {
                self.user_agent.clone()
            },
            timeout_secs: self.timeout_secs,
            proxy_url: self.proxy_url.clone(),
            referer,
            // The browser session claimed from the inbox: without it a
            // login-gated file probes as 403 or as the login page's size.
            cookie: self.cookie.clone(),
            quality_pref,
            captured_headers: engine::CapturedHeaders::for_url(&url, &self.headers),
            browser: self.browser.clone(),
            ..Default::default()
        };
        let f_opts = opts.clone();
        rt.spawn(async move {
            let out = match opts.build_client() {
                Ok(client) => {
                    let (mut total, _, ctype, mut disp) =
                        engine::probe(&client, &opts, &url).await;
                    // A playlist's length is its text, not the video: show
                    // the duration instead (as IDM does for streams).
                    if engine::is_stream_manifest(&url, ctype.as_deref(), &page_mime) {
                        total = None;
                    }
                    let is_page = ctype
                        .as_deref()
                        .map(|c| c.contains("text/html"))
                        .unwrap_or(false)
                        || url.contains("youtube.com")
                        || url.contains("youtu.be");
                    if total.is_none() && is_page {
                        let shared =
                            std::sync::Arc::new(engine::SharedState::new());
                        if let Some(hit) =
                            engine::resolve_media(&client, &opts, &url, &shared).await
                        {
                            let (t2, _, _, _) =
                                engine::probe(&client, &opts, &hit.url).await;
                            if total.is_none() {
                                total = t2;
                            }
                            if let Some(pair) = hit.pair_url.filter(|p| p.starts_with("http")) {
                                let (ta, _, _, _) =
                                    engine::probe(&client, &opts, &pair).await;
                                if let (Some(a), Some(b)) = (total, ta) {
                                    total = Some(a.saturating_add(b));
                                } else {
                                    total = total.or(ta);
                                }
                            }
                            if disp.is_none() && !hit.title.trim().is_empty() {
                                disp = Some(engine::sanitize_filename(&hit.title));
                            }
                        }
                    }
                    if disp.is_none()
                        && crate::model::is_fallback_filename(&guess_name)
                    {
                        // No disposition and no media title: at least fix the
                        // extension — probed MIME first, extension hint when
                        // the probe itself was refused off-session.
                        let ext = ctype
                            .as_deref()
                            .and_then(crate::model::ext_for_mime)
                            .or_else(|| crate::model::ext_for_mime(&page_mime));
                        if let Some(ext) = ext {
                            disp = Some(guess_name.trim_end_matches(".bin").to_string() + ext);
                        }
                    }
                    (total, disp)
                }
                Err(_) => (None, None),
            };
            let _ = tx.send(out);
        });

        let (ftx, frx) = std::sync::mpsc::channel();
        self.formats_rx = Some(frx);
        let f_url = self.url.clone();
        rt.spawn(async move {
            let info = if let Ok(client) = f_opts.build_client() {
                engine::probe_real_video_formats(&client, &f_opts, &f_url).await
            } else {
                None
            };
            let _ = ftx.send(info);
        });
    }

    /// Queue the download. `start` = also wake the main app to begin it now
    /// (IDM "Start Download") vs merely queueing ("Download Later").
    fn commit(&mut self, start: bool) {
        if self.filename.trim().is_empty() {
            self.filename = filename_from_url(&self.url);
        }
        let mut item = DownloadItem::new(
            0, // id assigned atomically under the queue lock below
            self.url.trim().to_string(),
            engine::sanitize_filename(self.filename.trim()),
            self.effective_dir(),
            self.connections,
        );
        item.referer = self.referer.clone();
        item.cookie = self.cookie.clone();
        item.extra_headers = self.headers.clone();
        item.browser = self.browser.clone();
        item.description = self.description.trim().to_string();
        // Browser-measured size (extension HEAD/Range in the real session):
        // keep it so the engine and progress UI know the total even when a
        // server-side re-probe would 403 off-session.
        item.total_bytes = self.probe_size.filter(|s| *s > 0);
        if self.pair_url.starts_with("http") {
            item.pair_url = self.pair_url.clone();
            item.pair_mime = self.pair_mime.clone();
        }
        item.quality_pref = self.quality_pref.trim().to_string();
        item.duration_secs = self.duration_secs;
        item.expects_audio = self.expects_audio;
        item.is_playlist = self.is_playlist;
        let id = match crate::model::append_to_queue(&mut item) {
            crate::model::AppendResult::Added(id) => {
                save_remember_category(self.remember_path);
                id
            }
            crate::model::AppendResult::AlreadyQueued(id) => {
                // Not an error. The URL is already in the list; if the user
                // asked to start it we wake that row, so this is never a dead
                // end. Stay open so the notice is actually read. The notice
                // names what really happens to the row.
                use crate::model::Status;
                self.notice = match crate::model::queued_status(id) {
                    Some(Status::Completed) => format!("Already downloaded (#{id}) — see Finished."),
                    Some(Status::Downloading) => format!("Already downloading (#{id})."),
                    _ if start => {
                        self.wake_targets(&[id]);
                        format!("Already in the queue (#{id}) — starting it.")
                    }
                    _ => format!("Already in the queue (#{id})."),
                };
                return;
            }
            crate::model::AppendResult::Error(e) => {
                // Never close on a failed write: closing with a stale message
                // is exactly what made this look like "already in queue".
                self.notice = format!("Could not update the queue: {e}");
                return;
            }
        };
        save_remember_category(self.remember_path);
        if start {
            self.wake_targets(&[id]);
        }
        self.done = true;
    }

    fn wake_targets(&self, targets: &[u64]) {
        wake_ids(targets);
    }
}

/// Ask the main app to start these ids (consumed on its next poll, or at
/// startup when we launch it below).
pub(crate) fn wake_ids(targets: &[u64]) {
    if let Some(p) = wake_path() {
        let _guard = crate::model::fs_lock("wake");
        if !_guard.owned() { return; }
        let mut ids: Vec<u64> = std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("start").cloned())
            .and_then(|v| serde_json::from_value::<Vec<u64>>(v).ok())
            .unwrap_or_default();
        for id in targets {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        if let Ok(json) =
            serde_json::to_string_pretty(&serde_json::json!({ "start": ids }))
        {
            let _ = atomic_write(&p, json.as_bytes());
        }
    }
    // tasklist + cold-start spawn can take seconds (debug builds
    // especially) — never block the dialog on it; close at once.
    std::thread::spawn(|| {
        if !main_app_running() {
            launch_main_app();
        }
    });
}

/// Entry point for `--confirm-download`. Never returns normally
/// (runs the dialog window until it closes).
pub fn run() -> eframe::Result<()> {
    // Index-iterated flag parsing: a VALUE that looks like a flag (e.g. a
    // `Content-Disposition` filename of `--referer`) must not shift parsing.
    // Only the token immediately following a known flag is consumed.
    const KNOWN: &[&str] = &[
        "--url",
        "--filename",
        "--referer",
        "--cookie",
        "--mime",
        "--size",
        "--pair-url",
        "--pair-mime",
        "--quality",
        "--dur",
        "--is-playlist",
    ];
    let args: Vec<String> = std::env::args().collect();
    let mut map: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(flag) = KNOWN.iter().find(|k| **k == args[i].as_str()) {
            if i + 1 < args.len() {
                map.insert(*flag, args[i + 1].clone());
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    let mut get = |name: &str| -> Option<String> { map.remove(name) };
    let url = get("--url").unwrap_or_default();
    if !is_download_url(&url) {
        eprintln!("--confirm-download needs --url <http(s)://...>");
        std::process::exit(2);
    }
    let page_mime = get("--mime").filter(|s| !s.trim().is_empty()).unwrap_or_default();
    // `get` consumes the flag, so read it once.
    let caller_name = get("--filename").filter(|f| !f.trim().is_empty());
    let named_by_caller = caller_name.is_some();
    let filename = caller_name.unwrap_or_else(|| {
        crate::model::filename_from_url_with_mime(&url, Some(page_mime.as_str()))
    });
    let referer = get("--referer").unwrap_or_default();
    let mut cookie = get("--cookie").unwrap_or_default();
    let pair_url = get("--pair-url").filter(|s| s.starts_with("http")).unwrap_or_default();
    let pair_mime = get("--pair-mime").unwrap_or_default();
    let mut quality_pref = get("--quality").filter(|s| !s.trim().is_empty()).unwrap_or_default();
    // Browser-measured size (menu HEAD/Range): trust it, skip probing.
    let known_size: Option<u64> = get("--size")
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s > 0)
        // An older extension sends a playlist's text length as its size.
        .filter(|_| !engine::is_stream_manifest(&url, None, &page_mime));
    // Browser-known media duration in seconds (best-effort badge).
    let duration_secs: u64 = get("--dur")
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s > 0 && *s < 86400)
        .unwrap_or(0);
    let is_playlist_cli = args.iter().any(|s| s == "--is-playlist");
    let is_pure_pl = engine::is_pure_playlist_url(&url);
    let mut is_playlist = is_playlist_cli || is_pure_pl;

    // Claim the inbox entry first so the main window won't prompt too.
    let mut headers = std::collections::BTreeMap::new();
    let mut browser = None;
    if let Some(item)=claim_inbox(url.trim()) {
        browser = item.browser.clone();
        // The native host leaves credentials in the private inbox, not the
        // process command line where other local users can inspect them.
        if cookie.is_empty() {cookie=item.cookie.unwrap_or_default();}
        headers = item.headers.unwrap_or_default();
        if item.is_playlist {
            is_playlist = true;
        }
        if quality_pref.is_empty() {
            if let Some(q) = item.quality {
                quality_pref = q;
            }
        }
    }

    let prefs = load_prefs();
    let category = category_of(&filename).to_string();
    // A row that is still usable (queued/downloading/paused, or finished with
    // its file present) is reported up front. A dead row (failed, or finished
    // with the file gone) is refreshed by the capture instead, so the buttons
    // never dead-end on a stale "already in queue".
    let initial_notice = match crate::model::queued_entry(url.trim(), &quality_pref) {
        Some((id, false)) => format!("Already in the queue (#{id})."),
        _ => String::new(),
    };

    let rt = std::sync::Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"),
    );
    let start_name = crate::model::apply_rename_template(&prefs.rename_template, &engine::sanitize_filename(&filename), url.trim());
    let mut app = ConfirmApp {
        auto_name: (!named_by_caller).then(|| start_name.clone()),
        browser_size: known_size,
        url: url.trim().to_string(),
        filename: start_name,
        category,
        remember_path: prefs.category_folders,
        save_base: prefs.save_dir,
        description: String::new(),
        referer,
        cookie,
        headers,
        browser,
        pair_url,
        pair_mime,
        page_mime,
        duration_secs,
        expects_audio: args.iter().any(|s| s == "--expect-audio"),
        quality_pref,
        connections: prefs.connections,
        user_agent: prefs.user_agent,
        proxy_url: prefs.proxy_url,
        timeout_secs: prefs.timeout_secs,
        rename_template: prefs.rename_template,
        folder_rules: prefs.folder_rules,
        probing: false,
        probe_size: known_size,
        probe_rx: None,
        rt: Some(rt.clone()),
        centered: false,
        show_adv: false,
        done: false,
        exited: false,
        notice: initial_notice,
        cat_open: false,
        recent_open: false,
        win_h: 0.0,
        adding_cat: false,
        new_cat: String::new(),
        custom_cats: Vec::new(),
        is_playlist,
        quality_open: false,
        loading_playlist: false,
        playlist_rx: None,
        real_formats: Vec::new(),
        formats_rx: None,
    };
    app.start_probe();
    let _rt_keepalive = rt;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Add download")
        .with_inner_size([DM_W, DM_H])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top();
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    eframe::run_native(
        "Add download",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| {
            setup_popup_fonts(cc);
            crate::app::apply_theme(&cc.egui_ctx, &crate::app::stored_theme());
            Ok(Box::new(app))
        }),
    )
}

impl eframe::App for ConfirmApp {
    /// The window is transparent and frameless: the concept's rounded card is
    /// painted by hand, so the surface behind it must stay clear.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Same bulletproofing as the progress window: Close can no-op, so back
    /// it with a delayed exit once the dialog is done.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(text) = crate::i18n::take_font_text() {
            crate::fonts::ensure_text(ctx, &text);
        }
        crate::fonts::ensure_text(ctx, &self.filename);
        crate::fonts::ensure_text(ctx, &self.description);
        if self.done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            if !self.exited {
                self.exited = true;
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    std::process::exit(0);
                });
            }
            return;
        }
        // Middle of the work area (taskbar excluded), once the monitor
        // geometry is known; Windows would otherwise cascade the window.
        if !self.centered && crate::model::center_window(ctx, egui::vec2(DM_W, DM_H)) {
            self.centered = true;
        }
        // Collect probe result.
        if let Some(rx) = self.probe_rx.as_ref() {
            while let Ok((sz, nm)) = rx.try_recv() {
                self.probing = false;
                // A refused re-probe (off-session 403) must not erase the
                // size the browser measured in its own session.
                self.probe_size = sz.or(self.browser_size);
                if let Some(n) = nm {
                    // Prefer the server/video title while our name is still a
                    // guess we made (or a last-resort fallback whose timestamp
                    // drifted since dialog open) — never over a name the
                    // extension chose or the user typed.
                    let untouched_guess = self.auto_name.as_deref() == Some(self.filename.as_str());
                    if untouched_guess
                        || self.filename == filename_from_url(&self.url)
                        || crate::model::is_fallback_filename(&self.filename)
                    {
                        self.filename = crate::model::apply_rename_template(
                            &self.rename_template,
                            &n,
                            &self.url,
                        );
                        self.category = category_of(&self.filename).to_string();
                        if untouched_guess {
                            self.auto_name = Some(self.filename.clone());
                        }
                    }
                }
            }
        }
        if let Some(rx) = self.formats_rx.as_ref() {
            while let Ok(Some(info)) = rx.try_recv() {
                if let Some(t) = info.title {
                    let untouched_guess = self.auto_name.as_deref() == Some(self.filename.as_str());
                    if untouched_guess
                        || self.filename == filename_from_url(&self.url)
                        || crate::model::is_fallback_filename(&self.filename)
                    {
                        self.filename = crate::model::apply_rename_template(
                            &self.rename_template,
                            &engine::sanitize_filename(&t),
                            &self.url,
                        );
                        self.category = category_of(&self.filename).to_string();
                        if untouched_guess {
                            self.auto_name = Some(self.filename.clone());
                        }
                    }
                }
                if let Some(d) = info.duration_secs {
                    if self.duration_secs == 0 {
                        self.duration_secs = d;
                    }
                }
                if !info.formats.is_empty() {
                    if self.probe_size.is_none() || self.probe_size == Some(0) {
                        if let Some(first_sz) = info.formats.first().and_then(|f| f.size) {
                            self.probe_size = Some(first_sz);
                        }
                    }
                    self.real_formats = info.formats;
                }
            }
        }
        if let Some(rx) = self.playlist_rx.as_ref() {
            if let Ok(res) = rx.try_recv() {
                self.loading_playlist = false;
                match res {
                    Ok((title, entries)) => {
                        let links: Vec<crate::native_host::PageLink> = entries
                            .into_iter()
                            .map(|(u, t)| crate::native_host::PageLink { url: u, text: t })
                            .collect();
                        if !links.is_empty() {
                            let _ = crate::native_host::open_links_window(&self.url, &title, &self.cookie, &links);
                            self.done = true;
                            return;
                        } else {
                            self.notice = "Playlist had no videos to load.".to_string();
                        }
                    }
                    Err(e) => {
                        self.notice = format!("Could not load playlist: {e:#}");
                    }
                }
            }
        }
        if self.probing || self.loading_playlist {
            ctx.request_repaint();
        }
        // Grow the window when the gear panel (or a notice) needs room, so the
        // concept surface itself never changes size or position.
        let notes = self.extra_notes();
        let panel_h = if self.show_adv {
            PANEL_H + if notes.is_empty() { 0.0 } else { 4.0 + NOTE_H * notes.len() as f32 }
        } else {
            0.0
        };
        let adv_h = if self.show_adv { panel_h + 14.0 } else { 0.0 };
        let is_media = engine::is_playlist_url_or_query(&self.url)
            || self.category == "Video"
            || self.category == "Music"
            || engine::is_youtube_page(&self.url)
            || engine::social_video_page(&self.url)
            || !self.quality_pref.is_empty();
        let pl_h = if is_media { 36.0 } else { 0.0 };
        let mut want_h = DM_H + adv_h + pl_h;
        if !self.notice.is_empty() {
            want_h += 16.0;
        }
        if (self.win_h - want_h).abs() > 0.5 {
            self.win_h = want_h;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(DM_W, want_h)));
        }
        // Esc closes, unless a menu is open (those close on the same key).
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.cat_open && !self.recent_open && !self.quality_open {
            self.done = true;
        }

        let panel = egui::CentralPanel::default().frame(egui::Frame::NONE);
        panel.show(ctx, |ui| {
            // The concept card IS the window: 562x253, 1px #1e293b border,
            // 8px radius, painted edge to edge over the clear surface.
            let card = ui.max_rect();
            let o = card.min;
            let p = ui.painter().clone();
            let at = |x: f32, y: f32, w: f32, h: f32| {
                egui::Rect::from_min_size(egui::pos2(o.x + x, o.y + y), egui::vec2(w, h))
            };
            p.rect(
                card,
                8.0,
                pl().dm_card,
                egui::Stroke::new(1.0_f32, pl().dm_line),
                egui::StrokeKind::Inside,
            );
            // ---- header: mark + title + window controls (drag to move) ----
            dm_mark(&p, at(15.0, 11.0, 20.0, 20.0));
            p.text(
                egui::pos2(o.x + 43.0, o.y + 20.0),
                egui::Align2::LEFT_CENTER,
                crate::i18n::t("New Download Task"),
                semi(13.5),
                pl().dm_txt,
            );
            if ui
                .interact(
                    at(0.0, 0.0, DM_W - 130.0, 39.0),
                    egui::Id::new("popup-header"),
                    egui::Sense::click_and_drag(),
                )
                .drag_started()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            let ctl = |x: f32| {
                egui::Rect::from_center_size(
                    egui::pos2(o.x + x, o.y + 20.0),
                    egui::Vec2::splat(12.0),
                )
            };
            let (min_r, max_r, close_r) = (ctl(485.0), ctl(513.0), ctl(541.0));
            let min_b = ui.interact(min_r, egui::Id::new("popup-min"), egui::Sense::click());
            let max_b = ui.interact(max_r, egui::Id::new("popup-max"), egui::Sense::click());
            let close_b = ui.interact(close_r, egui::Id::new("popup-close"), egui::Sense::click());
            for (r, hov) in [
                (min_r, min_b.hovered()),
                (max_r, max_b.hovered()),
                (close_r, close_b.hovered()),
            ] {
                if hov {
                    p.rect_filled(r.expand(4.0), DM_RAD, pl().dm_hover2);
                }
            }
            if min_b.hovered() || max_b.hovered() || close_b.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            dm_icon(&p, min_r, DmIcon::Minimize, pl().dm_dim, 2.0);
            dm_icon(&p, max_r, DmIcon::Maximize, pl().dm_dim, 2.0);
            dm_icon(&p, close_r, DmIcon::Close, pl().dm_dim, 2.0);
            if min_b.clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if max_b.clicked() {
                let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            if close_b.clicked() {
                self.done = true;
            }

            // ---- size measurement ----
            let sz_txt: String = if self.probing {
                "…".to_string()
            } else {
                match self.probe_size.filter(|s| *s > 0) {
                    Some(sz) if self.duration_secs > 0 => {
                        format!("{} • {}", format_dur(self.duration_secs), format_bytes(sz))
                    }
                    Some(sz) => format_bytes(sz),
                    None if self.duration_secs > 0 => format_dur(self.duration_secs),
                    None => "—".to_string(),
                }
            };

            // ---- row 1: File Info (thumbnail with badge + filename with pencil + file size) ----
            let r1_y = 48.0;
            let thumb_r = at(DM_X, r1_y, 44.0, 36.0);
            dm_file_thumbnail(&p, thumb_r, &self.filename);

            let file_w = 340.0;
            let file_box = at(DM_X + 50.0, r1_y, file_w, 36.0);
            let pencil_btn = egui::Rect::from_center_size(
                egui::pos2(file_box.max.x - 16.0, file_box.center().y),
                egui::Vec2::splat(14.0),
            );
            dm_field(
                ui,
                "popup_file",
                file_box,
                pencil_btn.min.x - 4.0,
                &mut self.filename,
                "Filename",
                false,
            );
            dm_icon(&p, pencil_btn, DmIcon::Pencil, pl().dm_dim, 1.8);

            // Size text on right of row 1
            p.text(
                egui::pos2(o.x + DM_W - 17.0, o.y + r1_y + 18.0),
                egui::Align2::RIGHT_CENTER,
                &sz_txt,
                semi(12.5),
                pl().dm_txt,
            );

            // ---- row 2: URL + clipboard button ----
            let r2_y = 92.0;
            p.text(
                egui::pos2(o.x + DM_X, o.y + r2_y + 18.0),
                egui::Align2::LEFT_CENTER,
                crate::i18n::t("URL"),
                semi(12.0),
                pl().dm_txt,
            );
            let url_x = DM_X + 36.0;
            let url_w = DM_W_IN - 36.0;
            let url_box = at(url_x, r2_y, url_w, 36.0);
            let clip_btn = egui::Rect::from_min_size(
                egui::pos2(url_box.max.x - 30.0, url_box.min.y + 4.0),
                egui::Vec2::splat(28.0),
            );
            dm_field(
                ui,
                "popup_url",
                url_box,
                clip_btn.min.x - 4.0,
                &mut self.url,
                "https://…",
                false,
            );
            let clip_b = ui.interact(clip_btn, egui::Id::new("popup-clip"), egui::Sense::click());
            if clip_b.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            icon_btn(&p, clip_btn, clip_b.hovered(), DmIcon::Clipboard, 14.0, 1.5, pl().dm_dim);
            if clip_b.clicked() {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(self.url.clone());
                }
            }

            // ---- row 3: Category dropdown | Save To selector + Available space ----
            let r3_lbl_y = 136.0;
            let r3_y = 152.0;

            // Category
            p.text(
                egui::pos2(o.x + DM_X, o.y + r3_lbl_y),
                egui::Align2::LEFT_TOP,
                crate::i18n::t("Category"),
                reg(10.5),
                pl().dm_dim,
            );
            let cat_w = 145.0;
            let cat_r = at(DM_X, r3_y, cat_w, 36.0);
            let mut cats: Vec<String> = CATEGORIES.iter().map(|c| c.to_string()).collect();
            cats.extend(self.custom_cats.iter().cloned());
            if let Some(pick) = combo(ui, cat_r, "cat", &self.category, &cats, &mut self.cat_open) {
                self.category = pick;
            }

            // Save To
            let save_x = DM_X + cat_w + 12.0;
            let save_w = DM_W_IN - cat_w - 12.0;
            p.text(
                egui::pos2(o.x + save_x, o.y + r3_lbl_y),
                egui::Align2::LEFT_TOP,
                crate::i18n::t("Save To"),
                reg(10.5),
                pl().dm_dim,
            );

            // Available disk space indicator on right
            let free_space = available_disk_space(&self.effective_dir()).or_else(|| available_disk_space(&self.save_base));
            if let Some(free_b) = free_space {
                p.text(
                    egui::pos2(o.x + DM_W - 17.0, o.y + r3_lbl_y),
                    egui::Align2::RIGHT_TOP,
                    format!("Available: {}", format_disk_space(free_b)),
                    reg(10.5),
                    pl().dm_dim,
                );
            }

            let path_box = at(save_x, r3_y, save_w, 36.0);
            let caret_btn = egui::Rect::from_min_size(
                egui::pos2(path_box.max.x - 30.0, path_box.min.y + 4.0),
                egui::Vec2::splat(28.0),
            );
            let folder_btn = egui::Rect::from_min_size(
                egui::pos2(caret_btn.min.x - 28.0, caret_btn.min.y),
                egui::Vec2::splat(28.0),
            );
            dm_field(
                ui,
                "popup_dir",
                path_box,
                folder_btn.min.x - 4.0,
                &mut self.save_base,
                "Save folder",
                false,
            );
            if self.remember_path {
                ui.interact(path_box, egui::Id::new("popup_dir_tip"), egui::Sense::hover())
                    .on_hover_text(format!("→ {}", self.effective_dir()));
            }
            let f_b = ui.interact(folder_btn, egui::Id::new("popup-folder"), egui::Sense::click());
            icon_btn(&p, folder_btn, f_b.hovered(), DmIcon::Folder, 14.0, 1.5, COLOR_LIGHTNING_YELLOW);
            let c_b = ui.interact(caret_btn, egui::Id::new("popup-caret"), egui::Sense::click());
            icon_btn(&p, caret_btn, c_b.hovered(), DmIcon::Chevron, 10.0, 2.0, pl().dm_dim);
            if f_b.hovered() || c_b.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if c_b.clicked() {
                self.recent_open = !self.recent_open;
            }
            let recents = self.recent_dirs();
            if let Some(dir) = options_menu(&c_b, &recents, &mut self.recent_open, &self.save_base) {
                self.save_base = dir;
            }
            if f_b.clicked() {
                let mut dlg = rfd::FileDialog::new();
                if !self.save_base.trim().is_empty() {
                    dlg = dlg.set_directory(&self.save_base);
                }
                if let Some(d) = dlg.pick_folder() {
                    self.save_base = d.to_string_lossy().to_string();
                }
            }

            // Gear button for more options in title bar (right of title)
            let gear_r = egui::Rect::from_center_size(
                egui::pos2(o.x + 450.0, o.y + 20.0),
                egui::Vec2::splat(14.0),
            );
            let g_b = ui.interact(gear_r, egui::Id::new("popup-gear"), egui::Sense::click())
                .on_hover_text(if self.show_adv { "Hide more options" } else { "More options: connections, cookie, referer" });
            icon_btn(&p, gear_r, g_b.hovered(), DmIcon::Settings, 14.0, 1.5, if self.show_adv { DM_ACC } else { pl().dm_dim });
            if g_b.clicked() {
                self.show_adv = !self.show_adv;
            }

            // ---- gear panel: connections / referer / cookie / description ----
            if self.show_adv {
                let panel = at(DM_X, DM_ACT, DM_W_IN, panel_h);
                dm_box(&p, panel);
                let cx = panel.min.x + 13.0;
                let cw = panel.width() - 26.0;
                let half = (cw - 8.0) / 2.0;
                let mut cy = panel.min.y + 13.0;
                let desc_r = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(cw, DM_BOX_H));
                dm_field(ui, "popup_desc", desc_r, desc_r.max.x - 13.0, &mut self.description, "Description (optional)", false);
                cy += DM_BOX_H + 8.0;
                let ref_r = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(half, DM_BOX_H));
                let cok_r = egui::Rect::from_min_size(egui::pos2(cx + half + 8.0, cy), egui::vec2(half, DM_BOX_H));
                dm_field(ui, "popup_ref", ref_r, ref_r.max.x - 13.0, &mut self.referer, "Referer (optional)", false);
                dm_field(ui, "popup_cookie", cok_r, cok_r.max.x - 13.0, &mut self.cookie, "Cookie (optional)", false);
                cy += DM_BOX_H + 8.0;
                let row = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(cw, 30.0));
                p.text(egui::pos2(row.min.x, row.center().y), egui::Align2::LEFT_CENTER, "Connections", reg(11.0), pl().dm_dim);
                let srect = egui::Rect::from_min_size(egui::pos2(row.min.x + 92.0, row.min.y + 5.0), egui::vec2(row.width() - 144.0, 20.0));
                let frac = ((self.connections as f32 - 1.0) / 31.0).clamp(0.0, 1.0);
                if let Some(nf) = slider_row(ui, srect, "conns", frac) {
                    self.connections = ((nf * 31.0).round() as usize + 1).clamp(1, 32);
                }
                p.text(egui::pos2(row.max.x, row.center().y), egui::Align2::RIGHT_CENTER, format!("{}", self.connections), reg(11.0), pl().dm_txt);
                cy += 30.0 + 8.0;
                let rem = egui::Rect::from_min_size(egui::pos2(cx, cy), egui::vec2(cw, 22.0));
                if check_row(ui, rem, "remember", self.remember_path, "Remember this path (category subfolder)").clicked() {
                    self.remember_path = !self.remember_path;
                }
                cy += 22.0;
                for n in &notes {
                    p.text(egui::pos2(cx, cy + 6.0), egui::Align2::LEFT_CENTER, n, reg(11.0), pl().dm_dim);
                    cy += NOTE_H;
                }
            }

            // ---- media options: Resolution selector and Playlist toggle ----
            let mut pl_offset = 0.0;
            if is_media {
                pl_offset = 36.0;
                let is_pl_query = engine::is_playlist_url_or_query(&self.url);
                let row_y = DM_ACT + adv_h - 2.0;

                let (q_x, q_w) = if is_pl_query {
                    let pl_rect = at(DM_X, row_y, 205.0, 28.0);
                    if check_row(ui, pl_rect, "popup_pl", self.is_playlist, "Download full playlist").clicked() {
                        self.is_playlist = !self.is_playlist;
                    }

                    // Button to load all playlist items into the multi-file selector
                    let view_r = at(DM_X + 210.0, row_y, 145.0, 28.0);
                    let view_b = ui.interact(view_r, egui::Id::new("popup_pl_view"), egui::Sense::click());
                    if view_b.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let lbl = if self.loading_playlist { "Loading playlist…" } else { "📋 Select Videos" };
                    btn_pill(&p, view_r, view_b.hovered(), lbl, pl().dm_line, pl().dm_box_bd, pl().dm_txt, None);
                    if view_b.clicked() && !self.loading_playlist {
                        if let Some(rt) = self.rt.clone() {
                            let (tx, rx) = std::sync::mpsc::channel();
                            self.playlist_rx = Some(rx);
                            self.loading_playlist = true;
                            let url = self.url.clone();
                            let opts = EngineOpts {
                                user_agent: self.user_agent.clone(),
                                timeout_secs: self.timeout_secs,
                                proxy_url: self.proxy_url.clone(),
                                referer: self.referer.clone(),
                                cookie: self.cookie.clone(),
                                quality_pref: self.quality_pref.clone(),
                                ..Default::default()
                            };
                            rt.spawn(async move {
                                let res = engine::playlist_entries(&url, &opts).await;
                                let _ = tx.send(res);
                            });
                        }
                    }
                    (DM_X + 340.0, DM_W_IN - 340.0)
                } else {
                    (DM_X, DM_W_IN)
                };

                // Resolution / Quality dropdown
                let q_rect = at(q_x, row_y, q_w, 28.0);
                let cur_q_disp = if !self.real_formats.is_empty() {
                    if let Some(cur) = self.real_formats.iter().find(|f| {
                        f.quality_val.eq_ignore_ascii_case(&self.quality_pref)
                            || (self.quality_pref == "best" && f.quality_val == "best")
                    }) {
                        cur.label.clone()
                    } else {
                        self.real_formats[0].label.clone()
                    }
                } else {
                    match self.quality_pref.trim().to_ascii_lowercase().as_str() {
                        "" | "best" | "max" | "highest" => "Best Quality".to_string(),
                        "audio" => "Audio only (MP3)".to_string(),
                        "2160p" | "4k" => "4K (2160p)".to_string(),
                        "1440p" | "2k" => "2K (1440p)".to_string(),
                        "1080p" => "1080p Full HD".to_string(),
                        "720p" => "720p HD".to_string(),
                        "480p" => "480p SD".to_string(),
                        "360p" => "360p SD".to_string(),
                        other => other.to_string(),
                    }
                };

                let q_opts: Vec<String> = if !self.real_formats.is_empty() {
                    self.real_formats.iter().map(|f| f.label.clone()).collect()
                } else {
                    vec![
                        "Best Quality".to_string(),
                        "4K (2160p)".to_string(),
                        "2K (1440p)".to_string(),
                        "1080p Full HD".to_string(),
                        "720p HD".to_string(),
                        "480p SD".to_string(),
                        "360p SD".to_string(),
                        "Audio only (MP3)".to_string(),
                    ]
                };

                if let Some(pick) = combo(ui, q_rect, "quality_combo", &cur_q_disp, &q_opts, &mut self.quality_open) {
                    if let Some(rf) = self.real_formats.iter().find(|f| f.label == pick) {
                        self.quality_pref = rf.quality_val.clone();
                        if let Some(sz) = rf.size {
                            self.probe_size = Some(sz);
                        }
                        if rf.is_audio {
                            self.category = "Music".to_string();
                            if !self.filename.ends_with(".mp3") {
                                let stem = std::path::Path::new(&self.filename)
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(&self.filename);
                                self.filename = format!("{stem}.mp3");
                            }
                        }
                    } else {
                        match pick.as_str() {
                            "Best Quality" => self.quality_pref = "best".to_string(),
                            "4K (2160p)" => self.quality_pref = "2160p".to_string(),
                            "2K (1440p)" => self.quality_pref = "1440p".to_string(),
                            "1080p Full HD" => self.quality_pref = "1080p".to_string(),
                            "720p HD" => self.quality_pref = "720p".to_string(),
                            "480p SD" => self.quality_pref = "480p".to_string(),
                            "360p SD" => self.quality_pref = "360p".to_string(),
                            "Audio only (MP3)" => {
                                self.quality_pref = "audio".to_string();
                                self.category = "Music".to_string();
                            }
                            other => self.quality_pref = other.to_string(),
                        }
                    }
                }
            }

            // ---- actions: [Download Later] [Cancel X] [Download v] ----
            let act_y = DM_ACT + adv_h + pl_offset;
            let later_r = at(DM_X, act_y, 130.0, 36.0);
            let dl_r = egui::Rect::from_min_size(
                egui::pos2(o.x + DM_W - 17.0 - 130.0, o.y + act_y),
                egui::vec2(130.0, 36.0),
            );
            let cancel_r = egui::Rect::from_min_size(
                egui::pos2(dl_r.min.x - 10.0 - 100.0, o.y + act_y),
                egui::vec2(100.0, 36.0),
            );

            let later_b = ui.interact(later_r, egui::Id::new("popup-later"), egui::Sense::click());
            let cancel_b = ui.interact(cancel_r, egui::Id::new("popup-cancel"), egui::Sense::click());
            let dl_b = ui.interact(dl_r, egui::Id::new("popup-dl"), egui::Sense::click());

            btn_pill(&p, later_r, later_b.hovered(), crate::i18n::t("Download Later"), pl().dm_line, pl().dm_box_bd, pl().dm_txt, None);
            btn_pill(&p, cancel_r, cancel_b.hovered(), crate::i18n::t("Cancel"), pl().dm_line, pl().dm_box_bd, egui::Color32::from_rgb(0xFF, 0x78, 0x75), Some(DmIcon::Close));
            btn_spring_breeze(&p, dl_r, dl_b.hovered(), crate::i18n::t("Download"), Some(DmIcon::ArrowDown));

            if later_b.hovered() || dl_b.hovered() || cancel_b.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if later_b.clicked() {
                self.commit(false);
            }
            if dl_b.clicked() {
                self.commit(true);
            }
            if cancel_b.clicked() {
                self.done = true;
            }
            // ---- notice (the window grew 16px for it) ----
            if !self.notice.is_empty() {
                let shown: String = self.notice.chars().take(78).collect();
                p.text(
                    egui::pos2(o.x + DM_X, act_y + 36.0 + 10.0),
                    egui::Align2::LEFT_CENTER,
                    shown,
                    reg(11.0),
                    pl().dm_warn,
                );
            }
        });
    }
}

#[cfg(test)]
mod audit_inbox_tests {
    #[test]
    fn claiming_a_capture_returns_private_cookies_and_preserves_other_entries() {
        let dir=std::env::temp_dir().join(format!("dc-inbox-audit-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();let path=dir.join("inbox.json");
        let data=serde_json::json!([
            {"url":"https://example.com/a","received_at":"2026-10-20","cookie":"session=secret"},
            {"url":"https://example.com/b","received_at":"2026-10-20"}
        ]);
        super::atomic_write(&path,serde_json::to_string(&data).unwrap().as_bytes()).unwrap();
        let capture=super::claim_inbox_at(&path,"https://example.com/a").unwrap();
        assert_eq!(capture.cookie.as_deref(),Some("session=secret"));
        let left:Vec<crate::native_host::InboxItem>=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(left.len(),1);assert_eq!(left[0].url,"https://example.com/b");
        std::fs::write(&path,b"{broken").unwrap();
        assert!(super::claim_inbox_at(&path,"https://example.com/b").is_none());
        assert_eq!(std::fs::read(&path).unwrap(),b"{broken");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
