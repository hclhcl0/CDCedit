# Google Flow in Concat

> **Developer & Agent Guide**: For an exhaustive technical reference, action mapping table, CDP automation protocol, and instructions on modifying Flow features, see [AGENT_GUIDE_GOOGLE_FLOW.md](AGENT_GUIDE_GOOGLE_FLOW.md).

GitHub screenwriting craft skills are applied through an embedded runtime
adapter. See [SCREENWRITING_SKILLS.md](SCREENWRITING_SKILLS.md) for source,
license, short-clip adaptations, controls and validation.

The native desktop title bar has a **Google Flow** button. It selects Flow in the existing Properties inspector,
not the standalone prototype website. Configure/connect an account in Settings,
sign in manually, enter a
Flow project URL and prompt, choose Omni Flash 1.2, ratio and duration. **Prepare**
fills the composer without submitting the prompt. A reference image, if supplied,
is uploaded during preparation. Generation additionally requires explicitly
enabling Flow credit use and pressing **Create video**.

Completed clips remain in the selected account's application-data output folder
(`google-flow/output/` for the primary account). **Import into library** goes through the existing media
import/probe pipeline. Import needs an open Concat project; adding a clip to the
timeline remains the editor's usual action. **Saved results** lists completed
local downloads. Closing a project never imports its late result into another
project. Closing the sheet does not cancel a render already submitted to Google.

Settings are remembered in `google-flow.json` in the Concat project folder, or in
the application data directory when no project is open. Chrome uses a separate
persistent profile in Concat's data directory. No passwords or cookies are copied
from Sesame or from the standalone prototype. Job receipts and the upstream
worker's job records survive a restart. Unknown outcomes are not retried
automatically; inspect Flow before submitting a replacement job.

## Development and packaging

- Worker source: `tools/google-flow-worker`, vendored MIT upstream with explicit
  model matching. `bridge.mjs` provides serialized JSON-lines IPC on standard IO.
- Rust process/validated output paths: `flow_worker.rs`; project-scoped state and
  existing media import: `panes/flow.rs`; Slint sheet: `dialogs/google-flow.slint`.
- Build worker: install its lockfile with `npm ci --ignore-scripts`, then
  `node node_modules/typescript/bin/tsc -p tsconfig.build.json`.
- Windows local build: `scripts/build-concat-desktop.ps1 -Mode Check`, then
  `-Mode Build`. The helper reuses optional local native toolchains without
  changing system settings. It stages sherpa import libraries to avoid the
  dependency's DLL copier failing on the custom quick profile.
  Use `-Mode Build -NoAppIncremental` on a space-limited machine to avoid the
  generated UI's large incremental query cache; library caches are retained.
- Package Node + worker beside `cdcedit.exe` with
  `scripts/package-flow-worker.ps1 -Destination <absolute-path>/google-flow-worker`.
  Chrome is a prerequisite. Do not distribute profile/data/log directories.
- Bridge test: `node --test bridge.test.mjs` in the worker folder. It verifies
  persistent requests, credit gating and receipt recovery without generation.
- Native regression tests cover Flow project URL validation and restricting
  imported results to real video files within the managed output tree.

## Scope

Flow Studio lives in the existing Properties inspector's Flow tab (no extra
column and no modal overlay), with
Screenplay, Characters, Scenes, Generate video and Results steps. The prompt
editor is inside Scenes. Add/remove scene controls and
editable individual character profiles are saved in the project settings.
The panel includes a manual screenplay editor: separate scenes with
blank lines, enter shared character/location continuity and visual/camera/sound
notes, select a scene, then compose its editable prompt. These fields are saved
with the project's existing Flow settings. Composing never submits a generation;
the normal prepare and credit-gated generation actions remain explicit.
Gemini draft buttons reuse the API key pool from Settings: generate new scenes,
synchronize character profiles and background descriptions, or write the selected
scene's prompt. The shared idea/brief and editorial context are sent only when
an AI button is clicked. Text generation can incur Gemini API usage and is
separate from Flow video credits. Structured responses are validated before
applying; failures preserve existing edits. Generated drafts are editable and
saved per project. A separate image-generation button can create reference
images; it never submits a Flow video automatically. The existing
standalone prototype remains separate. Flow's interface can
change, and account plan/model/duration availability is checked by Flow; no
guarantee is inferred from merely seeing a model in the native dropdown.

## Accounts, connections and parallel queue

Manage connections in **Settings → Google Flow**. Add named accounts, enable
them, edit connection settings and save before connecting or checking status.
These settings are application-wide in `google-flow/accounts.json`; project
screenplay and scene settings remain separate. Removing an account entry does
not delete its Chrome profile or downloaded files.

Each account has its own worker, CDP port and output directory. The primary
account retains the legacy directory; additional accounts use
`google-flow/accounts/<id>/`. Choose a managed persistent Chrome profile, attach
to an existing local CDP endpoint, or launch a dedicated existing user-data
directory. Attaching requires Chrome already started with remote debugging;
ordinary Chrome without CDP cannot be attached this way. The regular Chrome
User Data directory is rejected for custom launches. Sign in manually; Concat
does not copy passwords or cookies. A detected legacy profile initially uses
attach mode to preserve its existing connection.

The simultaneous-account and total-task controls each support 1–4. Effective
capacity is the smaller setting, with at most one video task per profile.
These limits apply within a running Concat session. Automatic scene assignment
rotates between eligible enabled accounts. A scene can instead choose a fixed
account and project URL. Imported native Flow characters and their original
voice stay bound to their source account and project; incompatible character
combinations fail before submission. Connection/profile edits are held while
work is active; concurrency limits can be saved independently.

## Scene plans and queue recovery

Each scene now stores its prompt, reference-image path, shot/angle, camera
movement, sound/music/SFX, exact dialogue, status and downloaded video paths.
Switching scenes restores its prompt and validated results. Editorial changes
mark prompts stale; regenerate them locally or with Gemini before preparing or
submitting. The Video tab can compose missing/stale prompts, export all scene
prompts to `flow-prompts.txt` beside project settings, and select scenes to run.

Running the queue requires the explicit credit toggle and the separate paid
queue button. It validates all eligible scenes before starting and submits up
to the saved account/task limits. Completed responses must contain existing
validated local video files. Completed or uncertain scenes are not automatically retried.
Stop/closing Flow prevents future submissions; an already-submitted render is
not cancelled. Errors, missing downloads or uncertain outcomes block that account
for the rest of the queue; other eligible accounts can continue. The failed
scene is never automatically resubmitted using another account.
On restart an in-flight scene becomes uncertain and queued-but-unsent scenes
return to idle; queues never auto-resume. Project switches drop late UI results.

Scene metadata tests use the actual `flow_scene.rs` via the isolated native
test harness and cover interrupted-job exclusion, stale-prompt exclusion and
occurrence-aware metadata mapping on scene reordering/duplicates. No paid
multi-scene queue was submitted during verification.

## Reference assets, cast, backgrounds and suggestions

Scenes can choose an explicit cast and a reusable background. Deleting a
profile/location remaps scene references instead of silently assigning another
person/location at the same index. Older scene files keep their previous shared
cast behavior until the user makes an explicit selection.

Editable dropdown suggestions cover shot size, angle, camera movement, lighting,
sound/music and visual style. Selected suggestions populate ordinary text fields
and participate in both local prompt composition and Gemini prompt generation.

Gemini character/location images reuse Settings' key pool with an editable image
model ID (default `gemini-2.5-flash-image`). The request/inline image format follows
[Google's image generation API](https://ai.google.dev/gemini-api/docs/generate-content/image-generation?hl=en).
Image responses are size bounded, MIME/signature checked and decoded by Slint
before attaching. Files are stored under application data `google-flow/assets`.
Text-only, malformed or failed responses preserve the previous asset. A separate
Use image action adds the asset to the selected scene; preparation uploads those
references to Flow. Multiple local references use one path per line (maximum 8,
actual model/account support may be lower). No image generation API was called
during automated verification.

Each character also has a local **Choose image from computer** action, independent
of Gemini keys. It accepts PNG/JPG/JPEG/WebP up to 32 MB and copies the selected
file into `google-flow/assets` so moving the original does not break the profile.
The same preview and **Use image for scene** controls handle both local and AI
images. Cancelling the picker changes nothing; a project/profile-list change
during selection discards the stale result. An unreadable image preserves the
previous profile image. Selection alone does not upload the image or call an API.

The screenplay tab exports `flow-storyboard.json` without runtime job results.
Import accepts a bounded JSON file, backs up the current draft to
`flow-before-import.json`, preserves the current Flow project URL and clears
runtime statuses/results/queue selection. JSON stores image paths rather than
embedding image files. The queue also supports status filtering and selecting
unfinished scenes together.

## Import saved Flow characters (source-only upgrade)

In Properties → Flow → Nhân vật, enter the source project URL, choose
**Đọc thư viện nhân vật Flow**, then **Nhập nhân vật** or **Cập nhật từ Flow**.
This reads the existing signed-in managed Chrome session; it does not generate
images or videos or spend generation credits. The imported profile contains the
name, personality notes, portrait (when exposed), and voice reference ID. The
voice ID is metadata, not downloaded audio or a guarantee of voice reuse.

The adapter follows RohaanA/google-flow-mcp at commit
`a357dc36b8b9c2e8c85c809e4d80210149652fd0` (MIT; see THIRD_PARTY_NOTICES.md).
It reads project characters and resolves each portrait through its own media or
workflow identity rather than gallery position. Project/media identities and
Google image download hosts are checked. Unsupported protocol responses fail
without replacing the existing local profile. Google may change these private
read endpoints.

Imported profiles are matched by source project ID and character ID, so a repeat
import updates the same profile. Before changes, the current settings are backed
up to `flow-before-character-import.json` in the managed Flow state directory.
Portraits are saved under `google-flow/assets/flow-import`. Import does not add
the portrait to a scene automatically; use the existing scene-reference action.

The four offline protocol tests pass with
`node --test character-protocol.test.mjs` from `tools/google-flow-worker`.
This upgrade has not been compiled, packaged, or tested against a live account,
in accordance with the request to defer builds. The existing EXE is unchanged.
The next explicitly requested desktop Build will also package the updated worker.

### Reuse the original character and its saved voice

Imported Flow characters now retain a native source link during video preparation
and generation. In the screenplay tab, select the imported character under the
scene's cast list (marked **Gốc Flow + giọng**, or **Gốc Flow** when no voice is
saved). An unconfigured cast uses all profiles, matching continuity behavior.
Use the original Flow project URL and Omni Flash 1.2, then prepare the scene.
The worker re-reads the library by ID, verifies that the imported voice ID is
unchanged, and uses Flow's native character ingredient picker. It verifies the
character chips before preparation succeeds and before sending. The preparation
result reports the number of attached characters. Cached portrait copies for
those characters are excluded from image uploads; other scene references remain.

Missing characters, changed voices, duplicate picker names, a different project,
an unsupported model, or unverified ingredients stop the request before sending.
Rename duplicate characters on Flow or re-import an updated profile as needed.
The agent follow-up nudge is disabled for these requests so it cannot substitute
a generic image instruction for the original character. There is no separate
voice generation, voice download, or audio cloning. Flow controls the final audio;
this implementation does not guarantee identical rendered voices. Picker selectors
follow the pinned MIT upstream UI adapter and have not been tested live locally.
This addition is also source-only: no build or paid generation was run.

## Shared screenplay characters and per-line speakers (source-only)

The screenplay and character tabs now share persistent local character IDs.
Existing index-based cast selections migrate to IDs on load; legacy profile
text, portrait paths and native Flow bindings stay attached to the same profile.
JSON storyboard import/export includes the identities, per-scene cast, character
actions/emotions, and structured dialogue rows. Editing a profile's first line
changes its display name without changing its identity. Free-form scene prose is
not rewritten by a rename; review older names in that prose when necessary.

**AI viết kịch bản** returns character profiles and cast/speaker assignments with
the new scenes. **AI trích xuất nhân vật & phân vai theo cảnh** reads existing
scene text and synchronizes those assignments without rewriting the scene text.
Both use the configured Gemini text API keys; they do not generate images or
submit videos to Flow. Results are validated on a cloned draft and backed up to
`flow-before-cast-sync.json` before replacing live state. Unknown identities,
ambiguous duplicate profiles, invalid speakers or incomplete scene assignments
reject the draft. Existing profiles, portraits and Flow source links are retained
when an existing character is reused. Two people named Nam can have different
roles, such as father and student; their scene and speaker links remain distinct.
An ambiguous speaker is retained as an unassigned dialogue row; assign that row
before preparing the prompt. AI extraction is a draft for review, not a guarantee
that every name or speaker in free-form prose will be identified correctly.

In **Kịch bản**, toggle each appearing character and enter their action/emotion.
Local portrait references are collected automatically for those selected profiles;
portraits belonging to unselected profiles are excluded. Native Flow characters
use their original character ingredients instead of uploading the cached portrait.
The outgoing prompt identifies which uploaded image belongs to each character.
Use **Thêm câu thoại / người nói** to add individual lines, select the speaker,
and enter their exact words. Selecting a speaker also includes that character in
the scene. Removing them from the cast leaves the dialogue visible and blocks
prompt preparation/generation until the speaker is corrected. Deleting a profile
does not silently reassign their dialogue to another character. AI dialogue
polishing preserves the number, order and speaker identity of structured lines.
Legacy free-text dialogue remains editable; use extraction to obtain speaker
assignments, or clear the old dialogue before adding structured lines manually.

In **Nhân vật**, the Flow library now offers a target local profile selector.
Select the screenplay's Nam and choose **Gắn vào hồ sơ đã chọn** on the desired
Flow character. This retains Nam's local profile and identity while attaching the
source character/voice and portrait. With no target selected, import creates or
updates a source-linked profile as before. Video preparation includes an explicit
mapping between the screenplay's character label and the attached Flow character's
name so those names can differ.

Source tests were added for migration, renaming, duplicate names with different
roles, invalid speaker/cast links and AI speaker preservation. They have not been
compiled or executed because builds remain deferred. No live Gemini/Flow calls,
UI render, EXE build or packaging was performed for this upgrade.

## Flow Studio workflow navigation

The inspector's fixed tools are **Flow Studio → Lồng tiếng AI → Phụ đề AI**.
**Thuộc tính** appears only for selected video/audio clips. Flow Studio keeps
five numbered steps: **Kịch bản → Nhân vật → Cảnh → Tạo video → Kết quả**.
The active scene stays selected between steps; the step is also remembered when
switching inspector tools. Each step has Back/Next navigation and its scroll
position resets to the top when changing steps.

The screenplay overview owns the brief, AI writing/extraction and JSON transfer.
Shared characters and backgrounds have separate groups within the character
step. The scene step owns cast, action/emotion, dialogue, camera/lighting/sound
and a collapsible prompt editor. Generation owns the Flow connection, output
settings, references and queue. Results lists per-scene statuses and downloaded
files, with actions to open a video, reveal its folder or import into the editor.
Changing steps does not submit an AI or generation request.

## Workflow build verification — 2026-11-07

The quick-profile desktop build and worker packaging completed successfully.
The new `cdcedit.exe` passed `--version` and `--smoke` (exit 0, GPU and Whisper
CPU backend initialized). Slint renders were reviewed for all five Flow steps
and the complete inspector at a 440 px panel width. Interactive input to the
running user window was stopped when the user pressed Escape; the existing
window was left running, and the rebuilt EXE was verified separately.

Validation passed: four concat-speech editorial/image tests, three shared-cast
tests against the actual source module in the isolated native harness, four
protocol tests, a headless DOM fixture exercising multi-character attachment,
and both Node bridge and native Rust IPC checks against the packaged worker.
Packaged character-adapter/composer files and license match the source build.
These checks do not verify a live Google Flow account or a paid render.

## Earlier Windows build verification (before source-only upgrades)

The app passed `cargo check -p concat --profile quick`, Slint's full-tree check,
the Node bridge test and isolated Rust tests using the actual `flow_worker.rs`
module (credit gating, persistent IPC and managed-path validation). A live
Rust → packaged worker → signed-in Flow dry run verified Omni Flash 1.2 and
returned `ready_for_confirmation`, without submission or credit spend.

The first startup smoke run found a Windows main-thread stack overflow. The
MSVC binary now reserves 32 MiB of stack in `build.rs`; the local helper also
applies the equivalent PE setting with editbin. The resulting executable passed
`--version` and `--smoke` (exit 0), launched the real editor, displayed the Flow
sheet and reported a connected Chrome session. A paid render/download/import
cycle has not been submitted as part of this validation.

## Multi-account build verification — 2026-11-08

The account-aware desktop EXE and worker package built successfully using
`-Mode Build -NoAppIncremental` with one Cargo job. Earlier attempts encountered
a stalled build and an out-of-space error while writing the UI query cache;
only generated application caches were removed before the successful build.
The new EXE passed `--version` and `--smoke` (exit 0, GPU and Whisper backend
initialized). Smoke still reports existing AppData log/card directory warnings
in this restricted environment.

Validation passed: 16 isolated Rust tests against the actual account, worker,
scene and cast modules; nine worker tests; TypeScript compilation; full Slint
tree checking; and a reviewed headless render of the accounts settings page.
Both native IPC checks also passed against the packaged worker, including
independent account state directories. The packaged bridge test passed, and
connection/composer/protocol files and character license matched the source
build by hash. No live multi-account login or paid Flow render was performed.

Vietnamese setup instructions: [FLOW_ACCOUNTS_VI.md](FLOW_ACCOUNTS_VI.md).

## Flow workspace size

The Flow inspector has three inline layout controls, without a separate window:

- **Gọn** restores the normal dock. Drag the existing panel divider to adjust
  the inspector width.
- **Rộng** gives Flow roughly 70% of the workspace with the existing preview
  alongside it. Their divider is draggable, with minimum widths of 440 px for
  Flow and 240 px for the preview. If the workspace is too narrow or no preview
  pane exists, Flow uses the full available region.
- **Toàn vùng** temporarily hides the other panels and fills the workspace.

These modes change only the published panel rectangles, preserving the dock
tree, seat indices and original split ratios. Returning to Gọn restores them.
Flow step, current scene and input components stay in place. Changing inspector
tool returns to the normal dock. Explicit dock rearrangement also exits the
temporary mode; resizing the window does not replace the saved dock while Flow
is expanded.

At 900 px or more, Flow shows a left navigation list for scenes, plus screenplay,
character and results shortcuts, and a taller screenplay editor. Narrow panels
retain the existing scene dropdown. This is the expanded editor workspace;
a node-graph video canvas is not included in this change.

Workspace-size validation (2026-11-08): full Slint checking, reviewed headless
renders at 460 px and 1366 px, two tests against the actual dock module, and
`cargo check` passed. Cargo compiled and linked the new EXE, then failed to
replace the top-level `cdcedit.exe` because the user's running process held it
open. The newly linked `deps/cdcedit.exe` was therefore staged alongside it as
`cdcedit-flow-workspace.exe`, given the normal 32 MiB stack reservation and
packaged with the current worker. Both `--version` and `--smoke` passed (exit 0,
GPU and Whisper initialized), with the existing restricted-AppData warnings.
The running older editor was left untouched. Interactive clicking in the new
native window has not been verified; the screenshots are headless fixtures.

## Reference image generation through Flow

In **Nhân vật → Tạo ảnh nhân vật & bối cảnh**, choose **Google Flow** (default)
or **Gemini API**. The Flow source uses the selected source account and its
default project URL, falling back to the URL entered in the character section.
Choose Nano Banana Pro / Nano Banana 2 / Nano Banana 2 Lite and an image ratio;
model availability is verified by the worker against the account's UI.

Enable **Cho phép dùng credit Flow**, then click the character or background's
Flow image button. This explicitly submits one image. It does not require a
Gemini API key and does not automatically fall back to the API on error.
Image jobs share the per-account worker and global task capacity with videos.
Closing the panel does not cancel an already submitted image request; uncertain
results are never automatically resubmitted.

The worker downloads into the selected account's managed output directory and
keeps a job receipt. Native code accepts only that account's bounded PNG/JPEG/
WebP files with matching signatures; Slint must decode the image before the
profile reference is replaced. The profile and image path are then persisted.
Image results are excluded from the video result list. A generated portrait is
a local reference image, not a new native saved Flow character or attached voice.
The imported native character identity/voice is not changed by this operation.

Gemini remains a manually selected option with its own model ID and API key
quota. The profile-writing and screenplay AI buttons still use Gemini text
generation; they are separate from the image provider choice.

Validation (2026-11-08): full Slint checking and a reviewed character/background
render passed. The isolated native harness passed 19 tests (one live test
ignored); the worker suite passed nine tests, and the editorial/image crate
passed four content tests after the character-skill adaptation. Native IPC and
the bridge credit gate also passed against the packaged worker, including
refusal of image submission without authorization. The desktop compiled in
12m21s; the initial DLL copy encountered a running editor's file locks. Packaging
now compares the final runtime overlay by hash and skips identical DLLs; rerunning
the full build helper completed successfully without recompilation.

`cdcedit-flow-images.exe` passed `--version` and `--smoke` (exit 0, GPU and
Whisper initialized), with the existing restricted-AppData warnings. No live
image generation or multi-scene video consistency assessment was performed;
no Flow credit was spent during these checks.
