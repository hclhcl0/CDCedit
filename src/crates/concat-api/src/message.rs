// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

//! What crosses the API boundary: a [`Request`] in, a [`Response`] out,
//! and [`Event`]s along the way.
//!
//! All three are serde types with a fixed JSON shape, and that shape is the
//! contract: a transport is free to carry it over stdin, a socket or a
//! function call, but never to reinterpret it. Requests are tagged by
//! `method`, events by `event`, and a response is `{"result": ...}` or
//! `{"error": {"code": ..., "message": ...}}`. Fields are camelCase,
//! matching the document and the command layer, so a caller learns one
//! spelling. On a line or a socket the three travel inside the JSON-RPC 2.0
//! envelope [`crate::rpc`] describes, which adds an id to tie a response to
//! its request and nothing else.
//!
//! A method that takes longer than a caller should wait - an export - is a
//! job: the response names it at once, and everything that happens to it
//! comes back as [`Event`]s carrying that name, ending in `export.done` or
//! `export.failed`. A refusal is an [`ApiError`]: a [`ErrorCode`] a program
//! can branch on and the sentence a person would be shown.
//!
//! The edit vocabulary is not redefined here. [`Request::EditApply`] carries
//! a `concat_project` [`Command`] as it is, so every operation the window
//! can perform is one an API caller can perform, with the same clamps and
//! the same refusals, and a new command needs nothing added here.

use concat_host::export::Progress;
use concat_host::media::MediaSummary;
use concat_host::session::EditorView;
use concat_host::templates::TemplateInfo;
use concat_host::{AppDirs, ProjectInfo};
use concat_project::Command;
use concat_project::model::VideoSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The version of this contract. Bumped when a method's shape changes in a
/// way a caller written against the previous one would misread; adding a
/// method or an optional field does not bump it.
pub const API_VERSION: &str = "0.2";

/// What a caller asks for. One variant per method, named `area.verb`.
///
/// Every method that touches a project names it by its folder, the same
/// path [`Request::ProjectOpen`] took; the API keeps one session per folder
/// and a method on a folder that is not open is an error, not a silent
/// open, so a caller always knows what it is editing.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all_fields = "camelCase")]
pub enum Request {
    /// The API's version, the build behind it, and what that build
    /// serves. The one method to call first: its reply's `capabilities`
    /// names the transports and optional features this server has, so a
    /// caller knows what to ask for before it asks.
    #[serde(rename = "version")]
    Version,

    /// Creates a project folder under `location`, named `name`, and opens
    /// it. Refuses a folder that already holds a project.
    #[serde(rename = "project.create")]
    ProjectCreate {
        /// The directory the project folder is made in.
        location: String,
        /// The project's name; the folder is named after it.
        name: String,
        /// Frame and rate; 1080p at 30 when absent.
        #[serde(default)]
        video: Option<VideoSettings>,
    },
    /// Opens a project folder. Opening one already open returns its state
    /// as it stands, edits and all.
    #[serde(rename = "project.open")]
    ProjectOpen {
        /// The project folder.
        path: String,
    },
    /// Closes an open project, saving first when asked. Unsaved edits are
    /// dropped otherwise.
    #[serde(rename = "project.close")]
    ProjectClose {
        /// The project folder.
        path: String,
        /// Whether to write the document before closing.
        #[serde(default)]
        save: bool,
    },
    /// The projects this machine opened most recently, newest first.
    #[serde(rename = "project.list")]
    ProjectList,
    /// The state of an open project.
    #[serde(rename = "project.get")]
    ProjectGet {
        /// The project folder.
        path: String,
    },
    /// The document exactly as a save would write it.
    #[serde(rename = "project.document")]
    ProjectDocument {
        /// The project folder.
        path: String,
    },
    /// Writes the document to the project folder.
    #[serde(rename = "project.save")]
    ProjectSave {
        /// The project folder.
        path: String,
        /// A new name for the project, when renaming.
        #[serde(default)]
        name: Option<String>,
    },
    /// Sets the active timeline's frame and rate, as an undoable edit.
    #[serde(rename = "project.setVideo")]
    ProjectSetVideo {
        /// The project folder.
        path: String,
        /// The frame and rate.
        video: VideoSettings,
    },

    /// Applies one edit command. The command's own notes say what each
    /// does; a refusal comes back as the sentence the window would show.
    #[serde(rename = "edit.apply")]
    EditApply {
        /// The project folder.
        path: String,
        /// The edit. Boxed because a command is the largest thing a
        /// request carries; the JSON is the command as it is.
        command: Box<Command>,
    },
    /// Steps the history back one edit.
    #[serde(rename = "edit.undo")]
    EditUndo {
        /// The project folder.
        path: String,
    },
    /// Steps the history forward one edit.
    #[serde(rename = "edit.redo")]
    EditRedo {
        /// The project folder.
        path: String,
    },

    /// Reports what is inside a media file without touching any project.
    #[serde(rename = "media.probe")]
    MediaProbe {
        /// The file.
        path: String,
    },
    /// Probes a file and adds it to a project's bin: what dropping a file
    /// on the window does. A path already in the bin is a no-op.
    #[serde(rename = "media.import")]
    MediaImport {
        /// The project folder.
        path: String,
        /// The file to import.
        file: String,
    },

    /// Every effect package the build knows, with its parameters, so a
    /// caller can build a valid chain without reading a manifest.
    #[serde(rename = "catalogue.list")]
    CatalogueList {
        /// Restrict to one kind: "effect", "filter", "audio", "transition"
        /// or "generator".
        #[serde(default)]
        kind: Option<String>,
    },

    /// The template library.
    #[serde(rename = "template.list")]
    TemplateList,
    /// Makes a project from a template with every slot filled, and opens
    /// it. Every slot must be filled; a set that leaves one empty makes
    /// nothing.
    #[serde(rename = "template.instantiate")]
    TemplateInstantiate {
        /// The template bundle folder.
        template: String,
        /// The directory the project folder is made in.
        location: String,
        /// The project's name.
        name: String,
        /// The file for each slot.
        fills: Vec<Fill>,
    },
    /// Packs an open project into a new template bundle.
    #[serde(rename = "template.save")]
    TemplateSave {
        /// The project folder.
        path: String,
        /// The template's name.
        name: String,
    },

    /// Renders an open project's active timeline to a file, exactly as
    /// the window's Export does: cutouts analysed, stabilized clips'
    /// camera shake measured, titles painted, then the
    /// frame loop and the mix. Returns at once with the job's name; the
    /// render runs on its own thread and reports through [`Event`]s, ending
    /// in [`Event::ExportDone`] or [`Event::ExportFailed`]. One export runs
    /// at a time; a second is refused as [`ErrorCode::Busy`].
    #[serde(rename = "export.run")]
    ExportRun {
        /// The project folder.
        path: String,
        /// How to render it.
        #[serde(flatten)]
        spec: ExportSpec,
    },
    /// Stops a running export at its next frame. The job then ends with
    /// [`Event::ExportFailed`] carrying [`ErrorCode::Cancelled`].
    #[serde(rename = "export.cancel")]
    ExportCancel {
        /// The job [`Started`] named.
        job: String,
    },
    /// How a job stands: running and how far along, or how it ended. The
    /// same news the events carry, for a caller that was not listening
    /// when it came; the last few finished jobs are remembered.
    #[serde(rename = "export.status")]
    ExportStatus {
        /// The job [`Started`] named.
        job: String,
    },

    /// Composites the true frame at one instant. Written as a PNG to
    /// `output` when there is one, and handed back inline as a
    /// [`Picture`] otherwise, for a caller on the far side of a socket.
    #[serde(rename = "preview.frame")]
    PreviewFrame {
        /// The project folder.
        path: String,
        /// The timeline instant, in seconds.
        time: f64,
        /// The file to write; absent means the picture comes back inline.
        #[serde(default)]
        output: Option<String>,
        /// Frame width; the timeline's when absent.
        #[serde(default)]
        width: Option<u32>,
        /// Frame height; the timeline's when absent.
        #[serde(default)]
        height: Option<u32>,
    },

    /// Current status of any background job (export, link download, AI speech, or Flow).
    #[serde(rename = "job.status")]
    JobStatus {
        /// The job [`Started`] named.
        job: String,
    },
    /// Cancels a running background job.
    #[serde(rename = "job.cancel")]
    JobCancel {
        /// The job [`Started`] named.
        job: String,
    },

    /// Inspects video or Google Drive links and lists downloadable items without downloading.
    #[serde(rename = "link.probe")]
    LinkProbe {
        /// Video page URLs or Google Drive file/folder URLs (1 to 50).
        links: Vec<String>,
        /// True to use saved Google Drive OAuth credentials for private Drive links.
        #[serde(default)]
        use_drive: bool,
        /// Optional Netscape `cookies.txt` path for authenticated video sites.
        #[serde(default)]
        cookies_path: Option<String>,
    },
    /// Downloads items returned by `link.probe` into an open project's `media/downloads/`
    /// folder on a background thread and optionally probes/adds them to the media bin.
    #[serde(rename = "link.download")]
    LinkDownload {
        /// The open project folder where downloaded files land under `media/downloads/`.
        path: String,
        /// Items to download (typically from `link.probe`).
        items: Vec<LinkItem>,
        /// Maximum video height in pixels; `0` selects best available (`1080` when absent).
        #[serde(default)]
        height: Option<u32>,
        /// Download audio only as `.m4a` instead of video.
        #[serde(default)]
        audio_only: bool,
        /// Probe downloaded media and add it to the project bin automatically (`true` when absent).
        #[serde(default)]
        auto_import: Option<bool>,
        /// Optional Netscape `cookies.txt` path for authenticated video sites.
        #[serde(default)]
        cookies_path: Option<String>,
    },

    /// Imports a CapCut PC draft (`draft_content.json`, `draft_info.json`, draft folder,
    /// or `.zip`) as a new CDCAi project under `location`, opens it, and returns its view.
    #[serde(rename = "capcut.import")]
    CapCutImport {
        /// Path to the CapCut draft folder, `.json`, or `.zip` file.
        input_path: String,
        /// Parent directory where the new CDCAi project folder will be created.
        location: String,
        /// Optional project name; defaults to `<draft>_imported`.
        #[serde(default)]
        name: Option<String>,
    },
    /// Exports an open CDCAi project's active timeline into a CapCut PC draft folder.
    #[serde(rename = "capcut.export")]
    CapCutExport {
        /// The open project folder.
        path: String,
        /// Optional destination folder; defaults to CapCut PC's local drafts folder.
        #[serde(default)]
        output_dir: Option<String>,
        /// Optional subtitle styling overrides for CapCut text segments.
        #[serde(default)]
        text_options: Option<CapCutTextSpec>,
    },

    /// Lists downloaded/available local Whisper and TTS models, voices, and whether
    /// Gemini / ElevenLabs API keys are configured on this machine (keys themselves are never returned).
    #[serde(rename = "models.speechStatus")]
    ModelsSpeechStatus,

    /// Runs speech-to-text transcription on a timeline clip or standalone media file
    /// using offline Whisper (`engine: "whisper"`) or Gemini (`engine: "gemini"`), optionally
    /// adding caption titles above the timeline (`autoApply` defaults to `true`).
    #[serde(rename = "captions.transcribe")]
    CaptionsTranscribe {
        /// The open project folder.
        path: String,
        /// Timeline clip id whose audio window will be transcribed (mutually exclusive with `mediaPath`).
        #[serde(default)]
        clip_id: Option<String>,
        /// Standalone audio/video file path to transcribe when `clipId` is omitted.
        #[serde(default)]
        media_path: Option<String>,
        /// Transcription engine: `"whisper"` (offline, default) or `"gemini"` (cloud API key required).
        #[serde(default)]
        engine: Option<String>,
        /// Whisper model id (e.g. `"base"`, `"small"`); defaults to the first installed model.
        #[serde(default)]
        model: Option<String>,
        /// Language code hint (e.g. `"vi"`, `"en"`, `"auto"`); defaults to `"vi"`.
        #[serde(default)]
        language: Option<String>,
        /// Enable two-speaker diarization `[A]` / `[B]` (Gemini engine).
        #[serde(default)]
        two_speakers: bool,
        /// Smart dubbing sentence segmentation after Gemini transcription.
        #[serde(default)]
        smart_split: bool,
        /// Optional domain glossary / proper nouns hint for higher accuracy.
        #[serde(default)]
        glossary: Option<String>,
        /// Place generated captions as text clips on the timeline when finished (`true` by default).
        #[serde(default)]
        auto_apply: Option<bool>,
        /// Vertical caption placement as a fraction of frame height (`0.35` by default).
        #[serde(default)]
        offset_y: Option<f64>,
        /// Estimate duration and validate credentials without running cloud transcription.
        #[serde(default)]
        dry_run: bool,
    },

    /// Contextual AI translation for subtitle lines or a timeline text track using Gemini.
    #[serde(rename = "captions.translate")]
    CaptionsTranslate {
        /// Optional open project folder (required when reading from or applying to `trackId`).
        #[serde(default)]
        path: Option<String>,
        /// Text track id on the active timeline to translate when `lines` is omitted.
        #[serde(default)]
        track_id: Option<String>,
        /// Explicit timed subtitle lines to translate; when omitted, reads text clips from `path`.
        #[serde(default)]
        lines: Option<Vec<CaptionLineSpec>>,
        /// Target translation language name (e.g. `"Vietnamese"`, `"English"`, `"Japanese"`).
        target_language: String,
        /// Gemini model name (defaults to `"gemini-3.8-flash"`).
        #[serde(default)]
        model: Option<String>,
        /// Optional glossary of names, terms, and pronoun rules.
        #[serde(default)]
        glossary: Option<String>,
        /// Favor natural spoken phrasing suited to voiceover (`false` by default).
        #[serde(default)]
        natural: bool,
        /// Condense translated text to fit each subtitle's spoken slot duration.
        #[serde(default)]
        condense_to_fit: bool,
        /// Update matching text clips in the open project when translating a track (`true` when `path` given cleanly unless `false`).
        #[serde(default)]
        apply_to_track: Option<bool>,
        /// Count lines/characters and verify Gemini key configuration without billing.
        #[serde(default)]
        dry_run: bool,
    },

    /// Synthesizes narration or two-speaker A/B dialogue using local TTS (`"local"`),
    /// Gemini (`"gemini"`), or ElevenLabs (`"elevenlabs"`), importing and optionally placing the WAV.
    #[serde(rename = "speech.tts")]
    SpeechTts {
        /// The open project folder where generated audio lands under `audio/`.
        path: String,
        /// Single-speaker narration text (provide either `text` or `turns`).
        #[serde(default)]
        text: Option<String>,
        /// Two-speaker dialogue turns for Gemini multi-speaker mode.
        #[serde(default)]
        turns: Option<Vec<DialogueTurnSpec>>,
        /// Synthesis provider: `"local"` (offline Kokoro/Pocket/Chatterbox, default), `"gemini"`, or `"elevenlabs"`.
        #[serde(default)]
        provider: Option<String>,
        /// Model id (local bundle id, or cloud model name).
        #[serde(default)]
        model: Option<String>,
        /// Voice identifier (name or numeric id for local voices; Gemini prebuilt voice name or ElevenLabs voice id).
        voice_id: String,
        /// Optional second speaker voice id when `turns` has speaker `1`.
        #[serde(default)]
        voice_b_id: Option<String>,
        /// Optional delivery/style prompt for cloud synthesis.
        #[serde(default)]
        style: Option<String>,
        /// Speaking speed multiplier (`1.0` default).
        #[serde(default)]
        speed: Option<f32>,
        /// Silence scale between sentences for local sherpa-onnx engines (`0.0..=1.0`).
        #[serde(default)]
        pauses: Option<f32>,
        /// Sampling temperature for Gemini synthesis (`1.0` default).
        #[serde(default)]
        temperature: Option<f32>,
        /// Import and place generated clip onto the timeline automatically (`true` by default).
        #[serde(default)]
        place_on_timeline: Option<bool>,
        /// Timeline start position in seconds when placing (`0.0` by default).
        #[serde(default)]
        start: Option<f64>,
        /// Destination track id when placing on timeline (first free track when absent).
        #[serde(default)]
        track_id: Option<String>,
        /// Return character count and check readiness without running synthesis.
        #[serde(default)]
        dry_run: bool,
    },

    /// Time-aligned multi-cue AI dubbing: synthesizes each cue and time-stretches it cleanly
    /// into its timeline `[start, end]` slot, mixing a continuous dubbing WAV track.
    #[serde(rename = "speech.dub")]
    SpeechDub {
        /// The open project folder.
        path: String,
        /// Timed cues to synthesize; when omitted, reads text clips from `sourceTrackId` (or timeline titles).
        #[serde(default)]
        cues: Option<Vec<DubCueSpec>>,
        /// Timeline text track id to read cues from when `cues` is omitted.
        #[serde(default)]
        source_track_id: Option<String>,
        /// Synthesis provider: `"gemini"` (default), `"local"`, or `"elevenlabs"`.
        #[serde(default)]
        provider: Option<String>,
        /// Model id for the chosen provider.
        #[serde(default)]
        model: Option<String>,
        /// Primary voice id (`speaker: 0`).
        voice_a: String,
        /// Optional second voice id (`speaker: 1`).
        #[serde(default)]
        voice_b: Option<String>,
        /// Sampling temperature for Gemini (`1.0` default).
        #[serde(default)]
        temperature: Option<f32>,
        /// Import and place rendered dubbing track on the timeline (`true` by default).
        #[serde(default)]
        place_on_timeline: Option<bool>,
        /// Optional destination track id for the dubbed audio clip.
        #[serde(default)]
        target_track_id: Option<String>,
        /// Validate cues and report character/cue estimate without running synthesis.
        #[serde(default)]
        dry_run: bool,
    },

    /// Transforms an audio clip or media file using the 10 offline DSP studio presets (`"dsp"`)
    /// or cloud AI speech-to-speech (`"elevenlabs"` or `"gemini"`).
    #[serde(rename = "speech.voiceChange")]
    SpeechVoiceChange {
        /// The open project folder.
        path: String,
        /// Timeline clip id to transform (mutually exclusive with `sourcePath`).
        #[serde(default)]
        clip_id: Option<String>,
        /// Standalone audio/video path to transform when `clipId` is omitted.
        #[serde(default)]
        source_path: Option<String>,
        /// Conversion method: `"dsp"` (offline FFmpeg, default), `"elevenlabs"`, or `"gemini"`.
        #[serde(default)]
        method: Option<String>,
        /// Offline DSP preset `0..=9` (0 studio clean, 1 deep male, 2 bright female, 3 anonymous, 4 chipmunk, 5 giant, 6 robot, 7 radio, 8 hall echo, 9 custom).
        #[serde(default)]
        preset: Option<i32>,
        /// Custom pitch shift in semitones (`-12.0..=12.0`, overrides preset pitch when set).
        #[serde(default)]
        pitch_semitones: Option<f32>,
        /// Output speed multiplier (`0.5..=2.0`, `1.0` default).
        #[serde(default)]
        speed: Option<f32>,
        /// Apply noise reduction pre-filter before conversion.
        #[serde(default)]
        denoise: Option<bool>,
        /// Cloud S2S model id when using `"elevenlabs"` or `"gemini"`.
        #[serde(default)]
        model: Option<String>,
        /// Target voice id required for `"elevenlabs"` or `"gemini"` speech-to-speech.
        #[serde(default)]
        voice_id: Option<String>,
        /// Import and place transformed audio on timeline (`true` by default).
        #[serde(default)]
        place_on_timeline: Option<bool>,
        /// Timeline placement start in seconds (defaults to original clip start or `0.0`).
        #[serde(default)]
        start: Option<f64>,
        /// Destination track id for placement.
        #[serde(default)]
        track_id: Option<String>,
        /// Estimate duration and check cloud key readiness without running conversion.
        #[serde(default)]
        dry_run: bool,
    },

    /// Lists configured Google Flow Chrome CDP accounts (`google-flow/accounts.json`),
    /// concurrency limits, detected emails, active lock status, and unresolved receipts.
    #[serde(rename = "flow.accountsList")]
    FlowAccountsList,

    /// Connects (or launches for managed/custom profiles) Google Flow's Chrome session on the
    /// configured CDP port and checks whether the user is signed in to Flow.
    #[serde(rename = "flow.connect")]
    FlowConnect {
        /// Account id from `flow.accountsList` (defaults to the first enabled account / `"primary"`).
        #[serde(default)]
        account_id: Option<String>,
        /// Optional Google Flow URL to open instead of the Flow home page or account default URL.
        #[serde(default)]
        open_url: Option<String>,
    },

    /// Checks Google Flow browser connection, login state, current Flow project/scene URL,
    /// in-flight jobs, and unresolved receipts without launching a browser when none is running.
    #[serde(rename = "flow.status")]
    FlowStatus {
        /// Account id from `flow.accountsList` (defaults to the first enabled account / `"primary"`).
        #[serde(default)]
        account_id: Option<String>,
    },

    /// Generates 1–4 images in Google Flow (Nano Banana models) as a background job and downloads
    /// them into `google-flow/output/`, optionally importing into an open CDCAi project.
    /// Requires explicit `allowCredit: true` before spending Flow credits.
    #[serde(rename = "flow.generateImage")]
    FlowGenerateImage {
        /// Account id from `flow.accountsList` (defaults to the first enabled account).
        #[serde(default)]
        account_id: Option<String>,
        /// Scene or image description prompt (may be omitted when `resume: true`).
        #[serde(default)]
        prompt: String,
        /// Image model (`"nano-banana-pro"` default, `"nano-banana-2"`, or `"nano-banana-2-lite"`).
        #[serde(default)]
        model: Option<String>,
        /// Aspect ratio (`"1:1"` default, `"9:16"`, `"16:9"`, `"4:3"`, or `"3:4"`).
        #[serde(default)]
        ratio: Option<String>,
        /// Number of images to generate (`1..=4`, default `1`).
        #[serde(default)]
        count: Option<u32>,
        /// Optional local visual reference image paths.
        #[serde(default)]
        reference_images: Vec<String>,
        /// Google Flow project URL (falls back to the selected account's saved `projectUrl` if omitted).
        #[serde(default)]
        project_url: Option<String>,
        /// Submit generation immediately (`true` default) or prepare without sending (`false`, 0 credits).
        #[serde(default)]
        auto_confirm: Option<bool>,
        /// Resume waiting for / downloading an already-running Flow job without creating a new request.
        #[serde(default)]
        resume: bool,
        /// Worker Flow job id to resume when `resume: true`.
        #[serde(default)]
        job_id: Option<String>,
        /// Explicit confirmation to spend account credits (defaults to `false`; must be `true` when submitting).
        #[serde(default)]
        allow_credit: bool,
        /// Acknowledge existing `uncertain` receipts on this account after checking Flow manually.
        #[serde(default)]
        acknowledge_uncertain: bool,
        /// Optional open CDCAi project folder where downloaded images are auto-imported into the bin.
        #[serde(default)]
        project_path: Option<String>,
        /// Automatically import downloaded outputs into `projectPath` (`true` by default).
        #[serde(default)]
        auto_import: Option<bool>,
        /// Also place imported output onto the timeline when `projectPath` is given (`false` default).
        #[serde(default)]
        place_on_timeline: bool,
        /// Timeline start position in seconds when `placeOnTimeline` is `true` (`0.0` default).
        #[serde(default)]
        start: Option<f64>,
        /// Destination track id when `placeOnTimeline` is `true` (first free track when absent).
        #[serde(default)]
        track_id: Option<String>,
        /// Validate account, project URL, references, receipts, and credit gate without running Chrome.
        #[serde(default)]
        dry_run: bool,
    },

    /// Generates a video in Google Flow (Veo 3.1 / Omni Flash) as a background job and downloads
    /// `.mp4` output into `google-flow/output/`, optionally importing into an open CDCAi project.
    /// Requires explicit `allowCredit: true` before spending Flow credits and never auto-retries on error.
    #[serde(rename = "flow.generateVideo")]
    FlowGenerateVideo {
        /// Account id from `flow.accountsList` (defaults to the first enabled account).
        #[serde(default)]
        account_id: Option<String>,
        /// Video scene prompt (may be omitted when `resume: true`).
        #[serde(default)]
        prompt: String,
        /// Video model (`"veo-3.1-fast"` default, `"veo-3.1-lite"`, `"veo-3.1-quality"`, `"omni-flash"`, `"omni-flash-1.2"`).
        #[serde(default)]
        model: Option<String>,
        /// Aspect ratio (`"9:16"` default or `"16:9"`).
        #[serde(default)]
        ratio: Option<String>,
        /// Duration in seconds (`8` default; `omni-flash-1.2`: `3..=15`, `omni-flash`: `4|6|8|10`, Veo: `4|6|8`).
        #[serde(default)]
        duration: Option<u32>,
        /// Optional local first-frame / reference image paths.
        #[serde(default)]
        reference_images: Vec<String>,
        /// Google Flow project URL (falls back to the selected account's saved `projectUrl` if omitted).
        #[serde(default)]
        project_url: Option<String>,
        /// Submit generation immediately (`true` default) or prepare without sending (`false`, 0 credits).
        #[serde(default)]
        auto_confirm: Option<bool>,
        /// Resume waiting for / downloading an in-flight Flow job without spending new credits.
        #[serde(default)]
        resume: bool,
        /// Worker Flow job id to resume when `resume: true`.
        #[serde(default)]
        job_id: Option<String>,
        /// Edit the last generated video of the current Flow session instead of creating a new one.
        #[serde(default)]
        edit_mode: bool,
        /// Use direct in-browser Flow API when available (`true` default).
        #[serde(default)]
        use_direct_api: Option<bool>,
        /// Explicit confirmation to spend account credits (defaults to `false`; must be `true` when submitting).
        #[serde(default)]
        allow_credit: bool,
        /// Acknowledge existing `uncertain` receipts on this account after checking Flow manually.
        #[serde(default)]
        acknowledge_uncertain: bool,
        /// Optional open CDCAi project folder where downloaded videos are auto-imported into the bin.
        #[serde(default)]
        project_path: Option<String>,
        /// Automatically import downloaded outputs into `projectPath` (`true` by default).
        #[serde(default)]
        auto_import: Option<bool>,
        /// Also place imported clip onto the timeline when `projectPath` is given (`false` default).
        #[serde(default)]
        place_on_timeline: bool,
        /// Timeline start position in seconds when `placeOnTimeline` is `true` (`0.0` default).
        #[serde(default)]
        start: Option<f64>,
        /// Destination track id when `placeOnTimeline` is `true` (first free track when absent).
        #[serde(default)]
        track_id: Option<String>,
        /// Validate account, project URL, duration, receipts, and credit gate without running Chrome.
        #[serde(default)]
        dry_run: bool,
    },

    /// Lists verified downloaded outputs on disk and any unresolved (`running` / `uncertain`)
    /// Google Flow job receipts without spawning Chrome or spending credits.
    #[serde(rename = "flow.recentOutputs")]
    FlowRecentOutputs {
        /// Optional account id to inspect (defaults to inspecting all configured accounts).
        #[serde(default)]
        account_id: Option<String>,
    },
}

/// One downloadable link entry discovered by [`Request::LinkProbe`].
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkItem {
    /// Display title.
    pub title: String,
    /// Original URL.
    pub url: String,
    /// Drive file id for authenticated Drive transfers; empty for public video URLs.
    #[serde(default)]
    pub drive_id: String,
    /// True for anonymous public Google Drive downloads.
    #[serde(default)]
    pub public_drive: bool,
    /// Duration in seconds when known.
    #[serde(default)]
    pub duration: f64,
    /// Estimated file size in bytes; `0` when unknown.
    #[serde(default)]
    pub size: u64,
    /// Optional local cached thumbnail path.
    #[serde(default)]
    pub thumbnail: String,
    /// Available video heights returned during probing.
    #[serde(default)]
    pub heights: Vec<u32>,
}

/// [`Request::LinkProbe`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkProbeInfo {
    /// Downloadable items discovered for the requested URLs.
    pub items: Vec<LinkItem>,
    /// Whether yt-dlp and ffmpeg helpers are installed on this machine.
    pub tools_ready: bool,
    /// Whether Google Drive OAuth remote is configured on this machine.
    pub drive_connected: bool,
}

/// Subtitle styling overrides when exporting a CapCut PC draft.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CapCutTextSpec {
    /// Text font size on CapCut canvas (`1..=100`; `5.0` by default).
    pub font_size: Option<f64>,
    /// Scale percentage (`83.0` by default).
    pub scale_percent: Option<f64>,
    /// Vertical offset in CapCut pixels (`-870.0` by default).
    pub position_y: Option<f64>,
    /// Enable text stroke outline (`true` by default).
    pub stroke_enabled: Option<bool>,
    /// Outline colour in `#rrggbb` (`"#000000"` by default).
    pub stroke_color: Option<String>,
    /// Outline thickness percentage (`25.0` by default).
    pub stroke_width: Option<f64>,
}

/// [`Request::CapCutExport`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapCutExportInfo {
    /// Generated CapCut draft folder path containing `draft_content.json`.
    pub draft_dir: String,
    /// True when written directly into CapCut PC's local drafts folder.
    pub in_capcut_app: bool,
    /// True if clips have unbaked CDCAi effects/cutouts/masks.
    pub has_unbaked_effects: bool,
}

/// One timed caption row supplied to [`Request::CaptionsTranslate`].
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionLineSpec {
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Source subtitle text.
    pub text: String,
    /// Optional speaker index (`0` or `1`).
    #[serde(default)]
    pub speaker: usize,
}

/// One speaker turn supplied to [`Request::SpeechTts`].
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueTurnSpec {
    /// Speaker index (`0` for primary voice, `1` for secondary voice).
    #[serde(default)]
    pub speaker: usize,
    /// Spoken line.
    pub text: String,
    /// Optional delivery / mood instruction.
    #[serde(default)]
    pub style: String,
}

/// One timed dubbing cue supplied to [`Request::SpeechDub`].
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DubCueSpec {
    /// Text to synthesize into this slot.
    pub text: String,
    /// Slot start on the timeline in seconds.
    pub start: f64,
    /// Slot end on the timeline in seconds.
    pub end: f64,
    /// Speaker index (`0` or `1`).
    #[serde(default)]
    pub speaker: usize,
    /// Optional delivery / mood instruction.
    #[serde(default)]
    pub style: String,
}

/// Transcriber model entry in [`SpeechStatusInfo`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechTranscriberInfo {
    /// Model id (`"base"`, `"small"`, ...).
    pub id: String,
    /// Display name.
    pub label: String,
    /// Model trade-off description.
    pub blurb: String,
    /// True if model supports English only.
    pub english_only: bool,
    /// Approximate size in bytes.
    pub size_bytes: u64,
    /// True if model file is present on disk.
    pub downloaded: bool,
}

/// Local TTS model bundle entry in [`SpeechStatusInfo`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechTtsModelInfo {
    /// Bundle id.
    pub id: String,
    /// Engine family (`"kokoro"`, `"pocket"`, or `"chatterbox"`).
    pub family: String,
    /// Display name.
    pub label: String,
    /// Description.
    pub blurb: String,
    /// Approximate size in bytes.
    pub size_bytes: u64,
    /// True if bundle is unpacked on disk.
    pub downloaded: bool,
}

/// Built-in local TTS voice entry in [`SpeechStatusInfo`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechVoiceInfo {
    /// Numeric voice id.
    pub id: i32,
    /// Voice name (`"af_heart"`, `"pocket_alba"`, ...).
    pub name: String,
    /// Engine family (`"kokoro"`, `"pocket"`, or `"chatterbox"`).
    pub family: String,
}

/// [`Request::ModelsSpeechStatus`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatusInfo {
    /// Local Whisper models directory.
    pub transcribers_dir: String,
    /// Available Whisper / PhoWhisper models.
    pub transcribers: Vec<SpeechTranscriberInfo>,
    /// Local TTS models directory.
    pub tts_models_dir: String,
    /// Available local TTS bundles.
    pub tts_models: Vec<SpeechTtsModelInfo>,
    /// Available local voices.
    pub voices: Vec<SpeechVoiceInfo>,
    /// Whether one or more Gemini API keys are configured (keys themselves are never returned).
    pub gemini_configured: bool,
    /// Number of configured Gemini API keys available for rotation.
    pub gemini_key_count: usize,
    /// Whether one or more ElevenLabs API keys are configured.
    pub eleven_configured: bool,
    /// Number of configured ElevenLabs API keys available for rotation.
    pub eleven_key_count: usize,
}

/// Pre-flight estimate returned when `dryRun: true` on AI speech / caption / translation requests.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunInfo {
    /// Always `true`.
    pub dry_run: bool,
    /// Target operation (`"captions.transcribe"`, `"captions.translate"`, `"speech.tts"`, ...).
    pub operation: String,
    /// Provider or engine selected.
    pub provider: String,
    /// Model identifier that would run.
    pub model: String,
    /// Total input text characters (when text-based).
    pub estimated_characters: usize,
    /// Total audio duration seconds (when audio-based).
    pub estimated_seconds: f64,
    /// Whether required local model or cloud API key is configured.
    pub ready: bool,
    /// Human-readable readiness note or cost reminder.
    pub note: String,
}

/// Summary of one configured Google Flow account in [`FlowAccountsInfo`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowAccountInfo {
    /// Profile identifier (`"primary"`, `"account-..."`, ...).
    pub id: String,
    /// Display name shown in Settings.
    pub name: String,
    /// Whether this account is enabled for generation.
    pub enabled: bool,
    /// Connection mode (`"managed"`, `"attachOnly"`, or `"customProfile"`).
    pub mode: String,
    /// Local Chrome CDP port.
    pub port: u16,
    /// Chrome profile subdirectory (`"Default"`, `"Profile 1"`, ...).
    pub profile_directory: String,
    /// Saved default Flow project URL, if configured.
    pub project_url: String,
    /// Detected Chrome signed-in email or profile label, when readable from `Local State`.
    pub detected_email: Option<String>,
    /// True if a Flow worker or generation slot is currently active on this account.
    pub busy: bool,
    /// Number of unresolved (`running` or `uncertain`) receipts in this account's state directory.
    pub unresolved_receipts: usize,
}

/// [`Request::FlowAccountsList`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowAccountsInfo {
    /// Data directory storing Google Flow profiles, receipts, and generated outputs.
    pub root_dir: String,
    /// True when `accounts.json` passed validation.
    pub config_valid: bool,
    /// Validation or configuration notice, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_error: Option<String>,
    /// Configured max parallel accounts (`1..=4`).
    pub parallel_accounts: usize,
    /// Configured max simultaneous generation slots (`1..=4`).
    pub total_slots: usize,
    /// Effective concurrency limit `min(parallelAccounts, totalSlots)`.
    pub effective_limit: usize,
    /// Configured Flow accounts.
    pub accounts: Vec<FlowAccountInfo>,
}

/// [`Request::FlowConnect`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowConnectionInfo {
    /// Selected account id.
    pub account_id: String,
    /// True if attached to an already-running Chrome on the CDP port.
    pub attached_existing: bool,
    /// CDP debugging port.
    pub cdp_port: u16,
    /// Current Flow browser tab URL.
    pub tab_url: String,
    /// Whether the Chrome profile appears signed in to Google Flow.
    pub logged_in: bool,
    /// Manual sign-in or Chrome launch hint when not yet signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Summary of one Flow job receipt on disk (`receipts/*.json`).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowReceiptInfo {
    /// Request identifier.
    pub id: String,
    /// Receipt lifecycle phase (`"running"`, `"finished"`, `"failed"`, or `"uncertain"`).
    pub phase: String,
    /// Tool name (`"flow_generate_video"`, `"flow_generate_image"`, ...).
    pub tool: String,
    /// ISO-8601 last updated timestamp.
    pub updated: String,
    /// Model name if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Recorded error text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Verified existing local output paths attached to this receipt.
    pub files: Vec<String>,
}

/// [`Request::FlowStatus`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStatusInfo {
    /// Selected account id.
    pub account_id: String,
    /// Configured CDP port.
    pub cdp_port: u16,
    /// True if this account is currently executing another Flow operation.
    pub busy: bool,
    /// Whether Chrome CDP was reachable without launching a browser.
    pub connected: bool,
    /// Whether Google Flow login is detected (`None` when busy/offline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_in: Option<bool>,
    /// Current Flow tab URL, if connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Current Flow project UUID parsed from the tab URL, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Current Flow scene id parsed from the tab URL, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_id: Option<String>,
    /// Tool name currently holding the worker lock, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_tool: Option<String>,
    /// In-flight jobs reported by the Flow worker.
    pub jobs_in_flight: Vec<Value>,
    /// Unresolved (`running` / `uncertain`) receipts stored locally for this account.
    pub unresolved_receipts: Vec<FlowReceiptInfo>,
    /// Optional status note (for example, why live Chrome polling was skipped while busy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Per-account output and unresolved-receipt summary inside [`FlowRecentInfo`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowAccountRecentInfo {
    /// Account identifier.
    pub account_id: String,
    /// State directory path scanned.
    pub state_dir: String,
    /// Verified existing generated media paths on disk.
    pub files: Vec<String>,
    /// Unresolved (`running` or `uncertain`) receipts requiring inspection before retry.
    pub unresolved: Vec<FlowReceiptInfo>,
}

/// [`Request::FlowRecentOutputs`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRecentInfo {
    /// Root `google-flow` directory.
    pub root_dir: String,
    /// Total verified existing generated files across inspected accounts.
    pub total_files: usize,
    /// Total unresolved (`running` or `uncertain`) receipts across inspected accounts.
    pub total_unresolved: usize,
    /// Per-account breakdown.
    pub accounts: Vec<FlowAccountRecentInfo>,
}

/// One template slot and the file that takes it.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    /// The placeholder's media id in the template.
    pub media_id: String,
    /// The file to put there; probed on the way in.
    pub file: String,
}

/// How an export is rendered. Everything but the output is optional and
/// defaults to what the window's sheet defaults to.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSpec {
    /// The file to write.
    pub output: String,
    /// Constant rate factor; lower is better and bigger. 20 when absent.
    #[serde(default)]
    pub crf: Option<u8>,
    /// The x264 preset name. "medium" when absent.
    #[serde(default)]
    pub preset: Option<String>,
    /// Output width; the timeline's when absent.
    #[serde(default)]
    pub width: Option<u32>,
    /// Output height; the timeline's when absent.
    #[serde(default)]
    pub height: Option<u32>,
    /// Frame rate numerator; the timeline's when absent.
    #[serde(default)]
    pub rate_num: Option<i64>,
    /// Frame rate denominator; the timeline's when absent.
    #[serde(default)]
    pub rate_den: Option<i64>,
    /// "h264", "hevc" or "av1". H.264 when absent - HEVC for an HDR file,
    /// which H.264 does not carry.
    #[serde(default)]
    pub codec: Option<String>,
    /// Ten bits a channel. Eight when absent.
    #[serde(default)]
    pub ten_bit: Option<bool>,
    /// "limited" (16-235, what every player and YouTube expect) or "full"
    /// (0-255, for screen content bound for a PC player that reads the
    /// tag). Limited when absent.
    /// https://github.com/jub0t/Concat/issues/103
    #[serde(default)]
    pub color_range: Option<String>,
    /// For an HDR timeline: written as HDR, its own HLG or PQ in ten bits
    /// (true, the default), or tone-mapped to SDR (false). Nothing for an
    /// SDR timeline.
    #[serde(default)]
    pub hdr: Option<bool>,
}

/// What a request hands back. Serialised as the payload alone: the variant
/// is implied by the method, so a caller that sent `project.get` reads an
/// editor view and nothing wraps it.
#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum Reply {
    /// [`Request::Version`].
    Version(VersionInfo),
    /// The project after a change, or as it stands.
    View(Box<EditorView>),
    /// [`Request::ProjectList`].
    Projects(Vec<ProjectInfo>),
    /// [`Request::ProjectDocument`].
    Document(Value),
    /// [`Request::MediaProbe`].
    Media(MediaSummary),
    /// [`Request::CatalogueList`].
    Packages(Vec<PackageInfo>),
    /// [`Request::TemplateList`].
    Templates(Vec<TemplateInfo>),
    /// [`Request::TemplateSave`].
    Template(TemplateInfo),
    /// [`Request::ExportRun`]: the job that is now running.
    Started(Started),
    /// [`Request::ExportStatus`].
    Job(JobStatus),
    /// [`Request::PreviewFrame`] with an output: the file written.
    Written(Written),
    /// [`Request::PreviewFrame`] without one: the picture itself.
    Picture(Picture),
    /// [`Request::LinkProbe`].
    LinkItems(LinkProbeInfo),
    /// [`Request::CapCutExport`].
    CapCutExport(CapCutExportInfo),
    /// [`Request::ModelsSpeechStatus`].
    SpeechStatus(SpeechStatusInfo),
    /// [`Request::FlowAccountsList`].
    FlowAccounts(FlowAccountsInfo),
    /// [`Request::FlowConnect`].
    FlowConnection(FlowConnectionInfo),
    /// [`Request::FlowStatus`].
    FlowStatus(FlowStatusInfo),
    /// [`Request::FlowRecentOutputs`].
    FlowRecent(FlowRecentInfo),
    /// Pre-flight dry-run response.
    DryRun(DryRunInfo),
    /// Methods with nothing to say beyond having worked.
    Done(Done),
}

/// The build and the contract it speaks.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    /// [`API_VERSION`].
    pub api_version: String,
    /// The CDCAi build.
    pub concat: String,
    /// Where this machine keeps recents, templates and models.
    pub dirs: Dirs,
    /// What this build serves, as names a caller tests for membership:
    /// what the API itself provides in every build, `events`, and what
    /// only some builds do, `gpu` when frames composite on one; then what
    /// the transport around it adds, `json-rpc`, `unix-socket` and `grpc`
    /// for each of the socket transports listening. A name absent from
    /// the list is not served, whatever the build; a name added later is
    /// not a version bump, so a caller ignores names it does not know.
    pub capabilities: Vec<String>,
}

/// The app's directories, as paths.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dirs {
    /// Small state: recents, settings, the template library.
    pub config: String,
    /// Large state: downloaded models, painted titles.
    pub data: String,
}

impl From<&AppDirs> for Dirs {
    fn from(dirs: &AppDirs) -> Self {
        Dirs {
            config: dirs.config.to_string_lossy().into_owned(),
            data: dirs.data.to_string_lossy().into_owned(),
        }
    }
}

/// A file the API wrote.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// The file's path.
    pub path: String,
    /// Its picture size.
    pub width: u32,
    /// Its picture size.
    pub height: u32,
}

/// A job the API has begun. Every event about it carries `job`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// The job's name, unique for the life of the API: "j1", "j2", ...
    pub job: String,
    /// The project it works on.
    pub path: String,
    /// The file it will write.
    pub output: String,
}

/// Where a job is in its life.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    /// Still working.
    Running,
    /// Wrote its file.
    Done,
    /// Stopped by [`Request::ExportCancel`].
    Cancelled,
    /// Ended without its file; `error` says why.
    Failed,
}

/// How one job stands: [`Request::JobStatus`] / [`Request::ExportStatus`]'s reply.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    /// The job's name.
    pub job: String,
    /// Kind of background job (`"export"`, `"link.download"`, `"speech.tts"`,
    /// `"captions.transcribe"`, `"flow.generate"`, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The project it works on.
    pub path: String,
    /// The file it writes.
    pub output: String,
    /// Running, or how it ended.
    pub state: JobState,
    /// What it is doing: "cutout" while masks are found, "stabilize" while
    /// shake is measured, then "rendering", "mixing audio" and "muxing";
    /// "rendering in software" when the hardware encoder stopped part way
    /// and the video is rendered again from its first frame on the CPU.
    /// Absent until it has said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Frames rendered, in the video stage.
    pub frame: i64,
    /// Frames in all, in the video stage; 0 until known.
    pub total: i64,
    /// How far along the current stage is, `0..=1`; 1 once done.
    pub fraction: f64,
    /// The written file's picture size, once done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The written file's picture size, once done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Structured JSON output once done for non-export background jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Why it failed, or that it was cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

impl JobStatus {
    /// An export job that has just begun.
    pub fn running(job: &str, path: &str, output: &str) -> JobStatus {
        Self::running_kind(job, "export", path, output)
    }

    /// A typed background job that has just begun.
    pub fn running_kind(job: &str, kind: &str, path: &str, output: &str) -> JobStatus {
        JobStatus {
            job: job.to_owned(),
            kind: Some(kind.to_owned()),
            path: path.to_owned(),
            output: output.to_owned(),
            state: JobState::Running,
            stage: None,
            frame: 0,
            total: 0,
            fraction: 0.0,
            width: None,
            height: None,
            result: None,
            error: None,
        }
    }
}

/// A frame handed back without touching the disk.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    /// Its picture size.
    pub width: u32,
    /// Its picture size.
    pub height: u32,
    /// The PNG, base64 in the standard alphabet with padding.
    pub png: String,
}

/// The empty reply, an object so every reply is one.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Done {}

/// One effect package, as a caller building a chain needs it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    /// The id a clip's chain stores, `author.name`.
    pub id: String,
    /// What the catalogue card says.
    pub name: String,
    /// "effect", "filter", "audio", "transition" or "generator".
    pub kind: String,
    /// The shelf the card sits on.
    pub category: String,
    /// A sentence about it.
    pub description: String,
    /// The parameter the simple view shows as the one slider, if any.
    pub intensity: Option<String>,
    /// The knobs, in the order the inspector shows them.
    pub params: Vec<ParamInfo>,
}

/// One parameter of a package.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamInfo {
    /// The key a chain entry's `params` stores it under.
    pub key: String,
    /// What the control is labelled.
    pub label: String,
    /// "float", "int", "bool", "enum", "color" or "point".
    #[serde(rename = "type")]
    pub kind: String,
    /// Lowest value.
    pub min: f64,
    /// Highest value.
    pub max: f64,
    /// The value an untouched control means.
    pub default: f64,
    /// Slider increment; 0 means continuous.
    pub step: f64,
    /// Displayed after the number.
    pub unit: String,
    /// Whether the control can carry keyframes.
    pub animate: bool,
    /// For "enum": the values the document may hold.
    pub values: Vec<f64>,
    /// For "enum": what each value is called, in `values` order.
    pub labels: Vec<String>,
}

/// Why a request has no reply, as a program reads it. The JSON-RPC number
/// each maps to is [`ErrorCode::number`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// The line was not JSON.
    Parse,
    /// The JSON was not a request: an unknown method, a missing or
    /// ill-typed field, a value out of range.
    Invalid,
    /// The project named is not open; open it first.
    NotOpen,
    /// A job, template or package named does not exist.
    NotFound,
    /// The edit layer said no. The message is the sentence the window
    /// would show, and the state is as it was.
    Refused,
    /// A one-at-a-time job is already running.
    Busy,
    /// The job was stopped by [`Request::ExportCancel`].
    Cancelled,
    /// The transport wanted a token and did not get the right one. Never
    /// raised by the API itself.
    Unauthorized,
    /// Everything else: a file that could not be read or written, a decode
    /// that failed, a model that did not download.
    Failed,
}

impl ErrorCode {
    /// The code's JSON-RPC 2.0 number: the standard ones for the standard
    /// meanings, and server-defined ones (-32000 downwards) for the rest.
    pub fn number(self) -> i64 {
        match self {
            ErrorCode::Parse => -32700,
            ErrorCode::Invalid => -32600,
            ErrorCode::Failed => -32000,
            ErrorCode::NotOpen => -32001,
            ErrorCode::NotFound => -32002,
            ErrorCode::Refused => -32003,
            ErrorCode::Busy => -32004,
            ErrorCode::Cancelled => -32005,
            ErrorCode::Unauthorized => -32006,
        }
    }
}

/// A refusal: what kind, and the sentence a person would be shown.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// What kind of refusal.
    pub code: ErrorCode,
    /// The sentence.
    pub message: String,
}

impl ApiError {
    /// An error of `code` saying `message`.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> ApiError {
        ApiError {
            code,
            message: message.into(),
        }
    }

    /// [`ErrorCode::Invalid`].
    pub fn invalid(message: impl Into<String>) -> ApiError {
        ApiError::new(ErrorCode::Invalid, message)
    }

    /// [`ErrorCode::Failed`].
    pub fn failed(message: impl Into<String>) -> ApiError {
        ApiError::new(ErrorCode::Failed, message)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// A request's outcome. `{"result": ...}` when it worked, `{"error":
/// {"code": ..., "message": ...}}` when it did not.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum Response {
    /// The reply.
    Result(Reply),
    /// Why there is none.
    Error(ApiError),
}

impl From<Result<Reply, ApiError>> for Response {
    fn from(result: Result<Reply, ApiError>) -> Self {
        match result {
            Ok(reply) => Response::Result(reply),
            Err(error) => Response::Error(error),
        }
    }
}

/// Something that happened to a job. A transport forwards these to every
/// caller as they come; each names its job and its project, so a caller
/// keeps the ones it asked for and ignores the rest.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all_fields = "camelCase")]
pub enum Event {
    /// A cutout's masks are being found ahead of an export.
    #[serde(rename = "cutout.progress")]
    CutoutProgress {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// The media being analysed.
        media_id: String,
        /// True while a model downloads, false while it runs.
        fetching: bool,
        /// How far along, `0..=1`.
        fraction: f32,
    },
    /// A stabilized clip's media is being measured ahead of an export.
    #[serde(rename = "stabilize.progress")]
    StabilizeProgress {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// The media being measured.
        media_id: String,
        /// How far along, `0..=1`.
        fraction: f32,
    },
    /// The export moved on.
    #[serde(rename = "export.progress")]
    ExportProgress {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// Frames done.
        frame: i64,
        /// Frames in total.
        total: i64,
        /// "video", "audio" or "mux".
        stage: String,
    },
    /// The export wrote its file. The job is over.
    #[serde(rename = "export.done")]
    ExportDone {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// The file written.
        output: String,
        /// Its picture size.
        width: u32,
        /// Its picture size.
        height: u32,
    },
    /// The export did not write its file. The job is over.
    #[serde(rename = "export.failed")]
    ExportFailed {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// Why: [`ErrorCode::Cancelled`] when asked to stop, otherwise what
        /// went wrong.
        error: ApiError,
    },
    /// Progress update from a non-export background job.
    #[serde(rename = "job.progress")]
    JobProgress {
        /// The job.
        job: String,
        /// The project folder, or empty when project-less.
        path: String,
        /// Stage or status line.
        stage: String,
        /// Progress fraction in `0..=1`.
        fraction: f64,
    },
    /// A non-export background job completed.
    #[serde(rename = "job.done")]
    JobDone {
        /// The job.
        job: String,
        /// The project folder, or empty when project-less.
        path: String,
        /// Main written file path, if any.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        output: String,
        /// Structured JSON outcome.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
    },
    /// A non-export background job failed or was cancelled.
    #[serde(rename = "job.failed")]
    JobFailed {
        /// The job.
        job: String,
        /// The project folder, or empty when project-less.
        path: String,
        /// Failure details.
        error: ApiError,
    },
}

impl Event {
    /// [`Event::ExportProgress`] for one report from the renderer.
    pub fn progress(job: &str, path: &str, progress: Progress) -> Event {
        Event::ExportProgress {
            job: job.to_owned(),
            path: path.to_owned(),
            frame: progress.frame,
            total: progress.total,
            stage: progress.stage.to_owned(),
        }
    }

    /// The job this event is about.
    pub fn job(&self) -> &str {
        match self {
            Event::CutoutProgress { job, .. }
            | Event::StabilizeProgress { job, .. }
            | Event::ExportProgress { job, .. }
            | Event::ExportDone { job, .. }
            | Event::ExportFailed { job, .. }
            | Event::JobProgress { job, .. }
            | Event::JobDone { job, .. }
            | Event::JobFailed { job, .. } => job,
        }
    }
}
