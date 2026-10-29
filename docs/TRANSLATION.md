# Dịch phụ đề & lồng tiếng Việt

Open the **Dịch Phụ Đề & Lồng Tiếng Việt** card in the startup Apps/AI tools section (desktop or phone).

## Workflow

1. Choose a UTF-8 SRT or an audio/video file. SRT loads directly. Media uses an installed Local Whisper model to recognize the original speech. Source language defaults to automatic detection; explicit language choices are also available. Unlike Vietnamese-only captions, this workflow never forces foreign speech to `vi`.
2. Select faithful translation or natural Vietnamese for dubbing. Optional names, terms and pronoun preferences accompany every request. Select a Gemini text model or enter its ID. Default: `gemini-3.6-flash`; other presets include `gemini-3.5-flash-lite` and `gemini-2.5-flash`. Model access and costs depend on the configured Google project. There is no automatic model substitution.
3. Press Translate All. Only subtitle text/context and glossary are sent to Gemini, using the shared rotating key pool. The media remains local for recognition. Translation uses batches with neighboring source lines and previous translations as context. Results must preserve every requested ID exactly once. A failed batch/cancel never replaces an existing translation with a partial result.
4. Compare original and Vietnamese text side by side on desktop, stacked on phones. Twenty rows per page keep long documents usable. Edit Vietnamese, translate a sentence again, or request shorter wording. Assign A/B manually when preparing multiple voices. Reading-time warnings are estimates; neither warnings nor AI shortening change timestamps.
5. Confirm that you reviewed the translation. Export Vietnamese SRT or bilingual SRT, apply Vietnamese captions to the current timeline, or continue to dubbing. Original SRT export is always available independently of AI or review.

The timeline start offset affects only applying caption clips to a project; SRT export and dubbing use the source file timestamps. Applying captions uses the existing undoable caption batch. Original wording and source timestamps are kept in memory for comparison. Changing projects cancels/isolates work using the app's existing worker epoch mechanism.

## Dubbing handoff

- **One voice, SRT timing:** write a unique reviewed Vietnamese SRT in the standalone/project audio area and open the existing dubbing tool on it. Voice generation and Whisper alignment still require their own explicit Generate action.
- **A/B, SRT timing:** load reviewed text, manually assigned speakers and timing slots into Gemini Studio. Select voices A/B, adjust per-turn style, and generate explicitly. This path creates each sentence separately and fits it to its target slot with the existing pitch-preserving renderer, rather than trying to infer boundaries inside one two-speaker recording. The WAV retains leading silence from source time zero.
- A/B timed generation currently supports 1–100 sentences and source timestamps within 0–30 minutes per pass. Longer inputs need splitting into smaller segments. Keep this limit distinct from translation, which accepts up to 5,000 cues. Use the existing one-voice workflow or export SRT for longer scripts.
- Successful sentence audio remains cached if another sentence or final fitting fails. Cached audio can be inspected, target times adjusted and the full WAV rendered again. Editing text, speaker or style invalidates affected sentence audio; changing model/temperature/voice invalidates the cached pass. A/B timing can explicitly be removed to create a natural untimed dialogue instead.

## Validation status

Static checks cover UI/Rust data field parity, callback/launcher wiring, balanced Slint delimiters and Rust formatting. Added regression cases cover strict translation IDs, preservation of source/timestamps, timing warnings and review-gated SRT export. No compilation, test execution, app interaction or live Gemini request was performed while builds remain on hold. Recognition and translation quality need actual media/API testing after a build is authorized.

API model protocol reference: https://ai.google.dev/gemini-api/docs/generate-content/whats-new-gemini-3.6 (newer text models omit deprecated sampling parameters).
