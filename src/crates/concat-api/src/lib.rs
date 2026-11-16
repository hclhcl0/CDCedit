// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

//! The CDCAi API: the one dispatcher every way of driving the editor
//! without its window goes through.
//!
//! The command line, a daemon on a socket, an MCP server, a plugin: each is
//! a transport, and a transport is a loop that reads a [`Request`], hands
//! it to [`Api::dispatch`], and writes the [`Response`] back, forwarding
//! the [`Event`]s the API's jobs raise as they come. Nothing about what a
//! request *means* lives in a transport, so two of them cannot disagree,
//! and a method added here reaches all of them. What a line transport puts
//! around the three is the JSON-RPC 2.0 envelope in [`rpc`].
//!
//! The crate decides nothing about the edit either. Edits are
//! `concat_project` [`Command`]s carried as they are; projects, media,
//! templates, titles, cutouts and exports are `concat_host`'s. What this
//! crate owns is the choreography the window performs by hand - probe then
//! add, find masks and paint titles before rendering, save through the
//! session - stated once so a file exported here is the file the window
//! would have written.
//!
//! One [`Api`] holds one session per open project folder. It is not
//! thread-safe by design: a transport that serves several callers owns the
//! one `Api` and serialises through it, the way the window's event loop
//! does. A method that would keep everyone waiting - an export - runs as a
//! job on its own thread instead: [`Api::dispatch`] returns its name at
//! once and the job reports through the [`EventSink`] the API was made
//! with, from that thread, which is why the sink must be `Send + Sync`.

mod flow;
mod job;
pub mod message;
pub mod rpc;
#[cfg(feature = "speech")]
mod speech;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use base64::Engine as _;
use concat_effects::Catalogue;
use concat_effects::manifest::Kind;
use concat_export::{ExportClip, ExportRequest};
pub use concat_host::AppDirs;
use concat_host::cutout::{self, AnalyseRequest, Cutouts};
use concat_host::export::{self, Exporter};
use concat_host::flow::FlowAccountLock;
use concat_host::preview::{FrameSpec, Monitor};
use concat_host::session::EditorView;
use concat_host::stabilize::{StabilizeRequest, Stabilizers};
use concat_host::templates::{self, SlotFill};
use concat_host::{ProjectInfo, Session, Titles, capcut, link_import, lock, media, projects};
use concat_project::Command;
use concat_project::model::VideoSettings;
use serde_json::{Value, json};

pub use message::{
    API_VERSION, ApiError, CapCutExportInfo, CapCutTextSpec, CaptionLineSpec, DialogueTurnSpec,
    Dirs, Done, DryRunInfo, DubCueSpec, ErrorCode, Event, ExportSpec, Fill, FlowAccountInfo,
    FlowAccountRecentInfo, FlowAccountsInfo, FlowConnectionInfo, FlowReceiptInfo, FlowRecentInfo,
    FlowStatusInfo, JobState, JobStatus, LinkItem, LinkProbeInfo, PackageInfo, ParamInfo, Picture,
    Reply, Request, Response, SpeechStatusInfo, SpeechTranscriberInfo, SpeechTtsModelInfo,
    SpeechVoiceInfo, Started, VersionInfo, Written,
};

/// The export sheet's middle quality, and what an export gets when the
/// caller says nothing.
const DEFAULT_CRF: u8 = 20;
/// The x264 preset every export uses unless told otherwise.
const DEFAULT_PRESET: &str = "medium";

/// What every build of the API serves, before what is decided by the
/// build or the embedder is added; see [`VersionInfo::capabilities`].
const CAPABILITIES: &[&str] = &["events"];

/// The JSON Schema of [`Request`]: one alternative per method, each the
/// object its params are with `method` naming it, and every type a request
/// carries - an edit [`Command`] and all it holds - under `$defs`. The
/// descriptions are this crate's and `concat_project`'s doc comments, so
/// what a caller is told about a field is what its maintainer wrote next
/// to it. Generated, never written by hand: a method or a command field
/// added to the types is in the schema on the next build.
#[cfg(feature = "schema")]
pub fn request_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Request)).unwrap_or_default()
}

/// Where a job's events go. Called from the job's thread, so a transport
/// that writes them to a caller locks its writer inside.
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// The largest frame a preview or an export may ask for, a side: 8K, the
/// engine's own ceiling. A caller with the token is trusted to edit, not
/// to ask the machine for seventeen gigabytes of pixels (audit 2026-09-23,
/// #4).
pub const MAX_SIDE: u32 = concat_core::frame::MAX_SIDE;
/// The highest constant rate factor any codec here takes.
const MAX_CRF: u8 = 63;
/// The fastest frame rate an export may ask for.
const MAX_RATE: f64 = 240.0;

/// Who holds a project folder open on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// The window, on screen.
    Window,
    /// The API, for a caller on a socket.
    Api,
}

impl std::fmt::Display for Holder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Holder::Window => "the window",
            Holder::Api => "the API",
        })
    }
}

/// Why a project folder could not be claimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The other side of this process has it: the window, or the API it
    /// serves.
    Here(Holder),
    /// Another process has it - a second window, `concat --mcp`, the CLI -
    /// as its lock file says.
    Elsewhere(lock::Owner),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Here(holder) => write!(f, "open in {holder}"),
            Refusal::Elsewhere(owner) => {
                let what = if owner.role == "window" {
                    "a CDCAi window"
                } else {
                    "another CDCAi process"
                };
                match owner.pid {
                    Some(pid) => write!(f, "open in {what} (process {pid})"),
                    None => write!(f, "open in {what}"),
                }
            }
        }
    }
}

/// One claimed folder: who has it in this process, and the lock that
/// keeps every other process off it.
struct Claim {
    holder: Holder,
    /// `None` where the folder would not take a lock file - one that
    /// cannot be written - which costs the guard, not the open.
    _lock: Option<lock::ProjectLock>,
}

/// The project folders open on this machine and who has each, shared
/// between a window and the API it embeds, so neither opens a folder the
/// other is editing and saves over its work (audit 2026-09-23, #5). Cheap
/// to clone; every clone is the same register.
///
/// Each claimed folder is also locked against every other process
/// ([`lock::ProjectLock`]): a second window, `concat --mcp` working on its
/// own, the CLI. The lock goes with the claim, and with the process if it
/// dies, so a crash leaves a folder the next opener takes over.
#[derive(Clone, Default)]
pub struct OpenProjects(Arc<Mutex<BTreeMap<String, Claim>>>);

impl OpenProjects {
    /// Claims `path` for `holder`. Refused, naming who has it, when the
    /// other side of this process does or another process does; claiming
    /// again what one already holds is fine.
    pub fn claim(&self, path: &str, holder: Holder) -> Result<(), Refusal> {
        let key = key_of(path);
        let mut open = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match open.get(&key) {
            Some(claim) if claim.holder != holder => Err(Refusal::Here(claim.holder)),
            Some(_) => Ok(()),
            None => {
                let role = match holder {
                    Holder::Window => "window",
                    Holder::Api => "api",
                };
                let lock = match lock::ProjectLock::acquire(Path::new(path), role) {
                    Ok(lock) => Some(lock),
                    Err(lock::LockError::Held(owner)) => return Err(Refusal::Elsewhere(owner)),
                    // A folder that will not take the file: a read-only
                    // one, which nobody can save over anyway.
                    Err(lock::LockError::Io(_)) => None,
                };
                open.insert(
                    key,
                    Claim {
                        holder,
                        _lock: lock,
                    },
                );
                Ok(())
            }
        }
    }

    /// Gives `path` back, whoever had it.
    pub fn release(&self, path: &str) {
        let key = key_of(path);
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&key);
    }

    /// Who has `path` open, if anyone.
    pub fn holder(&self, path: &str) -> Option<Holder> {
        let key = key_of(path);
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key)
            .map(|claim| claim.holder)
    }
}

/// Bridge from [`Api`] to the project open in the desktop window (Live Mode).
///
/// When the user turns on Live Mode in Settings › Automation, the window
/// installs an implementation of this trait via [`Api::share_live_window`].
/// Requests targeting the window's active project then run on the window's
/// UI session — pushing onto the interactive undo stack and refreshing the
/// timeline and monitor — instead of failing with "open in the window".
pub trait LiveWindowHandler: Send + Sync {
    /// Whether Live Mode is currently enabled.
    fn is_enabled(&self) -> bool;

    /// Folder path of the active window project while Live Mode is enabled.
    fn active_project_path(&self) -> Option<String>;

    /// Executes a project read or edit request on the active window project.
    fn dispatch_live(&self, request: Request) -> Result<Reply, ApiError>;

    /// Captures an immutable snapshot of the active window project for
    /// off-thread preview rendering and export.
    fn snapshot_project(&self, path: &str) -> Result<Session, ApiError>;
}

/// The dispatcher: the open sessions and the services behind them.
pub struct Api {
    dirs: AppDirs,
    /// Open projects by canonical folder path.
    sessions: BTreeMap<String, Session>,
    /// Where this API may write: a created project, an instantiated
    /// template, an export or a preview file lands under one of these.
    /// Empty means anywhere, for the process's own owner at a terminal.
    roots: Vec<PathBuf>,
    /// Which project folders are open on this machine, across this API
    /// and a window it is embedded in.
    open: OpenProjects,
    live_window: Option<Arc<dyn LiveWindowHandler>>,
    titles: Titles,
    cutouts: Arc<Cutouts>,
    stabilizers: Arc<Stabilizers>,
    flow_lock: FlowAccountLock,
    #[cfg(feature = "speech")]
    transcriber: Arc<concat_speech::Transcriber>,
    #[cfg(feature = "speech")]
    speech: Arc<concat_speech::Speech>,
    monitor: Monitor,
    exporter: Exporter,
    events: EventSink,
    jobs: Jobs,
    /// Every job's last known state and each running one's stop flag, fed
    /// by the events on their way to `events`.
    book: Arc<job::Book>,
    /// Edits queued by completed background jobs (e.g. `autoImport` / `placeOnTimeline`)
    /// waiting to be applied to headless `sessions` on the dispatcher thread.
    pending_edits: Arc<Mutex<Vec<PendingSessionEdit>>>,
    /// [`VersionInfo::capabilities`], in the order they were added.
    capabilities: Vec<String>,
}

/// One deferred project mutation emitted by a finished background job.
#[derive(Clone)]
pub(crate) enum PendingStep {
    /// Applies a single [`Command`] directly.
    Apply(Command),
    /// Imports `item` into the media bin (or reuses an existing entry with the
    /// same path) and places a clip onto the timeline.
    ImportAndPlace {
        item: concat_project::commands::NewMedia,
        track_id: Option<String>,
        start: f64,
    },
}

/// Deferred project mutation queued by a finished background job when the
/// target project is held in [`Api::sessions`] rather than the live window.
pub(crate) struct PendingSessionEdit {
    job: String,
    path: String,
    steps: Vec<PendingStep>,
    pre_media_ids: Vec<String>,
    pre_clip_ids: Vec<String>,
}

/// A way to stop the jobs of one [`Api`] from any thread, without waiting
/// for its thread to be free: what a server that is shutting down calls
/// before it waits for that thread. Cheap to clone.
#[derive(Clone)]
pub struct Stopper {
    book: Arc<job::Book>,
    cutouts: Arc<Cutouts>,
    #[cfg(feature = "speech")]
    transcriber: Arc<concat_speech::Transcriber>,
    #[cfg(feature = "speech")]
    speech: Arc<concat_speech::Speech>,
}

impl Stopper {
    /// Asks every job this API started to stop at its next frame - its
    /// own and no other, even where the export slot is shared with a
    /// window. Each then ends with [`ErrorCode::Cancelled`]. How many
    /// were running.
    pub fn cancel_all(&self) -> usize {
        let running = self.book.cancel_all();
        if running > 0 {
            // A job still finding masks is in the analysis, not the
            // render; the analysis is this API's own.
            self.cutouts.cancel();
            #[cfg(feature = "speech")]
            {
                self.transcriber.cancel();
                self.speech.cancel();
            }
        }
        running
    }
}

impl Api {
    /// An API over this machine's app directories, reporting its jobs
    /// through `events`.
    pub fn new(events: EventSink) -> Result<Api, String> {
        Ok(Api::with_dirs(AppDirs::locate()?, events))
    }

    /// An API over the given directories: a test's scratch, or an embedder
    /// with a home of its own.
    pub fn with_dirs(dirs: AppDirs, events: EventSink) -> Api {
        let book = Arc::new(job::Book::default());
        let events: EventSink = {
            let book = Arc::clone(&book);
            Arc::new(move |event: Event| {
                book.record(&event);
                events(event);
            })
        };
        Api {
            book,
            pending_edits: Arc::new(Mutex::new(Vec::new())),
            titles: Titles::new(&dirs),
            cutouts: Arc::new(Cutouts::new(&dirs.data)),
            stabilizers: Arc::new(Stabilizers::new()),
            flow_lock: FlowAccountLock::new(),
            #[cfg(feature = "speech")]
            transcriber: Arc::new(concat_speech::Transcriber::new()),
            #[cfg(feature = "speech")]
            speech: Arc::new(concat_speech::Speech::new()),
            monitor: Monitor::new(),
            exporter: Exporter::new(),
            sessions: BTreeMap::new(),
            roots: Vec::new(),
            open: OpenProjects::default(),
            live_window: None,
            events,
            jobs: Jobs::default(),
            dirs,
            capabilities: CAPABILITIES.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    /// The directories this API works under.
    pub fn dirs(&self) -> &AppDirs {
        &self.dirs
    }

    /// Adds a name to [`VersionInfo::capabilities`]: what the transport
    /// or embedder around this API serves that the API cannot know of
    /// itself, such as the sockets it is listening on. A name already
    /// there is not repeated.
    pub fn add_capability(&mut self, name: &str) {
        if !self.capabilities.iter().any(|known| known == name) {
            self.capabilities.push(name.to_owned());
        }
    }

    /// What stops this API's jobs from another thread; see [`Stopper`].
    pub fn stopper(&self) -> Stopper {
        Stopper {
            book: Arc::clone(&self.book),
            cutouts: Arc::clone(&self.cutouts),
            #[cfg(feature = "speech")]
            transcriber: Arc::clone(&self.transcriber),
            #[cfg(feature = "speech")]
            speech: Arc::clone(&self.speech),
        }
    }

    /// Applies any session edits queued by completed background jobs when the
    /// target project lives in `self.sessions` (or active live window) and
    /// merges created ids back into the finished job's `result`.
    fn drain_pending_edits(&mut self) {
        let pending = {
            let mut queue = self
                .pending_edits
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if queue.is_empty() {
                return;
            }
            std::mem::take(&mut *queue)
        };
        for item in pending {
            let mut media_ids = item.pre_media_ids;
            let mut clip_ids = item.pre_clip_ids;
            let mut apply_error: Option<String> = None;
            for step in item.steps {
                if let Err(err) = execute_step(
                    step,
                    &mut |cmd| self.apply(&item.path, cmd),
                    &mut media_ids,
                    &mut clip_ids,
                ) {
                    apply_error = Some(err.message);
                    break;
                }
            }
            if let Some(err) = apply_error {
                self.book
                    .merge_result(&item.job, json!({ "importError": err }));
            } else {
                let mut patch = serde_json::Map::new();
                if !media_ids.is_empty() {
                    patch.insert("mediaIds".to_owned(), json!(media_ids));
                }
                if !clip_ids.is_empty() {
                    patch.insert("clipIds".to_owned(), json!(clip_ids));
                }
                if !patch.is_empty() {
                    self.book.merge_result(&item.job, Value::Object(patch));
                }
            }
        }
    }

    /// Starts a non-export background job tracked in [`job::Book`] and
    /// [`Jobs`], returning [`Started`] immediately.
    pub(crate) fn spawn_job<F>(
        &mut self,
        kind: &str,
        path: &str,
        output: &str,
        work: F,
    ) -> Result<Started, ApiError>
    where
        F: FnOnce(String, Arc<AtomicBool>, EventSink) + Send + 'static,
    {
        self.jobs.reap(&self.book);
        let started = Started {
            job: self.jobs.mint(),
            path: path.to_owned(),
            output: output.to_owned(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.book.start(
            JobStatus::running_kind(&started.job, kind, &started.path, &started.output),
            Arc::clone(&cancel),
        );
        let book = Arc::clone(&self.book);
        let job_name = started.job.clone();
        let events = Arc::clone(&self.events);
        let thread = std::thread::Builder::new()
            .name(format!("{kind} {}", started.job))
            .spawn(move || work(job_name, cancel, events))
            .map_err(|error| {
                book.abandon(&started.job);
                ApiError::failed(format!("could not start job {kind}: {error}"))
            })?;
        self.jobs.threads.push((started.job.clone(), thread));
        Ok(started)
    }

    /// [`Request::JobStatus`] / [`Request::ExportStatus`].
    pub fn job_status(&mut self, job: &str) -> Result<JobStatus, ApiError> {
        self.drain_pending_edits();
        self.jobs.reap(&self.book);
        self.book.status(job).ok_or_else(|| {
            ApiError::new(
                ErrorCode::NotFound,
                format!("no job {job}: it never ran, or ended long ago"),
            )
        })
    }

    /// The export slot, for an embedder that shares it with a window.
    pub fn exporter(&self) -> Exporter {
        self.exporter.clone()
    }

    /// Takes an embedder's export slot in place of its own, so "one export
    /// at a time" holds across a window and this API together.
    pub fn share_exporter(&mut self, exporter: Exporter) {
        self.exporter = exporter;
    }

    /// Takes an embedder's register of open projects in place of its own,
    /// so a folder the window has open is refused here, and the other way
    /// round.
    pub fn share_open_projects(&mut self, open: OpenProjects) {
        self.open = open;
    }

    /// Shares the window's live-project bridge so requests targeting the
    /// project currently open in the window run against its live UI session
    /// whenever Live Mode is enabled.
    pub fn share_live_window(&mut self, handler: Arc<dyn LiveWindowHandler>) {
        self.live_window = Some(handler);
    }

    /// Shares the window's Google Flow account lock & worker registry so
    /// concurrent Flow jobs from the window and MCP share capacity limits and
    /// profile locks.
    pub fn share_flow_lock(&mut self, lock: FlowAccountLock) {
        self.flow_lock = lock;
    }

    /// Shares the window's loaded Whisper transcriber so model weights stay
    /// cached across interactive and MCP captioning runs.
    #[cfg(feature = "speech")]
    pub fn share_transcriber(&mut self, transcriber: Arc<concat_speech::Transcriber>) {
        self.transcriber = transcriber;
    }

    /// Shares the window's loaded local TTS engine so model state is reused
    /// across UI and MCP speech synthesis.
    #[cfg(feature = "speech")]
    pub fn share_speech(&mut self, speech: Arc<concat_speech::Speech>) {
        self.speech = speech;
    }

    /// The live-window bridge when Live Mode is active and `path` matches the
    /// project currently open on screen.
    pub(crate) fn live_handler_for(&self, path: &str) -> Option<Arc<dyn LiveWindowHandler>> {
        let handler = self.live_window.as_ref()?;
        if !handler.is_enabled() {
            return None;
        }
        let active = handler.active_project_path()?;
        (key_of(&active) == key_of(path)).then(|| Arc::clone(handler))
    }

    /// Confines every write to `roots`: from here on a created project, an
    /// instantiated template, an export and a preview file must land under
    /// one of them, and any other path is `refused`. Reads - a probe, a
    /// project opened by path - are not confined. An empty list confines
    /// nothing, which is right for the process's own owner at a terminal
    /// and wrong for a socket.
    pub fn restrict_writes_to(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots
            .into_iter()
            .map(|root| root.canonicalize().unwrap_or(root))
            .collect();
    }

    /// The roots writes are confined to; empty when they are not.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// `path` as somewhere this API may write, or why not.
    pub(crate) fn writable(&self, path: &str) -> Result<(), ApiError> {
        if self.roots.is_empty() {
            return Ok(());
        }
        let outside = || {
            let roots = self
                .roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            ApiError::new(
                ErrorCode::Refused,
                format!("{path} is outside where this API writes ({roots})"),
            )
        };
        let given = Path::new(path);
        // A `..` past the part of the path that exists cannot be resolved
        // and would walk out of a root on paper while staying under it in
        // this check; there is no reason a caller needs one.
        if given
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(outside());
        }
        let target = resolved(given);
        if self.roots.iter().any(|root| target.starts_with(root)) {
            Ok(())
        } else {
            Err(outside())
        }
    }

    /// How many jobs are still running.
    pub fn running(&mut self) -> usize {
        self.jobs.reap(&self.book);
        self.jobs.threads.len()
    }

    /// Waits for every running job to end. A transport calls this before
    /// it exits, so an export begun on its last line still finishes.
    pub fn finish(&mut self) {
        self.jobs.finish();
    }

    /// Runs one request. The response is what the request is worth; a job
    /// it begins reports through the sink from then on.
    pub fn dispatch(&mut self, request: Request) -> Response {
        self.drain_pending_edits();
        self.jobs.reap(&self.book);
        self.run(request).into()
    }

    fn run(&mut self, request: Request) -> Result<Reply, ApiError> {
        let view = |view: EditorView| Ok(Reply::View(Box::new(view)));
        match &request {
            Request::ProjectOpen { path }
            | Request::ProjectGet { path }
            | Request::ProjectDocument { path }
            | Request::ProjectSave { path, .. }
            | Request::ProjectSetVideo { path, .. }
            | Request::EditApply { path, .. }
            | Request::EditUndo { path }
            | Request::EditRedo { path } => {
                if let Some(handler) = self.live_handler_for(path) {
                    return handler.dispatch_live(request);
                }
            }
            Request::ProjectClose { path, .. } => {
                if self.live_handler_for(path).is_some() {
                    return Err(ApiError::new(
                        ErrorCode::Refused,
                        format!(
                            "{path} is open in the window under Live Mode; close it from the window instead"
                        ),
                    ));
                }
            }
            _ => {}
        }
        match request {
            Request::Version => Ok(Reply::Version(self.version())),
            Request::ProjectCreate {
                location,
                name,
                video,
            } => view(self.create(&location, &name, video.unwrap_or_default())?),
            Request::ProjectOpen { path } => view(self.open(&path)?),
            Request::ProjectClose { path, save } => {
                self.close(&path, save)?;
                Ok(Reply::Done(Done {}))
            }
            Request::ProjectList => Ok(Reply::Projects(self.recents())),
            Request::ProjectGet { path } => view(self.session(&path)?.view()),
            Request::ProjectDocument { path } => {
                Ok(Reply::Document(self.session(&path)?.document()))
            }
            Request::ProjectSave { path, name } => {
                self.save(&path, name.as_deref())?;
                Ok(Reply::Done(Done {}))
            }
            Request::ProjectSetVideo { path, video } => {
                view(self.session_mut(&path)?.set_video(video).map_err(refused)?)
            }
            Request::EditApply { path, command } => view(self.apply(&path, *command)?),
            Request::EditUndo { path } => view(self.session_mut(&path)?.undo()),
            Request::EditRedo { path } => view(self.session_mut(&path)?.redo()),
            Request::MediaProbe { path } => {
                Ok(Reply::Media(media::probe(&path).map_err(ApiError::failed)?))
            }
            Request::MediaImport { path, file } => view(self.import(&path, &file)?),
            Request::CatalogueList { kind } => Ok(Reply::Packages(catalogue(kind.as_deref())?)),
            Request::TemplateList => Ok(Reply::Templates(templates::list(&self.dirs.config))),
            Request::TemplateInstantiate {
                template,
                location,
                name,
                fills,
            } => view(self.instantiate(&template, &location, &name, fills)?),
            Request::TemplateSave { path, name } => {
                Ok(Reply::Template(self.save_template(&path, &name)?))
            }
            Request::ExportRun { path, spec } => Ok(Reply::Started(self.export(&path, &spec)?)),
            Request::ExportCancel { job } => {
                self.cancel(&job)?;
                Ok(Reply::Done(Done {}))
            }
            Request::ExportStatus { job } => Ok(Reply::Job(self.job_status(&job)?)),
            Request::PreviewFrame {
                path,
                time,
                output,
                width,
                height,
            } => {
                let size = width.zip(height);
                match output {
                    Some(output) => Ok(Reply::Written(self.frame(&path, time, &output, size)?)),
                    None => Ok(Reply::Picture(self.picture(&path, time, size)?)),
                }
            }
            Request::JobStatus { job } => Ok(Reply::Job(self.job_status(&job)?)),
            Request::JobCancel { job } => {
                self.cancel(&job)?;
                Ok(Reply::Done(Done {}))
            }
            Request::LinkProbe {
                links,
                use_drive,
                cookies_path,
            } => Ok(Reply::LinkItems(self.link_probe(
                links,
                use_drive,
                cookies_path.as_deref(),
            )?)),
            Request::LinkDownload {
                path,
                items,
                height,
                audio_only,
                auto_import,
                cookies_path,
            } => Ok(Reply::Started(self.link_download(
                &path,
                items,
                height.unwrap_or(1080),
                audio_only,
                auto_import.unwrap_or(true),
                cookies_path.as_deref(),
            )?)),
            Request::CapCutImport {
                input_path,
                location,
                name,
            } => view(self.capcut_import(&input_path, &location, name.as_deref())?),
            Request::CapCutExport {
                path,
                output_dir,
                text_options,
            } => Ok(Reply::CapCutExport(self.capcut_export(
                &path,
                output_dir.as_deref(),
                text_options.as_ref(),
            )?)),
            Request::ModelsSpeechStatus
            | Request::CaptionsTranscribe { .. }
            | Request::CaptionsTranslate { .. }
            | Request::SpeechTts { .. }
            | Request::SpeechDub { .. }
            | Request::SpeechVoiceChange { .. } => {
                #[cfg(feature = "speech")]
                {
                    self.run_speech_request(request)
                }
                #[cfg(not(feature = "speech"))]
                {
                    Err(ApiError::new(
                        ErrorCode::Refused,
                        "AI speech/captions support was disabled at build time (`speech` feature off)",
                    ))
                }
            }
            Request::FlowAccountsList
            | Request::FlowConnect { .. }
            | Request::FlowStatus { .. }
            | Request::FlowGenerateImage { .. }
            | Request::FlowGenerateVideo { .. }
            | Request::FlowRecentOutputs { .. } => self.run_flow_request(request),
        }
    }

    /// [`Request::Version`].
    pub fn version(&self) -> VersionInfo {
        VersionInfo {
            api_version: API_VERSION.to_owned(),
            concat: env!("CARGO_PKG_VERSION").to_owned(),
            dirs: Dirs::from(&self.dirs),
            capabilities: {
                let mut capabilities = self.capabilities.clone();
                if self.monitor.has_gpu() {
                    capabilities.push("gpu".to_owned());
                }
                if self
                    .live_window
                    .as_ref()
                    .is_some_and(|handler| handler.is_enabled())
                {
                    capabilities.push("liveWindow".to_owned());
                }
                if cfg!(feature = "speech") {
                    capabilities.push("speech".to_owned());
                }
                if !cfg!(any(target_os = "android", target_os = "ios")) {
                    capabilities.push("flow".to_owned());
                }
                capabilities
            },
        }
    }

    /// [`Request::ProjectCreate`]: the folder, its manifest, and a session
    /// on it, remembered in recents like a project the window made.
    pub fn create(
        &mut self,
        location: &str,
        name: &str,
        video: VideoSettings,
    ) -> Result<EditorView, ApiError> {
        self.writable(location)?;
        let info = projects::create(
            location,
            name,
            video.width,
            video.height,
            video.rate_num,
            video.rate_den,
            video.canvas,
        )
        .map_err(ApiError::failed)?;
        self.adopt(info)
    }

    /// [`Request::ProjectOpen`].
    pub fn open(&mut self, path: &str) -> Result<EditorView, ApiError> {
        if let Some(handler) = self.live_handler_for(path) {
            return match handler.dispatch_live(Request::ProjectOpen {
                path: path.to_owned(),
            })? {
                Reply::View(view) => Ok(*view),
                _ => Err(ApiError::failed("unexpected live reply")),
            };
        }
        let key = key_of(path);
        if let Some(session) = self.sessions.get(&key) {
            return Ok(session.view());
        }
        let info = projects::open(path).map_err(ApiError::failed)?;
        self.adopt(info)
    }

    /// Opens a session on a project the host just described and puts it at
    /// the front of the recents list.
    fn adopt(&mut self, info: ProjectInfo) -> Result<EditorView, ApiError> {
        if let Err(refusal) = self.open.claim(&info.path, Holder::Api) {
            // A window holding it is the case an agent meets: say how to
            // go through that window instead of around it.
            let how = match &refusal {
                Refusal::Elsewhere(owner) if owner.role == "window" => {
                    ". To work on it from an agent, turn on Settings > Automation (and Live Mode to edit the active project on screen) in that window and run the agent with `concat --mcp window`"
                }
                Refusal::Elsewhere(_) => ". Close it there first",
                Refusal::Here(Holder::Window) => {
                    ". Enable Live Mode in Settings > Automation to edit the active window project"
                }
                Refusal::Here(Holder::Api) => "",
            };
            return Err(ApiError::new(
                ErrorCode::Refused,
                format!("{} is {refusal}{how}", info.path),
            ));
        }
        let session = match Session::open_info(&info) {
            Ok(session) => session,
            Err(error) => {
                self.open.release(&info.path);
                return Err(ApiError::failed(error));
            }
        };
        // Recents are a convenience for the launch screen; a machine whose
        // config folder cannot be written still edits.
        let _ = projects::remember(&self.dirs.config, &info);
        let view = session.view();
        self.sessions.insert(key_of(&info.path), session);
        Ok(view)
    }

    /// [`Request::ProjectClose`].
    pub fn close(&mut self, path: &str, save: bool) -> Result<(), ApiError> {
        if self.live_handler_for(path).is_some() {
            return Err(ApiError::new(
                ErrorCode::Refused,
                format!(
                    "{path} is open in the window under Live Mode; close it from the window instead"
                ),
            ));
        }
        if save {
            self.save(path, None)?;
        }
        let closed = self
            .sessions
            .remove(&key_of(path))
            .map(drop)
            .ok_or_else(|| not_open(path));
        if closed.is_ok() {
            self.open.release(path);
        }
        closed
    }

    /// [`Request::ProjectList`].
    pub fn recents(&self) -> Vec<ProjectInfo> {
        projects::list(&self.dirs.config)
    }

    /// [`Request::ProjectSave`].
    pub fn save(&mut self, path: &str, name: Option<&str>) -> Result<(), ApiError> {
        if let Some(handler) = self.live_handler_for(path) {
            handler.dispatch_live(Request::ProjectSave {
                path: path.to_owned(),
                name: name.map(str::to_owned),
            })?;
            return Ok(());
        }
        self.session_mut(path)?.save(name).map_err(ApiError::failed)
    }

    /// [`Request::EditApply`].
    pub fn apply(&mut self, path: &str, command: Command) -> Result<EditorView, ApiError> {
        if let Some(handler) = self.live_handler_for(path) {
            return match handler.dispatch_live(Request::EditApply {
                path: path.to_owned(),
                command: Box::new(command),
            })? {
                Reply::View(view) => Ok(*view),
                _ => Err(ApiError::failed("unexpected live reply")),
            };
        }
        self.session_mut(path)?.apply(command).map_err(refused)
    }

    /// [`Request::MediaImport`]: the probe and the add, as one.
    pub fn import(&mut self, path: &str, file: &str) -> Result<EditorView, ApiError> {
        let item = media::probe(file).map_err(ApiError::failed)?.to_new_media();
        self.apply(path, Command::AddMedia { item })
    }

    /// [`Request::TemplateInstantiate`].
    pub fn instantiate(
        &mut self,
        template: &str,
        location: &str,
        name: &str,
        fills: Vec<Fill>,
    ) -> Result<EditorView, ApiError> {
        self.writable(location)?;
        // Every file is probed before anything is made, so a bad path
        // refuses the whole request rather than leaving a folder behind.
        let fills = fills
            .into_iter()
            .map(|fill| {
                Ok(SlotFill {
                    media_id: fill.media_id,
                    item: media::probe(&fill.file)
                        .map_err(ApiError::failed)?
                        .to_new_media(),
                })
            })
            .collect::<Result<Vec<SlotFill>, ApiError>>()?;
        let info =
            templates::instantiate(template, location, name, fills).map_err(ApiError::failed)?;
        self.adopt(info)
    }

    /// [`Request::TemplateSave`].
    pub fn save_template(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<concat_host::templates::TemplateInfo, ApiError> {
        let live_snapshot = match self.live_handler_for(path) {
            Some(handler) => Some(handler.snapshot_project(path)?),
            None => None,
        };
        let session = match live_snapshot.as_ref() {
            Some(snapshot) => snapshot,
            None => self.session(path)?,
        };
        templates::save(
            &self.dirs.config,
            &session.document(),
            &session.settings(),
            session.path(),
            name,
        )
        .map_err(ApiError::failed)
    }

    /// [`Request::ExportRun`]: what the window's Export sheet does, in its
    /// order, on a thread of its own. Masks first, because the frame loop
    /// reads whatever is in the project's cache and draws the picture as
    /// shot where there is none; then the render, with the titles already
    /// painted here and rejoining the clip list as stills.
    pub fn export(&mut self, path: &str, spec: &ExportSpec) -> Result<Started, ApiError> {
        self.writable(&spec.output)?;
        let live_snapshot = match self.live_handler_for(path) {
            Some(handler) => Some(handler.snapshot_project(path)?),
            None => None,
        };
        let session = match live_snapshot.as_ref() {
            Some(snapshot) => snapshot,
            None => self.session(path)?,
        };
        if session.project().active().clips.is_empty() {
            return Err(ApiError::new(
                ErrorCode::Refused,
                "There is nothing on the timeline to export",
            ));
        }
        let settings = session.settings();
        let (width, height) = checked_size(
            spec.width.unwrap_or(settings.width),
            spec.height.unwrap_or(settings.height),
        )?;
        if let Some(crf) = spec.crf
            && crf > MAX_CRF
        {
            return Err(ApiError::invalid(format!("crf {crf} is over {MAX_CRF}")));
        }
        let rate_num = spec.rate_num.unwrap_or(settings.rate_num);
        let rate_den = spec.rate_den.unwrap_or(settings.rate_den);
        checked_rate(rate_num, rate_den)?;
        let hdr = session.video().color_space.is_hdr() && spec.hdr.unwrap_or(true);
        let codec = match spec.codec.as_deref() {
            None if hdr => export::VideoCodec::Hevc,
            None => export::VideoCodec::H264,
            Some(name) => export::VideoCodec::parse(name).ok_or_else(|| {
                ApiError::invalid(format!(
                    "unknown codec {name:?}: h264, hevc, av1, prores-4444 or vp9-alpha"
                ))
            })?,
        };
        if hdr && codec == export::VideoCodec::H264 {
            return Err(ApiError::invalid(
                "H.264 does not carry HDR: hevc or av1, or hdr false for SDR".to_owned(),
            ));
        }
        let color_range = match spec.color_range.as_deref() {
            None => export::ColorRange::Limited,
            Some(name) => export::ColorRange::parse(name).ok_or_else(|| {
                ApiError::invalid(format!("unknown colour range {name:?}: limited or full"))
            })?,
        };
        let host_spec = export::ExportSpec {
            output: spec.output.clone(),
            crf: spec.crf.unwrap_or(DEFAULT_CRF),
            preset: spec
                .preset
                .clone()
                .unwrap_or_else(|| DEFAULT_PRESET.to_owned()),
            codec,
            ten_bit: spec.ten_bit.unwrap_or(false),
            rate_mode: export::RateMode::Vbr,
            bitrate_kbps: 0,
            color_range,
            hdr,
        };

        let project_path = session.path().to_owned();
        let project_dir = PathBuf::from(&project_path);
        let masks: Vec<(String, AnalyseRequest)> =
            Cutouts::requests(session.project(), &project_dir)
                .into_iter()
                .filter(|(_, request)| Cutouts::outstanding(request) != 0)
                .collect();
        let steadies: Vec<(String, StabilizeRequest)> =
            Stabilizers::requests(session.project(), &project_dir)
                .into_iter()
                .filter(|(_, request)| !Stabilizers::outstanding(request).is_empty())
                .collect();
        let titles = self.title_clips(session, width, height);
        let mut request = export::request(session, &host_spec, titles);
        request.width = width;
        request.height = height;
        request.rate_num = rate_num;
        request.rate_den = rate_den;

        let slot = self
            .exporter
            .begin()
            .map_err(|message| ApiError::new(ErrorCode::Busy, message))?;
        let started = Started {
            job: self.jobs.mint(),
            path: project_path,
            output: spec.output.clone(),
        };
        self.book.start(
            JobStatus::running(&started.job, &started.path, &started.output),
            slot.cancel_handle(),
        );
        let job = ExportJob {
            name: started.job.clone(),
            path: started.path.clone(),
            request,
            masks,
            cutouts: Arc::clone(&self.cutouts),
            steadies,
            stabilizers: Arc::clone(&self.stabilizers),
            events: Arc::clone(&self.events),
        };
        let book = Arc::clone(&self.book);
        let name = started.job.clone();
        let thread = std::thread::Builder::new()
            .name(format!("export {}", started.job))
            .spawn(move || job.run(slot))
            .map_err(|error| {
                book.abandon(&name);
                ApiError::failed(format!("could not start the export: {error}"))
            })?;
        self.jobs.threads.push((started.job.clone(), thread));
        Ok(started)
    }

    /// [`Request::JobCancel`] / [`Request::ExportCancel`].
    pub fn cancel(&mut self, job: &str) -> Result<(), ApiError> {
        self.jobs.reap(&self.book);
        if !self.jobs.threads.iter().any(|(name, _)| name == job) {
            return Err(ApiError::new(
                ErrorCode::NotFound,
                format!("no job {job} is running"),
            ));
        }
        self.book.cancel(job);
        let kind = self.book.status(job).and_then(|status| status.kind);
        match kind.as_deref() {
            None | Some("export") => {
                // Whichever phase it is in: the analysis ahead of the render, or
                // the render itself.
                self.cutouts.cancel();
                self.stabilizers.cancel();
                self.exporter.cancel();
            }
            #[cfg(feature = "speech")]
            Some("captions.transcribe") => self.transcriber.cancel(),
            #[cfg(feature = "speech")]
            Some(k) if k.starts_with("speech.") => self.speech.cancel(),
            _ => {}
        }
        Ok(())
    }

    /// Checks that `path` is open either in headless `sessions` or active in
    /// Live Mode.
    pub(crate) fn ensure_open(&self, path: &str) -> Result<(), ApiError> {
        if self.live_handler_for(path).is_some() || self.sessions.contains_key(&key_of(path)) {
            Ok(())
        } else {
            Err(not_open(path))
        }
    }

    /// Applies `steps` from a background worker onto the active live window
    /// immediately when Live Mode is active, or queues them into
    /// `pending_edits` for [`Api::dispatch`] / [`Api::job_status`].
    /// Returns `(media_ids, clip_ids, error_message)`.
    pub(crate) fn finish_worker_steps(
        job: &str,
        path: &str,
        steps: Vec<PendingStep>,
        live: &Option<Arc<dyn LiveWindowHandler>>,
        pending_edits: &Arc<Mutex<Vec<PendingSessionEdit>>>,
    ) -> (Vec<String>, Vec<String>, Option<String>) {
        if steps.is_empty() {
            return (Vec::new(), Vec::new(), None);
        }
        if let Some(handler) = live
            && handler.is_enabled()
            && handler
                .active_project_path()
                .is_some_and(|active| key_of(&active) == key_of(path))
        {
            let mut media_ids = Vec::new();
            let mut clip_ids = Vec::new();
            let mut retry_from = None;
            for (idx, step) in steps.iter().enumerate() {
                let res = execute_step(
                    step.clone(),
                    &mut |cmd| match handler.dispatch_live(Request::EditApply {
                        path: path.to_owned(),
                        command: Box::new(cmd),
                    })? {
                        Reply::View(view) => Ok(*view),
                        _ => Err(ApiError::failed("unexpected live reply")),
                    },
                    &mut media_ids,
                    &mut clip_ids,
                );
                match res {
                    Ok(()) => {}
                    Err(err) if err.code == ErrorCode::Busy => {
                        retry_from = Some(idx);
                        break;
                    }
                    Err(err) => return (media_ids, clip_ids, Some(err.message)),
                }
            }
            if let Some(start_idx) = retry_from {
                pending_edits
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(PendingSessionEdit {
                        job: job.to_owned(),
                        path: path.to_owned(),
                        steps: steps.into_iter().skip(start_idx).collect(),
                        pre_media_ids: media_ids.clone(),
                        pre_clip_ids: clip_ids.clone(),
                    });
            }
            return (media_ids, clip_ids, None);
        }

        pending_edits
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(PendingSessionEdit {
                job: job.to_owned(),
                path: path.to_owned(),
                steps,
                pre_media_ids: Vec::new(),
                pre_clip_ids: Vec::new(),
            });
        (Vec::new(), Vec::new(), None)
    }

    /// [`Request::LinkProbe`].
    pub fn link_probe(
        &self,
        links: Vec<String>,
        use_drive: bool,
        cookies_path: Option<&str>,
    ) -> Result<LinkProbeInfo, ApiError> {
        let joined = links
            .into_iter()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if joined.is_empty() {
            return Err(ApiError::invalid("provide at least one URL"));
        }
        let cookies = validate_cookies_file(cookies_path)?;
        let tools = link_import::Tools::new(&self.dirs);
        let cancel = AtomicBool::new(false);
        let items = link_import::inspect(&tools, &joined, use_drive, cookies.as_deref(), &cancel)
            .map_err(ApiError::failed)?;
        Ok(LinkProbeInfo {
            items: items.into_iter().map(link_item_to_api).collect(),
            tools_ready: tools.ready(),
            drive_connected: tools.connected(),
        })
    }

    /// [`Request::LinkDownload`].
    pub fn link_download(
        &mut self,
        path: &str,
        items: Vec<LinkItem>,
        height: u32,
        audio_only: bool,
        auto_import: bool,
        cookies_path: Option<&str>,
    ) -> Result<Started, ApiError> {
        if items.is_empty() {
            return Err(ApiError::invalid("provide at least one item to download"));
        }
        self.ensure_open(path)?;
        self.writable(path)?;
        let cookies = validate_cookies_file(cookies_path)?;
        let project_path = key_of(path);
        let tools = link_import::Tools::new(&self.dirs);
        let host_items: Vec<link_import::Item> =
            items.into_iter().map(link_item_from_api).collect();
        let output_dir = PathBuf::from(&project_path)
            .join("media")
            .join("downloads")
            .to_string_lossy()
            .into_owned();
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);
        let job_path = project_path.clone();

        self.spawn_job(
            "link.download",
            path,
            &output_dir,
            move |job_id, cancel, events| {
                let project_dir = PathBuf::from(&job_path);
                let res = link_import::download(
                    &tools,
                    &host_items,
                    &project_dir,
                    height,
                    audio_only,
                    cookies.as_deref(),
                    &cancel,
                    &mut |fraction, stage| {
                        (events)(Event::JobProgress {
                            job: job_id.clone(),
                            path: job_path.clone(),
                            stage: if stage.is_empty() {
                                "downloading".to_owned()
                            } else {
                                stage
                            },
                            fraction: f64::from(fraction),
                        });
                    },
                );

                match res {
                    Ok((files, errors, _failed)) => {
                        if cancel.load(Ordering::Relaxed) {
                            (events)(Event::JobFailed {
                                job: job_id,
                                path: job_path,
                                error: ApiError::new(
                                    ErrorCode::Cancelled,
                                    "The link download was cancelled",
                                ),
                            });
                            return;
                        }
                        if files.is_empty() && !errors.is_empty() {
                            (events)(Event::JobFailed {
                                job: job_id,
                                path: job_path,
                                error: ApiError::failed(errors.join("; ")),
                            });
                            return;
                        }
                        let file_strs: Vec<String> = files
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect();
                        let mut media_ids = Vec::new();
                        let mut probe_errors = errors;
                        if auto_import && !file_strs.is_empty() {
                            let mut steps = Vec::new();
                            for f in &file_strs {
                                match media::probe(f) {
                                    Ok(summary) => {
                                        steps.push(PendingStep::Apply(Command::AddMedia {
                                            item: summary.to_new_media(),
                                        }));
                                    }
                                    Err(e) => probe_errors.push(format!("{f}: {e}")),
                                }
                            }
                            let (ids, _, import_err) = Self::finish_worker_steps(
                                &job_id,
                                &job_path,
                                steps,
                                &live,
                                &pending_edits,
                            );
                            media_ids = ids;
                            if let Some(e) = import_err {
                                probe_errors.push(e);
                            }
                        }
                        let main_output = file_strs.first().cloned().unwrap_or_default();
                        (events)(Event::JobDone {
                            job: job_id,
                            path: job_path,
                            output: main_output,
                            result: Some(json!({
                                "files": file_strs,
                                "mediaIds": media_ids,
                                "errors": probe_errors,
                            })),
                        });
                    }
                    Err(error) => {
                        (events)(Event::JobFailed {
                            job: job_id,
                            path: job_path,
                            error: if cancel.load(Ordering::Relaxed) {
                                ApiError::new(
                                    ErrorCode::Cancelled,
                                    "The link download was cancelled",
                                )
                            } else {
                                ApiError::failed(error)
                            },
                        });
                    }
                }
            },
        )
    }

    /// [`Request::CapCutImport`].
    pub fn capcut_import(
        &mut self,
        input_path: &str,
        location: &str,
        name: Option<&str>,
    ) -> Result<EditorView, ApiError> {
        self.writable(location)?;
        let input = Path::new(input_path);
        if !input.exists() {
            return Err(ApiError::new(
                ErrorCode::NotFound,
                format!("CapCut draft path not found: {input_path}"),
            ));
        }
        let imported =
            capcut::import_capcut_draft(input, Path::new(location), name, false, &self.dirs)
                .map_err(ApiError::failed)?;
        self.adopt(imported.project)
    }

    /// [`Request::CapCutExport`].
    pub fn capcut_export(
        &mut self,
        path: &str,
        output_dir: Option<&str>,
        text_options: Option<&CapCutTextSpec>,
    ) -> Result<CapCutExportInfo, ApiError> {
        let target_base = match output_dir {
            Some(dir) => PathBuf::from(dir),
            None => capcut::default_capcut_drafts_dir()
                .unwrap_or_else(|| self.dirs.data.join("CapCut_Drafts")),
        };
        self.writable(&target_base.to_string_lossy())?;

        let live_snapshot = match self.live_handler_for(path) {
            Some(handler) => Some(handler.snapshot_project(path)?),
            None => None,
        };
        let session = match live_snapshot.as_ref() {
            Some(snapshot) => snapshot,
            None => self.session(path)?,
        };

        let mut opts = capcut::CapCutTextOptions::default();
        if let Some(spec) = text_options {
            if let Some(v) = spec.font_size {
                opts.font_size = v;
            }
            if let Some(v) = spec.scale_percent {
                opts.scale_percent = v;
            }
            if let Some(v) = spec.position_y {
                opts.position_y = v;
            }
            if let Some(v) = spec.stroke_enabled {
                opts.stroke_enabled = v;
            }
            if let Some(ref c) = spec.stroke_color {
                opts.stroke_color = c.clone();
            }
            if let Some(v) = spec.stroke_width {
                opts.stroke_width = v;
            }
        }

        let explicit_dir = output_dir.map(PathBuf::from);
        let result = capcut::export_project_to_capcut(
            session.project(),
            Path::new(session.path()),
            &session.settings().name,
            &opts,
            explicit_dir.as_deref(),
            &self.dirs,
        )
        .map_err(ApiError::failed)?;

        Ok(CapCutExportInfo {
            draft_dir: result.draft_dir.to_string_lossy().into_owned(),
            in_capcut_app: result.in_capcut_app,
            has_unbaked_effects: result.has_unbaked_effects,
        })
    }

    /// The timeline's titles painted for a `width` × `height` frame, as
    /// the stills that stand in for them.
    fn title_clips(&self, session: &Session, width: u32, height: u32) -> Vec<ExportClip> {
        self.titles
            .clips(session.project(), width, height)
            .into_iter()
            .map(|title| title.clip)
            .collect()
    }

    /// [`Request::PreviewFrame`] with an output: the paused monitor's true
    /// frame, to a file.
    pub fn frame(
        &mut self,
        path: &str,
        time: f64,
        output: &str,
        size: Option<(u32, u32)>,
    ) -> Result<Written, ApiError> {
        self.writable(output)?;
        let (width, height, pixels) = self.pixels(path, time, size)?;
        write_png(Path::new(output), width, height, &pixels)?;
        Ok(Written {
            path: output.to_owned(),
            width,
            height,
        })
    }

    /// [`Request::PreviewFrame`] without one: the same frame, inline.
    pub fn picture(
        &mut self,
        path: &str,
        time: f64,
        size: Option<(u32, u32)>,
    ) -> Result<Picture, ApiError> {
        let (width, height, pixels) = self.pixels(path, time, size)?;
        let mut png = Vec::new();
        encode_png(&mut png, width, height, &pixels)
            .map_err(|error| ApiError::failed(format!("could not encode the frame: {error}")))?;
        Ok(Picture {
            width,
            height,
            png: base64::engine::general_purpose::STANDARD.encode(png),
        })
    }

    /// The composited RGBA frame at `time`.
    fn pixels(
        &mut self,
        path: &str,
        time: f64,
        size: Option<(u32, u32)>,
    ) -> Result<(u32, u32, Vec<u8>), ApiError> {
        let live_snapshot = match self.live_handler_for(path) {
            Some(handler) => Some(handler.snapshot_project(path)?),
            None => None,
        };
        let session = match live_snapshot.as_ref() {
            Some(snapshot) => snapshot,
            None => self.session(path)?,
        };
        let settings = session.settings();
        let (width, height) = size.unwrap_or((settings.width, settings.height));
        let (width, height) = checked_size(width, height)?;
        let mut clips = session.flattened_clips();
        clips.extend(self.title_clips(session, width, height));
        let pixels = self
            .monitor
            .frame(
                Arc::new(clips),
                &settings,
                FrameSpec {
                    time,
                    width,
                    height,
                    moving: false,
                    proxy: false,
                    color_space: session.video().color_space,
                    canvas: session.video().canvas,
                    bypass_effects: false,
                },
            )
            .map_err(ApiError::failed)?;
        Ok((width, height, pixels))
    }

    pub(crate) fn session(&self, path: &str) -> Result<&Session, ApiError> {
        self.sessions
            .get(&key_of(path))
            .ok_or_else(|| not_open(path))
    }

    fn session_mut(&mut self, path: &str) -> Result<&mut Session, ApiError> {
        self.sessions
            .get_mut(&key_of(path))
            .ok_or_else(|| not_open(path))
    }
}

/// The jobs the API has begun, by name, with the thread each runs on.
#[derive(Default)]
struct Jobs {
    minted: u64,
    threads: Vec<(String, JoinHandle<()>)>,
}

impl Jobs {
    /// The next name.
    fn mint(&mut self) -> String {
        self.minted += 1;
        format!("j{}", self.minted)
    }

    /// Forgets the threads that have ended. One that ended without its
    /// last event - a panic in the render - is booked as failed.
    fn reap(&mut self, book: &job::Book) {
        self.threads.retain(|(name, thread)| {
            let ended = thread.is_finished();
            if ended {
                book.abandon(name);
            }
            !ended
        });
    }

    /// Waits for the rest.
    fn finish(&mut self) {
        for (_, thread) in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// An export in flight: everything its thread needs, owned.
struct ExportJob {
    name: String,
    path: String,
    request: ExportRequest,
    masks: Vec<(String, AnalyseRequest)>,
    cutouts: Arc<Cutouts>,
    steadies: Vec<(String, StabilizeRequest)>,
    stabilizers: Arc<Stabilizers>,
    events: EventSink,
}

impl ExportJob {
    /// The whole job, start to end, on the thread it is called from. The
    /// slot is held until this returns, so the next export waits its turn.
    fn run(self, slot: concat_host::jobs::Job) {
        let outcome = self
            .masks(&slot)
            .and_then(|()| self.steadies(&slot))
            .and_then(|()| {
                export::run(&self.request, slot.cancel_flag(), |progress| {
                    (self.events)(Event::progress(&self.name, &self.path, progress));
                })
            });
        let event = match outcome {
            Ok(output) => Event::ExportDone {
                job: self.name.clone(),
                path: self.path.clone(),
                output,
                width: self.request.width,
                height: self.request.height,
            },
            Err(message) => Event::ExportFailed {
                job: self.name.clone(),
                path: self.path.clone(),
                error: if slot.cancelled() {
                    ApiError::new(ErrorCode::Cancelled, "The export was cancelled")
                } else {
                    ApiError::failed(message)
                },
            },
        };
        (self.events)(event);
    }

    /// Runs every cutout analysis the timeline still needs, one after the
    /// other, so the render that follows cuts every clip it should.
    fn masks(&self, slot: &concat_host::jobs::Job) -> Result<(), String> {
        for (media_id, request) in &self.masks {
            if slot.cancelled() {
                return Err("cancelled".to_owned());
            }
            self.cutouts.analyse(request, &mut |progress| {
                let (fetching, fraction) = match progress {
                    cutout::Progress::Fetching { received, total } => {
                        (true, received as f32 / total.max(1) as f32)
                    }
                    cutout::Progress::Analysing(fraction) => (false, fraction),
                };
                (self.events)(Event::CutoutProgress {
                    job: self.name.clone(),
                    path: self.path.clone(),
                    media_id: media_id.clone(),
                    fetching,
                    fraction,
                });
            })?;
        }
        Ok(())
    }
}

impl ExportJob {
    /// Measures every stabilized clip's media the timeline still needs,
    /// one after the other, so the render that follows steadies every
    /// clip it should.
    fn steadies(&self, slot: &concat_host::jobs::Job) -> Result<(), String> {
        for (media_id, request) in &self.steadies {
            if slot.cancelled() {
                return Err("cancelled".to_owned());
            }
            self.stabilizers.analyse(request, &mut |fraction| {
                (self.events)(Event::StabilizeProgress {
                    job: self.name.clone(),
                    path: self.path.clone(),
                    media_id: media_id.clone(),
                    fraction,
                });
            })?;
        }
        Ok(())
    }
}

/// [`Request::CatalogueList`]: the built-in packages plus whatever
/// [`Catalogue::install`] added, in id order.
fn catalogue(kind: Option<&str>) -> Result<Vec<PackageInfo>, ApiError> {
    let kind = match kind {
        None => None,
        Some("effect") => Some(Kind::Effect),
        Some("filter") => Some(Kind::Filter),
        Some("audio") => Some(Kind::Audio),
        Some("transition") => Some(Kind::Transition),
        Some("generator") => Some(Kind::Generator),
        Some(other) => {
            return Err(ApiError::invalid(format!(
                "{other:?} is not a kind: effect, filter, audio, transition or generator"
            )));
        }
    };
    let mut packages: Vec<PackageInfo> = Catalogue::builtin()
        .packages()
        .filter(|package| kind.is_none_or(|kind| package.kind() == kind))
        .map(|package| {
            let meta = &package.manifest.effect;
            PackageInfo {
                id: meta.id.clone(),
                name: meta.name.clone(),
                kind: format!("{:?}", meta.kind).to_ascii_lowercase(),
                category: meta.category.clone(),
                description: meta.description.clone(),
                intensity: meta.intensity.clone(),
                params: package
                    .manifest
                    .params
                    .iter()
                    .map(|param| ParamInfo {
                        key: param.key.clone(),
                        label: param.label.clone(),
                        kind: format!("{:?}", param.kind).to_ascii_lowercase(),
                        min: param.min,
                        max: param.max,
                        default: param.default,
                        step: param.step,
                        unit: param.unit.clone(),
                        animate: param.animate,
                        values: param.values.clone(),
                        labels: param.labels.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    packages.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(packages)
}

/// The key a project folder is held under: its canonical path where the
/// folder exists, so `.` and an absolute spelling of it are one session,
/// and the path as given where it does not yet.
impl Drop for Api {
    /// The register is shared with an embedder that outlives this API:
    /// what it held open is given back.
    fn drop(&mut self) {
        for key in self.sessions.keys() {
            self.open.release(key);
        }
    }
}

/// A frame size a caller may ask for, or why not.
fn checked_size(width: u32, height: u32) -> Result<(u32, u32), ApiError> {
    if width == 0 || height == 0 {
        return Err(ApiError::invalid("A frame needs a width and a height"));
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(ApiError::invalid(format!(
            "{width}×{height} is over {MAX_SIDE} a side"
        )));
    }
    Ok((width, height))
}

/// A frame rate a caller may ask for, or why not.
fn checked_rate(num: i64, den: i64) -> Result<(), ApiError> {
    if num <= 0 || den <= 0 {
        return Err(ApiError::invalid(
            "A frame rate needs a positive numerator and denominator",
        ));
    }
    if num as f64 / den as f64 > MAX_RATE {
        return Err(ApiError::invalid(format!(
            "{num}/{den} is over {MAX_RATE} frames a second"
        )));
    }
    Ok(())
}

/// `path` with its symlinks resolved as far as it exists: the deepest
/// ancestor that is there, canonicalised, with the rest appended. A file
/// not written yet still says where it would land.
fn resolved(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

fn same_media_path(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let fold = |byte: u8| match byte {
        b'\\' => b'/',
        other => other.to_ascii_lowercase(),
    };
    cfg!(windows) && a.len() == b.len() && a.bytes().zip(b.bytes()).all(|(x, y)| fold(x) == fold(y))
}

fn execute_step<F>(
    step: PendingStep,
    apply_cmd: &mut F,
    media_ids: &mut Vec<String>,
    clip_ids: &mut Vec<String>,
) -> Result<(), ApiError>
where
    F: FnMut(Command) -> Result<EditorView, ApiError>,
{
    match step {
        PendingStep::Apply(cmd) => {
            let is_media = matches!(&cmd, Command::AddMedia { .. });
            let fallback_path = match &cmd {
                Command::AddMedia { item } => Some(item.path.clone()),
                _ => None,
            };
            let is_clip = matches!(
                &cmd,
                Command::AddClip { .. }
                    | Command::AddClipAtFirstFree { .. }
                    | Command::AddTextClip { .. }
            );
            let view = apply_cmd(cmd)?;
            if is_media {
                let id = view.created_id.or_else(|| {
                    fallback_path.and_then(|p| {
                        view.project
                            .media
                            .iter()
                            .find(|m| same_media_path(&m.path, &p))
                            .map(|m| m.id.clone())
                    })
                });
                if let Some(id) = id {
                    media_ids.push(id);
                }
            } else if is_clip && let Some(id) = view.created_id {
                clip_ids.push(id);
            }
            Ok(())
        }
        PendingStep::ImportAndPlace {
            item,
            track_id,
            start,
        } => {
            let item_path = item.path.clone();
            let view = apply_cmd(Command::AddMedia { item })?;
            let media_id = view
                .created_id
                .or_else(|| {
                    view.project
                        .media
                        .iter()
                        .find(|m| same_media_path(&m.path, &item_path))
                        .map(|m| m.id.clone())
                })
                .ok_or_else(|| ApiError::failed("added media missing from project bin"))?;
            let place_cmd = match track_id {
                Some(track_id) => Command::AddClip {
                    media_id: media_id.clone(),
                    track_id,
                    start,
                    ripple: false,
                },
                None => Command::AddClipAtFirstFree {
                    media_id: media_id.clone(),
                    start,
                },
            };
            let placed = apply_cmd(place_cmd)?;
            media_ids.push(media_id);
            if let Some(cid) = placed.created_id {
                clip_ids.push(cid);
            }
            Ok(())
        }
    }
}

pub(crate) fn key_of(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|canonical| canonical.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

fn not_open(path: &str) -> ApiError {
    ApiError::new(
        ErrorCode::NotOpen,
        format!("{path} is not open - open it first"),
    )
}

/// The edit layer's no, as the code that says so.
fn refused(message: String) -> ApiError {
    ApiError::new(ErrorCode::Refused, message)
}

/// Writes RGBA pixels as a PNG, creating the folder above the file.
fn write_png(output: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), ApiError> {
    let could_not = |error: &dyn std::fmt::Display| {
        ApiError::failed(format!("could not write {}: {error}", output.display()))
    };
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            ApiError::failed(format!("could not create {}: {error}", parent.display()))
        })?;
    }
    let file = std::fs::File::create(output).map_err(|error| could_not(&error))?;
    encode_png(std::io::BufWriter::new(file), width, height, pixels)
        .map_err(|error| could_not(&error))
}

/// Encodes RGBA pixels as a PNG into `sink`.
fn encode_png(
    sink: impl std::io::Write,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), png::EncodingError> {
    let mut encoder = png::Encoder::new(sink, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    writer.finish()
}

fn validate_cookies_file(cookies_path: Option<&str>) -> Result<Option<PathBuf>, ApiError> {
    let Some(raw) = cookies_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(raw);
    if !path.is_file() {
        return Err(ApiError::new(
            ErrorCode::NotFound,
            format!("cookies file does not exist: {raw}"),
        ));
    }
    Ok(Some(path))
}

fn link_item_to_api(item: link_import::Item) -> LinkItem {
    LinkItem {
        title: item.title,
        url: item.url,
        drive_id: item.drive_id,
        public_drive: item.public_drive,
        duration: item.duration,
        size: item.size,
        thumbnail: item.thumbnail,
        heights: item.heights,
    }
}

fn link_item_from_api(item: LinkItem) -> link_import::Item {
    link_import::Item {
        title: item.title,
        url: item.url,
        drive_id: item.drive_id,
        public_drive: item.public_drive,
        duration: item.duration,
        size: item.size,
        thumbnail: item.thumbnail,
        heights: item.heights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concat_project::commands::NewMedia;
    use concat_project::model::MediaKind;
    use serde_json::json;
    use std::sync::Mutex;

    /// An API whose config and data live in a scratch folder that goes
    /// away with the test, and whose events pile up where a test can read
    /// them.
    fn api() -> (Api, tempfile::TempDir, Arc<Mutex<Vec<Event>>>) {
        let scratch = tempfile::tempdir().expect("scratch");
        let dirs = AppDirs {
            config: scratch.path().join("config"),
            data: scratch.path().join("data"),
        };
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let api = Api::with_dirs(
            dirs,
            Arc::new(move |event| sink.lock().expect("events").push(event)),
        );
        (api, scratch, events)
    }

    fn ok(response: Response) -> Reply {
        match response {
            Response::Result(reply) => reply,
            Response::Error(error) => panic!("refused: {error}"),
        }
    }

    fn err(response: Response) -> ApiError {
        match response {
            Response::Result(_) => panic!("worked, unexpectedly"),
            Response::Error(error) => error,
        }
    }

    fn view(reply: Reply) -> EditorView {
        match reply {
            Reply::View(view) => *view,
            _ => panic!("not a view"),
        }
    }

    fn still(path: &str) -> NewMedia {
        NewMedia {
            path: path.to_owned(),
            name: "still.png".to_owned(),
            duration: None,
            kind: MediaKind::Image,
            width: Some(640),
            height: Some(360),
            frame_rate: None,
            frame_rate_fraction: None,
            video_codec: None,
            audio_codec: None,
            has_audio: false,
            audio_tracks: Vec::new(),
            origin: None,
            color_space: Default::default(),
        }
    }

    /// A project named `name` under the scratch folder, open.
    fn project(api: &mut Api, scratch: &tempfile::TempDir, name: &str) -> String {
        let location = scratch.path().to_string_lossy().into_owned();
        ok(api.dispatch(Request::ProjectCreate {
            location: location.clone(),
            name: name.to_owned(),
            video: None,
        }));
        scratch.path().join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn version_names_what_the_build_serves() {
        let (mut api, _scratch, _events) = api();
        let base = api.version();
        assert_eq!(base.api_version, API_VERSION);
        assert_eq!(base.capabilities, vec!["events".to_owned()]);
        assert!(!base.capabilities.iter().any(|name| name == "gpu"));

        api.add_capability("json-rpc");
        api.add_capability("json-rpc");
        api.add_capability("grpc");
        let served = serde_json::to_value(ok(api.dispatch(Request::Version))).expect("JSON");
        assert_eq!(
            served["capabilities"],
            json!(["events", "json-rpc", "grpc"]),
            "each name once, in the order added"
        );
    }

    #[test]
    fn a_request_parses_by_method_and_carries_a_command_verbatim() {
        let request: Request = serde_json::from_value(json!({
            "method": "edit.apply",
            "path": "/p",
            "command": { "op": "addTextClip", "start": 1.5 }
        }))
        .expect("parses");
        assert_eq!(
            request,
            Request::EditApply {
                path: "/p".to_owned(),
                command: Box::new(Command::AddTextClip {
                    above: false,
                    track_id: None,
                    start: 1.5,
                    style: None,
                    duration: None,
                    offset_y: None,
                }),
            }
        );
    }

    #[test]
    fn a_response_is_a_result_or_an_error_object() {
        let worked = serde_json::to_value(Response::Result(Reply::Done(Done {}))).expect("json");
        assert_eq!(worked, json!({ "result": {} }));
        let refused =
            serde_json::to_value(Response::Error(ApiError::new(ErrorCode::Refused, "no")))
                .expect("json");
        assert_eq!(
            refused,
            json!({ "error": { "code": "refused", "message": "no" } })
        );
    }

    #[test]
    fn create_edit_save_and_reopen_round_trip() {
        let (mut api, scratch, _) = api();
        let location = scratch.path().join("projects");
        std::fs::create_dir_all(&location).expect("location");
        let location = location.to_string_lossy().into_owned();

        let created = view(ok(api.dispatch(Request::ProjectCreate {
            location: location.clone(),
            name: "Round trip".to_owned(),
            video: Some(VideoSettings {
                canvas: [0, 0, 0, 255],
                width: 1080,
                height: 1920,
                rate_num: 60,
                rate_den: 1,
                color_space: Default::default(),
            }),
        })));
        assert_eq!(created.settings.width, 1080);
        assert_eq!(created.settings.height, 1920);
        assert_eq!(created.settings.rate_num, 60);
        let path = format!("{location}/Round trip");
        assert!(projects::is_project(Path::new(&path)));

        let added = view(ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddMedia {
                item: still("/nowhere/still.png"),
            }),
        })));
        let media_id = added.created_id.expect("minted");
        let placed = view(ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddClipAtFirstFree {
                media_id,
                start: 2.0,
            }),
        })));
        assert!(placed.can_undo);
        assert_eq!(placed.project.active().clips.len(), 1);

        ok(api.dispatch(Request::ProjectClose {
            path: path.clone(),
            save: true,
        }));
        assert_eq!(
            err(api.dispatch(Request::ProjectGet { path: path.clone() })),
            not_open(&path)
        );

        let reopened = view(ok(api.dispatch(Request::ProjectOpen { path: path.clone() })));
        assert_eq!(reopened.project.active().clips.len(), 1);
        assert_eq!(reopened.project.active().clips[0].start, 2.0);
        assert!(!reopened.can_undo, "history does not survive a save");

        let recents = match ok(api.dispatch(Request::ProjectList)) {
            Reply::Projects(list) => list,
            _ => panic!("not a list"),
        };
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].name, "Round trip");
    }

    /// A clip's mask over the wire, as a caller writes it: put on through
    /// `updateClip`'s patch, its numbers keyed through `setClipKey`, and
    /// read back in the project the reply carries (#239).
    #[test]
    fn a_mask_is_set_and_keyed_through_the_api() {
        let (mut api, scratch, _) = api();
        let path = project(&mut api, &scratch, "Masked");
        let added = view(ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddMedia {
                item: still("/nowhere/still.png"),
            }),
        })));
        let placed = view(ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddClipAtFirstFree {
                media_id: added.created_id.expect("minted"),
                start: 0.0,
            }),
        })));
        let clip_id = placed.project.active().clips[0].id.clone();
        for command in [
            json!({ "op": "updateClip", "clipId": clip_id,
                    "patch": { "mask": { "shape": "star", "feather": 0.3, "invert": true } } }),
            json!({ "op": "setClipKey", "clipId": clip_id, "property": "maskWidth",
                    "at": 0.0, "value": 0.2 }),
            json!({ "op": "setClipKey", "clipId": clip_id, "property": "maskWidth",
                    "at": 1.0, "value": 1.2 }),
        ] {
            let request: Request = serde_json::from_value(json!({
                "method": "edit.apply", "path": path, "command": command
            }))
            .expect("parses");
            ok(api.dispatch(request));
        }
        let got = view(ok(api.dispatch(Request::ProjectGet { path })));
        let clip = &got.project.active().clips[0];
        let mask = clip.mask.expect("a mask");
        assert_eq!(mask.shape, concat_project::model::MaskShape::Star);
        assert!(mask.invert && (mask.feather - 0.3).abs() < 1e-12);
        let width = clip.value_at(concat_project::model::KeyProperty::MaskWidth, 0.5);
        assert!((width - 0.7).abs() < 1e-12, "the key rides: {width}");
    }

    #[test]
    fn a_refusal_is_the_command_layers_sentence_with_its_code() {
        let (mut api, scratch, _) = api();
        let path = project(&mut api, &scratch, "Refused");
        let created = view(ok(api.dispatch(Request::ProjectGet { path: path.clone() })));
        let track_id = created.project.active().tracks[0].id.clone();
        let refused = err(api.dispatch(Request::EditApply {
            path,
            command: Box::new(Command::AddClip {
                media_id: "m999".to_owned(),
                track_id,
                start: 0.0,
                ripple: false,
            }),
        }));
        assert_eq!(refused.code, ErrorCode::Refused);
        assert_eq!(refused.message, "That media is no longer in the bin.");
    }

    #[test]
    fn creating_over_a_project_is_refused() {
        let (mut api, scratch, _) = api();
        let location = scratch.path().to_string_lossy().into_owned();
        let create = || Request::ProjectCreate {
            location: location.clone(),
            name: "Twice".to_owned(),
            video: None,
        };
        ok(api.dispatch(create()));
        let again = err(api.dispatch(create()));
        assert_eq!(again.code, ErrorCode::Failed);
        assert!(again.message.contains("already exists"));
    }

    #[test]
    fn the_catalogue_lists_packages_with_their_knobs() {
        let packages = catalogue(Some("filter")).expect("kind");
        assert!(!packages.is_empty());
        assert!(packages.iter().all(|package| package.kind == "filter"));
        assert!(packages.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert_eq!(
            catalogue(Some("look")).expect_err("not a kind").code,
            ErrorCode::Invalid
        );
        let all = catalogue(None).expect("all");
        assert!(all.len() > packages.len());
        assert!(all.iter().any(|package| !package.params.is_empty()));
    }

    #[test]
    fn a_method_on_a_closed_project_says_so() {
        let (mut api, _scratch, _) = api();
        let refused = err(api.dispatch(Request::EditUndo {
            path: "/never".to_owned(),
        }));
        assert_eq!(refused, not_open("/never"));
        assert_eq!(refused.code, ErrorCode::NotOpen);
    }

    #[test]
    fn undo_and_redo_step_the_history() {
        let (mut api, scratch, _) = api();
        let path = project(&mut api, &scratch, "History");
        ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddTextClip {
                above: false,
                track_id: None,
                start: 0.0,
                style: None,
                duration: Some(3.0),
                offset_y: None,
            }),
        }));
        let undone = view(ok(api.dispatch(Request::EditUndo { path: path.clone() })));
        assert!(undone.project.active().clips.is_empty());
        assert!(undone.can_redo);
        let redone = view(ok(api.dispatch(Request::EditRedo { path })));
        assert_eq!(redone.project.active().clips.len(), 1);
    }

    #[test]
    fn the_document_is_what_a_save_writes() {
        let (mut api, scratch, _) = api();
        let path = project(&mut api, &scratch, "Doc");
        let document = match ok(api.dispatch(Request::ProjectDocument { path: path.clone() })) {
            Reply::Document(document) => document,
            _ => panic!("not a document"),
        };
        ok(api.dispatch(Request::ProjectSave {
            path: path.clone(),
            name: None,
        }));
        assert_eq!(
            projects::read_document(&path)
                .expect("saved")
                .expect("a document"),
            document
        );
    }

    #[test]
    fn an_empty_timeline_has_nothing_to_export_and_no_job_to_cancel() {
        let (mut api, scratch, _) = api();
        let path = project(&mut api, &scratch, "Empty");
        let refused = err(api.dispatch(Request::ExportRun {
            path,
            spec: ExportSpec {
                output: scratch
                    .path()
                    .join("out.mp4")
                    .to_string_lossy()
                    .into_owned(),
                ..ExportSpec::default()
            },
        }));
        assert_eq!(refused.code, ErrorCode::Refused);
        let missing = err(api.dispatch(Request::ExportCancel {
            job: "j1".to_owned(),
        }));
        assert_eq!(missing.code, ErrorCode::NotFound);
        assert_eq!(api.running(), 0);
    }

    #[test]
    fn an_export_is_a_job_that_reports_how_it_ended() {
        let (mut api, scratch, events) = api();
        let path = project(&mut api, &scratch, "Job");
        // A still that does not exist: the job starts, since the timeline
        // has a clip, and fails in the render, which is an event.
        let added = view(ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddMedia {
                item: still("/nowhere/still.png"),
            }),
        })));
        ok(api.dispatch(Request::EditApply {
            path: path.clone(),
            command: Box::new(Command::AddClipAtFirstFree {
                media_id: added.created_id.expect("minted"),
                start: 0.0,
            }),
        }));
        let started = match ok(api.dispatch(Request::ExportRun {
            path: path.clone(),
            spec: ExportSpec {
                output: scratch
                    .path()
                    .join("out.mp4")
                    .to_string_lossy()
                    .into_owned(),
                ..ExportSpec::default()
            },
        })) {
            Reply::Started(started) => started,
            _ => panic!("not a job"),
        };
        assert_eq!(started.job, "j1");
        api.finish();
        let events = events.lock().expect("events");
        let last = events.last().expect("the job said how it ended");
        assert_eq!(last.job(), "j1");
        match last {
            Event::ExportFailed {
                path: at, error, ..
            } => {
                assert_eq!(at, &path);
                assert_eq!(error.code, ErrorCode::Failed);
            }
            other => panic!("ended with {other:?}"),
        }
        drop(events);
        assert_eq!(api.running(), 0);
        // Asked afterwards, by a caller who was not listening, the job
        // says the same.
        let status = match ok(api.dispatch(Request::ExportStatus {
            job: "j1".to_owned(),
        })) {
            Reply::Job(status) => status,
            _ => panic!("not a job's status"),
        };
        assert_eq!(status.state, JobState::Failed);
        assert_eq!(status.path, path);
        assert_eq!(
            status.error.map(|error| error.code),
            Some(ErrorCode::Failed)
        );
        let unknown = err(api.dispatch(Request::ExportStatus {
            job: "j9".to_owned(),
        }));
        assert_eq!(unknown.code, ErrorCode::NotFound);
        assert_eq!(api.stopper().cancel_all(), 0, "nothing left to stop");
    }

    #[test]
    fn a_frame_comes_back_inline_when_no_output_is_named() {
        let (mut api, scratch, _) = api();
        let path = project(&mut api, &scratch, "Frame");
        let picture = match ok(api.dispatch(Request::PreviewFrame {
            path: path.clone(),
            time: 0.0,
            output: None,
            width: Some(16),
            height: Some(9),
        })) {
            Reply::Picture(picture) => picture,
            _ => panic!("not a picture"),
        };
        assert_eq!((picture.width, picture.height), (16, 9));
        let png = base64::engine::general_purpose::STANDARD
            .decode(picture.png)
            .expect("base64");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let reader = decoder.read_info().expect("a PNG");
        assert_eq!((reader.info().width, reader.info().height), (16, 9));

        let refused = err(api.dispatch(Request::PreviewFrame {
            path,
            time: 0.0,
            output: None,
            width: Some(0),
            height: Some(9),
        }));
        assert_eq!(refused.code, ErrorCode::Invalid);
    }

    #[test]
    fn writes_stay_under_the_roots() {
        let (mut api, scratch, _events) = api();
        let allowed = scratch.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed");
        api.restrict_writes_to(vec![allowed.clone()]);
        let outside = scratch
            .path()
            .join("elsewhere")
            .to_string_lossy()
            .into_owned();
        let refused = err(api.dispatch(Request::ProjectCreate {
            location: outside.clone(),
            name: "Out".to_owned(),
            video: None,
        }));
        assert_eq!(refused.code, ErrorCode::Refused);
        assert!(!scratch.path().join("elsewhere").exists(), "nothing made");
        let inside = allowed.to_string_lossy().into_owned();
        ok(api.dispatch(Request::ProjectCreate {
            location: inside.clone(),
            name: "In".to_owned(),
            video: None,
        }));
        let path = format!("{inside}/In");
        for output in [
            format!("{inside}/../elsewhere/frame.png"),
            format!("{outside}/frame.png"),
        ] {
            let refused = err(api.dispatch(Request::PreviewFrame {
                path: path.clone(),
                time: 0.0,
                output: Some(output.clone()),
                width: None,
                height: None,
            }));
            assert_eq!(refused.code, ErrorCode::Refused, "{output}");
        }
        // Under the root, into a folder not there yet, is fine.
        let written = serde_json::to_value(ok(api.dispatch(Request::PreviewFrame {
            path,
            time: 0.0,
            output: Some(format!("{inside}/frames/first.png")),
            width: Some(16),
            height: Some(9),
        })))
        .expect("JSON");
        assert!(
            written["path"]
                .as_str()
                .is_some_and(|written| written.ends_with("first.png"))
        );
    }

    #[test]
    fn a_frame_over_8k_a_side_is_invalid() {
        let (mut api, scratch, _events) = api();
        let path = project(&mut api, &scratch, "Big");
        let refused = err(api.dispatch(Request::PreviewFrame {
            path,
            time: 0.0,
            output: None,
            width: Some(65_535),
            height: Some(65_535),
        }));
        assert_eq!(refused.code, ErrorCode::Invalid);
    }

    #[test]
    fn a_project_the_window_holds_is_refused_and_the_other_way_round() {
        let (mut api, scratch, _events) = api();
        let path = project(&mut api, &scratch, "Mine");
        ok(api.dispatch(Request::ProjectClose {
            path: path.clone(),
            save: false,
        }));
        let open = OpenProjects::default();
        api.share_open_projects(open.clone());
        open.claim(&path, Holder::Window)
            .expect("the window has it");
        let refused = err(api.dispatch(Request::ProjectOpen { path: path.clone() }));
        assert_eq!(refused.code, ErrorCode::Refused);
        open.release(&path);
        ok(api.dispatch(Request::ProjectOpen { path: path.clone() }));
        assert_eq!(open.holder(&path), Some(Holder::Api));
        assert_eq!(
            open.claim(&path, Holder::Window),
            Err(Refusal::Here(Holder::Api)),
            "and the window is refused in turn"
        );
        assert!(
            Path::new(&path).join(lock::FILE).exists(),
            "and every other process by the lock"
        );
        ok(api.dispatch(Request::ProjectClose {
            path: path.clone(),
            save: false,
        }));
        assert_eq!(open.holder(&path), None, "closing gives it back");
        assert!(!Path::new(&path).join(lock::FILE).exists(), "lock and all");
    }

    /// Two registers are two processes as far as the lock goes: an API
    /// that does not share the window's register - `concat --mcp` on its
    /// own - is kept off a folder the window has, and told the way in.
    #[test]
    fn a_project_another_process_holds_is_refused_with_the_way_in() {
        let (mut other, _other_scratch, _other_events) = api();
        let (mut api, scratch, _events) = api();
        let path = project(&mut api, &scratch, "Held");
        ok(api.dispatch(Request::ProjectClose {
            path: path.clone(),
            save: false,
        }));
        let window = lock::ProjectLock::acquire(Path::new(&path), "window").expect("the window");
        let refused = err(api.dispatch(Request::ProjectOpen { path: path.clone() }));
        assert_eq!(refused.code, ErrorCode::Refused);
        assert!(
            refused.message.contains("a CDCAi window (process"),
            "{}",
            refused.message
        );
        assert!(
            refused.message.contains("concat --mcp window"),
            "{}",
            refused.message
        );
        drop(window);
        ok(api.dispatch(Request::ProjectOpen { path: path.clone() }));

        let refused = err(other.dispatch(Request::ProjectOpen { path: path.clone() }));
        assert!(
            refused.message.contains("another CDCAi process"),
            "{}",
            refused.message
        );
    }

    struct MockLiveWindow {
        enabled: Arc<std::sync::atomic::AtomicBool>,
        busy: Arc<std::sync::atomic::AtomicBool>,
        session: Mutex<Session>,
    }

    impl LiveWindowHandler for MockLiveWindow {
        fn is_enabled(&self) -> bool {
            self.enabled.load(std::sync::atomic::Ordering::Acquire)
        }

        fn active_project_path(&self) -> Option<String> {
            self.is_enabled()
                .then(|| self.session.lock().expect("live").path().to_owned())
        }

        fn dispatch_live(&self, request: Request) -> Result<Reply, ApiError> {
            let busy = self.busy.load(std::sync::atomic::Ordering::Acquire);
            let mut session = self.session.lock().expect("live");
            match request {
                Request::ProjectOpen { .. } | Request::ProjectGet { .. } => {
                    Ok(Reply::View(Box::new(session.view())))
                }
                Request::ProjectDocument { .. } => Ok(Reply::Document(session.document())),
                Request::ProjectSave { name, .. } => {
                    if busy {
                        return Err(ApiError::new(ErrorCode::Busy, "busy"));
                    }
                    session.save(name.as_deref()).map_err(ApiError::failed)?;
                    Ok(Reply::Done(Done {}))
                }
                Request::EditApply { command, .. } => {
                    if busy {
                        return Err(ApiError::new(ErrorCode::Busy, "busy"));
                    }
                    let view = session.apply(*command).map_err(refused)?;
                    Ok(Reply::View(Box::new(view)))
                }
                Request::EditUndo { .. } => {
                    if busy {
                        return Err(ApiError::new(ErrorCode::Busy, "busy"));
                    }
                    Ok(Reply::View(Box::new(session.undo())))
                }
                Request::EditRedo { .. } => {
                    if busy {
                        return Err(ApiError::new(ErrorCode::Busy, "busy"));
                    }
                    Ok(Reply::View(Box::new(session.redo())))
                }
                _ => Err(ApiError::invalid("unexpected live request")),
            }
        }

        fn snapshot_project(&self, _path: &str) -> Result<Session, ApiError> {
            Ok(self.session.lock().expect("live").snapshot())
        }
    }

    #[test]
    fn live_mode_routes_edits_undo_preview_and_refuses_close_when_enabled() {
        let (mut api, scratch, _events) = api();
        let live_path = project(&mut api, &scratch, "Live");
        ok(api.dispatch(Request::ProjectClose {
            path: live_path.clone(),
            save: false,
        }));

        let open = OpenProjects::default();
        api.share_open_projects(open.clone());
        open.claim(&live_path, Holder::Window)
            .expect("window claims live project");
        let info = projects::open(&live_path).expect("open info");
        let live_session = Session::open_info(&info).expect("live session");

        let enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let busy = Arc::new(std::sync::atomic::AtomicBool::new(false));
        api.share_live_window(Arc::new(MockLiveWindow {
            enabled: Arc::clone(&enabled),
            busy: Arc::clone(&busy),
            session: Mutex::new(live_session),
        }));

        // Disabled by default: no capability and opening refuses.
        assert!(
            !api.version()
                .capabilities
                .iter()
                .any(|cap| cap == "liveWindow")
        );
        let refused = err(api.dispatch(Request::ProjectOpen {
            path: live_path.clone(),
        }));
        assert_eq!(refused.code, ErrorCode::Refused);
        assert!(refused.message.contains("Live Mode"));

        // Enable Live Mode: capability appears, edits hit the live session,
        // another project remains isolated in Api::sessions, and close refuses.
        enabled.store(true, std::sync::atomic::Ordering::Release);
        assert!(
            api.version()
                .capabilities
                .iter()
                .any(|cap| cap == "liveWindow")
        );
        let other_path = project(&mut api, &scratch, "Other");
        let initial = view(ok(api.dispatch(Request::ProjectOpen {
            path: live_path.clone(),
        })));
        assert!(!initial.can_undo);

        let edited = view(ok(api.dispatch(Request::EditApply {
            path: live_path.clone(),
            command: Box::new(Command::AddTextClip {
                above: false,
                track_id: None,
                start: 1.0,
                style: None,
                duration: None,
                offset_y: None,
            }),
        })));
        assert!(edited.can_undo);
        assert!(edited.created_id.is_some());

        let undone = view(ok(api.dispatch(Request::EditUndo {
            path: live_path.clone(),
        })));
        assert!(!undone.can_undo);
        assert!(undone.can_redo);

        let redone = view(ok(api.dispatch(Request::EditRedo {
            path: live_path.clone(),
        })));
        assert!(redone.can_undo);

        ok(api.dispatch(Request::ProjectSave {
            path: live_path.clone(),
            name: None,
        }));
        match ok(api.dispatch(Request::PreviewFrame {
            path: live_path.clone(),
            time: 1.5,
            output: None,
            width: Some(16),
            height: Some(9),
        })) {
            Reply::Picture(pic) => assert!(!pic.png.is_empty()),
            _ => panic!("expected inline picture"),
        }

        // Cannot close the window's active project over Live Mode, while Other closes normally.
        let close_live = err(api.dispatch(Request::ProjectClose {
            path: live_path.clone(),
            save: false,
        }));
        assert_eq!(close_live.code, ErrorCode::Refused);
        ok(api.dispatch(Request::ProjectClose {
            path: other_path,
            save: false,
        }));

        // Busy guard returns ErrorCode::Busy.
        busy.store(true, std::sync::atomic::Ordering::Release);
        let busy_err = err(api.dispatch(Request::EditUndo { path: live_path }));
        assert_eq!(busy_err.code, ErrorCode::Busy);
    }

    #[test]
    fn capcut_export_and_import_respect_roots_and_round_trip() {
        let (mut api, scratch, _events) = api();
        let inside = scratch.path().join("allowed");
        let outside = scratch.path().join("outside");
        std::fs::create_dir_all(&inside).expect("inside");
        std::fs::create_dir_all(&outside).expect("outside");
        api.restrict_writes_to(vec![inside.clone()]);

        let inside_str = inside.to_string_lossy().into_owned();
        let proj = api
            .create(&inside_str, "SourceCap", VideoSettings::default())
            .expect("create");
        let _ = proj;
        let proj_path = inside.join("SourceCap").to_string_lossy().into_owned();
        ok(api.dispatch(Request::EditApply {
            path: proj_path.clone(),
            command: Box::new(Command::AddTextClip {
                above: false,
                track_id: None,
                start: 0.5,
                style: None,
                duration: Some(1.5),
                offset_y: None,
            }),
        }));

        // Writing outside confined roots is refused.
        let outside_err = err(api.dispatch(Request::CapCutExport {
            path: proj_path.clone(),
            output_dir: Some(outside.to_string_lossy().into_owned()),
            text_options: None,
        }));
        assert_eq!(outside_err.code, ErrorCode::Refused);

        // Exporting inside roots succeeds.
        let drafts_dir = inside.join("CapCutOut");
        let Reply::CapCutExport(exported) = ok(api.dispatch(Request::CapCutExport {
            path: proj_path,
            output_dir: Some(drafts_dir.to_string_lossy().into_owned()),
            text_options: None,
        })) else {
            panic!("expected CapCutExport reply");
        };
        assert!(
            Path::new(&exported.draft_dir)
                .join("draft_content.json")
                .is_file()
        );

        // Importing outside confined roots is refused, inside succeeds and opens.
        let import_outside = err(api.dispatch(Request::CapCutImport {
            input_path: exported.draft_dir.clone(),
            location: outside.to_string_lossy().into_owned(),
            name: Some("Reimported".to_owned()),
        }));
        assert_eq!(import_outside.code, ErrorCode::Refused);

        let reimported = view(ok(api.dispatch(Request::CapCutImport {
            input_path: exported.draft_dir,
            location: inside_str,
            name: Some("Reimported".to_owned()),
        })));
        assert_eq!(reimported.settings.name, "Reimported");
        assert_eq!(reimported.project.active().clips.len(), 1);
    }

    #[test]
    fn link_probe_and_download_validate_open_project_and_inputs() {
        let (mut api, scratch, _events) = api();
        let bad_probe = err(api.dispatch(Request::LinkProbe {
            links: vec!["ftp://invalid.local/file".to_owned()],
            use_drive: false,
            cookies_path: None,
        }));
        assert_eq!(bad_probe.code, ErrorCode::Failed);

        let not_open_dl = err(api.dispatch(Request::LinkDownload {
            path: scratch.path().join("Nope").to_string_lossy().into_owned(),
            items: vec![LinkItem {
                title: "Clip".to_owned(),
                url: "https://example.com/v".to_owned(),
                drive_id: String::new(),
                public_drive: false,
                duration: 0.0,
                size: 0,
                thumbnail: String::new(),
                heights: Vec::new(),
            }],
            height: Some(720),
            audio_only: false,
            auto_import: Some(true),
            cookies_path: None,
        }));
        assert_eq!(not_open_dl.code, ErrorCode::NotOpen);
    }
}
