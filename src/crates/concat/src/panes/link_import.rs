//! Link import sheet and cancellable background jobs.
use crate::{
    host::{on_ui_in_project, project_epoch, spawn_in_project},
    i18n::{t, tf},
    panes::{Msg, media_bin::MediaMsg},
    studio::Studio,
    ui::{LinkImportData, LinkImportItem},
};
use concat_host::link_import::{self, Item, Tools};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub enum LinkMsg {
    Action(i32, i32, String),
    Progress(u64, f32, String),
    Finished(u64, Result<Outcome, String>),
}
#[derive(Debug)]
pub enum Outcome {
    Installed,
    Connected,
    Inspected(Vec<Item>),
    Downloaded(Vec<PathBuf>, Vec<String>, Vec<usize>),
}
#[derive(Default)]
pub struct LinkImport {
    pub open: bool,
    links: String,
    cookies: Option<PathBuf>,
    items: Vec<Item>,
    thumbnails: Vec<slint::Image>,
    selected: Vec<bool>,
    quality: usize,
    audio: bool,
    drive_auth: bool,
    busy: bool,
    progress: f32,
    speed: String,
    message: String,
    generation: u64,
    cancel: Arc<AtomicBool>,
}
impl Drop for LinkImport {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl LinkImport {
    pub fn data(&self, studio: &Studio) -> LinkImportData {
        let tools = Tools::new(&studio.host.dirs);
        let items = self
            .items
            .iter()
            .zip(&self.selected)
            .enumerate()
            .map(|(index, (i, selected))| LinkImportItem {
                title: i.title.as_str().into(),
                thumbnail: self.thumbnails.get(index).cloned().unwrap_or_default(),
                detail: [
                    if i.duration > 0.0 {
                        format!("{:.0}s", i.duration)
                    } else {
                        String::new()
                    },
                    if i.size > 0 {
                        format!("{:.1} MiB", i.size as f64 / 1048576.0)
                    } else {
                        i.heights
                            .iter()
                            .map(|h| format!("{h}p"))
                            .collect::<Vec<_>>()
                            .join(" / ")
                    },
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
                .into(),
                selected: *selected,
            })
            .collect::<Vec<_>>();
        LinkImportData {
            mobile: cfg!(any(target_os = "android", target_os = "ios")),
            open: self.open,
            links: self.links.as_str().into(),
            busy: self.busy,
            progress: self.progress,
            speed: self.speed.as_str().into(),
            message: self.message.as_str().into(),
            ready: tools.ready(),
            connected: tools.connected(),
            audio: self.audio,
            drive_auth: self.drive_auth,
            quality: self.quality as i32,
            cookie_name: self
                .cookies
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            items: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(items))),
            can_download: !self.busy && self.selected.iter().any(|s| *s),
        }
    }
    pub fn update(&mut self, msg: LinkMsg, studio: &mut Studio) {
        match msg {
            LinkMsg::Progress(g, p, s) if g == self.generation => {
                self.progress = p;
                self.speed = s;
            }
            LinkMsg::Finished(g, result) if g == self.generation => {
                self.busy = false;
                self.speed.clear();
                match result {
                    Ok(Outcome::Installed) => self.message = t("links.toolsReady"),
                    Ok(Outcome::Connected) => {
                        self.items.clear();
                        self.selected.clear();
                        self.thumbnails.clear();
                        self.drive_auth = true;
                        self.message = t("links.connected");
                    }
                    Ok(Outcome::Inspected(items)) => {
                        self.thumbnails = items
                            .iter()
                            .map(|i| {
                                slint::Image::load_from_path(std::path::Path::new(&i.thumbnail))
                                    .unwrap_or_default()
                            })
                            .collect();
                        self.selected = vec![true; items.len()];
                        self.items = items;
                        self.message.clear();
                    }
                    Ok(Outcome::Downloaded(files, errors, failed)) => {
                        self.selected.fill(false);
                        for index in failed {
                            if let Some(s) = self.selected.get_mut(index) {
                                *s = true;
                            }
                        }
                        self.message = if files.is_empty() && errors.is_empty() {
                            t("links.cancelled")
                        } else {
                            format!(
                                "{}\n{}",
                                tf("links.downloaded", &[&files.len()]),
                                errors.join("\n")
                            )
                        };
                        if !files.is_empty() {
                            if studio.session.is_some() {
                                studio.handle(Msg::Media(MediaMsg::Import(files)));
                            } else {
                                let dl_str = crate::studio::home_folder("Downloads");
                                let target_dir = if !dl_str.is_empty()
                                    && std::path::Path::new(&dl_str).is_dir()
                                {
                                    PathBuf::from(dl_str)
                                } else {
                                    PathBuf::from(crate::studio::home_folder("Documents"))
                                };
                                let _ = std::fs::create_dir_all(&target_dir);
                                let mut saved_paths = Vec::new();
                                for file in &files {
                                    let file_name = file
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "downloaded.mp4".to_owned());
                                    let mut dest = target_dir.join(&file_name);
                                    if dest.exists() {
                                        let (stem, ext) = match file_name.rfind('.') {
                                            Some(dot) if dot > 0 => {
                                                (&file_name[..dot], &file_name[dot..])
                                            }
                                            _ => (file_name.as_str(), ""),
                                        };
                                        for n in 2..1000 {
                                            let candidate =
                                                target_dir.join(format!("{stem} ({n}){ext}"));
                                            if !candidate.exists() {
                                                dest = candidate;
                                                break;
                                            }
                                        }
                                    }
                                    if std::fs::copy(file, &dest).is_ok() {
                                        let _ = std::fs::remove_file(file);
                                        saved_paths.push(dest);
                                    } else {
                                        saved_paths.push(file.clone());
                                    }
                                }
                                if let Some(first) = saved_paths.first() {
                                    let _ = crate::platform::reveal(&first.to_string_lossy());
                                    let msg = format!(
                                        "Đã tải {} file vào {}",
                                        saved_paths.len(),
                                        target_dir.display()
                                    );
                                    self.message = msg.clone();
                                    studio.notify(&msg, false);
                                }
                            }
                        }
                    }
                    Err(e) => self.message = e,
                }
            }
            LinkMsg::Action(action, index, text) => {
                if action == 0 {
                    self.open = true;
                    return;
                }
                if action == 1 {
                    self.open = false;
                    return;
                }
                if action == 8 {
                    self.cancel.store(true, Ordering::Relaxed);
                    return;
                }
                if self.busy {
                    return;
                }
                match action {
                    2 => {
                        self.links = text;
                        self.items.clear();
                        self.thumbnails.clear();
                        self.selected.clear();
                    }
                    3 => self.quality = index.clamp(0, 3) as usize,
                    4 => self.audio = index != 0,
                    5 => {
                        self.drive_auth = index != 0;
                        self.items.clear();
                        self.selected.clear();
                        self.thumbnails.clear();
                    }
                    6 => {
                        if let Some(s) = self.selected.get_mut(index.max(0) as usize) {
                            *s = !*s;
                        }
                    }
                    7 => {
                        let pick = self.selected.iter().any(|s| !*s);
                        self.selected.fill(pick);
                    }
                    9 | 10 | 11 | 12 => self.start(action, studio),
                    13 => {
                        crate::host::pick_files_then(
                            t("links.chooseCookies"),
                            Some((t("links.cookieFile"), &["txt"])),
                            |studio, _, _, paths| {
                                if let Some(path) = paths.first() {
                                    studio.handle(Msg::Links(LinkMsg::Action(
                                        14,
                                        0,
                                        path.to_string_lossy().into_owned(),
                                    )));
                                }
                            },
                        );
                    }
                    14 => self.cookies = (!text.is_empty()).then(|| PathBuf::from(text)),
                    15 => {
                        let tools = Tools::new(&studio.host.dirs);
                        if let Err(e) = std::fs::remove_file(&tools.config) {
                            if e.kind() != std::io::ErrorKind::NotFound {
                                self.message = e.to_string();
                                return;
                            }
                        }
                        self.drive_auth = false;
                        self.items.clear();
                        self.selected.clear();
                        self.thumbnails.clear();
                        self.message = t("links.disconnected");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fn start(&mut self, action: i32, studio: &Studio) {
        let project = studio.standalone_project_dir();
        let tools = Tools::new(&studio.host.dirs);
        let mobile = cfg!(any(target_os = "android", target_os = "ios"));
        if mobile && matches!(action, 9 | 10) {
            self.message = t("links.mobileHint");
            return;
        }
        if mobile
            && action == 11
            && self
                .links
                .lines()
                .filter(|s| !s.trim().is_empty())
                .any(|s| {
                    !matches!(
                        link_import::source(s),
                        Ok(link_import::Source::Drive { folder: false, .. })
                    )
                })
        {
            self.message = t("links.mobileHint");
            return;
        }
        if action != 9 && !mobile && !tools.ready() {
            self.message = t("links.installFirst");
            return;
        }
        let selected = self
            .items
            .iter()
            .zip(&self.selected)
            .filter(|(_, s)| **s)
            .map(|(i, _)| i.clone())
            .collect::<Vec<_>>();
        if action == 12 && selected.is_empty() {
            return;
        }
        let selected_rows = self
            .selected
            .iter()
            .enumerate()
            .filter(|(_, s)| **s)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        self.busy = true;
        self.progress = 0.0;
        self.message = match action {
            9 => t("links.installing"),
            10 => t("links.signingIn"),
            11 => t("links.inspecting"),
            _ => t("links.downloading"),
        };
        self.generation += 1;
        let generation = self.generation;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let links = self.links.clone();
        let auth = self.drive_auth;
        let cookies = self.cookies.clone();
        let height = [0, 1080, 720, 480][self.quality];
        let audio = self.audio;
        let epoch = project_epoch();
        spawn_in_project(
            move || {
                let mut progress = move |p, s| {
                    on_ui_in_project(epoch, move |studio, _, _| {
                        studio.handle(Msg::Links(LinkMsg::Progress(generation, p, s)))
                    });
                };
                match action {
                    9 => link_import::install(&tools, &cancel, &mut progress)
                        .map(|_| Outcome::Installed),
                    10 => link_import::connect(&tools, &cancel).map(|_| Outcome::Connected),
                    11 => link_import::inspect(&tools, &links, auth, cookies.as_deref(), &cancel)
                        .map(Outcome::Inspected),
                    _ => link_import::download(
                        &tools,
                        &selected,
                        &project,
                        height,
                        audio,
                        cookies.as_deref(),
                        &cancel,
                        &mut progress,
                    )
                    .map(|(f, e, failed)| {
                        Outcome::Downloaded(
                            f,
                            e,
                            failed
                                .into_iter()
                                .filter_map(|i| selected_rows.get(i).copied())
                                .collect(),
                        )
                    }),
                }
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Links(LinkMsg::Finished(generation, result)))
            },
        );
    }
}
