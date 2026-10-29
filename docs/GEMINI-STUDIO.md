# Gemini 3.8 tools on the startup page

The **Công cụ & Tiện ích AI** section has six additional launch cards on desktop and mobile: Dialogue A/B, Expressive reading, Voice design, Voice replication, Voice library, and Reactions/overlap. Each opens the functional Gemini studio form; dialogue and reactions share the same turn editor. The gallery count follows the actual number of launch cards.

## Getting started

1. Add Gemini keys in Settings → General. No request is sent just by opening a card.
2. Choose Flash TTS or Flash-Lite TTS. The studio uses the same key pool as captions/TTS. Select a specific key/project when browsing or creating project-owned voices.
3. Enter the script, settings or recordings and explicitly press the corresponding generate/load button. Network actions may incur Google charges.
4. Use **Nghe thử**, **Dừng nghe**, and **Lưu .WAV…**. These tools run without creating a video project or applying anything to its timeline.

## Dialogue and performance

Select separate voices A/B. Add, remove and assign turns; each turn has its own text and style. **Tạo thử lượt này** generates that turn; **Tạo toàn bộ hội thoại** generates the complete dialogue. Two prebuilt voices use one native multi-speaker Interactions request. If either voice is custom, turns are synthesized separately with their owning project credentials and joined into a 24 kHz WAV. Pipe-delimited reactions such as `|ừ, đúng vậy|` pass through unchanged; native two-prebuilt-voice generation is the path for natural overlapping dialogue.

Expressive reading has editable style instructions, convenient presets, temperature 0–2, and inline vocal-event insertion at the caret. Sustained emotion belongs in style; `<laugh>`, `<cry>`, `<sigh>` and pauses belong in the transcript.

## Voice library

Load/filter the official Voices API by language code, gender, accent, pitch and search text. Pagination stays on one key/project and is bounded. Assign a result to A/B or synthesize an audition using your sample sentence. Auditions are TTS requests, not guaranteed free audio downloads. Each custom voice retains its originating credential in memory so key rotation cannot accidentally use an unrelated project. A removed key cannot be reused for generation from an old in-memory voice entry.

## Voice design and replication

Voice design sends a name, description, optional language and gender to `POST /v1beta/voices` with `type=prompted`, `store=true`. The returned voice ID is retained and selected for A. If Google supplies a valid WAV sample, it is available for preview/save; otherwise synthesize a sample sentence. A missing/invalid sample does not trigger another voice creation.

Replication requires an actual 10–30 second reference recording and a separate consent recording from the same adult speaker, plus explicit confirmation of rights/consent. The UI shows Google's English consent statement and discloses uploading the recordings and storing the profile in the Google project. Audio/video inputs are decoded locally to 24 kHz mono PCM16 WAV before sending. Consent clips are capped at 60 seconds. Google performs consent verification. The app does not generate a consent recording.

A canceled creation that Google already committed retains the returned voice ID rather than discarding it or creating another profile. Closing the form keeps an ongoing job and draft available when reopened; project changes use the existing worker epoch isolation and cancel the pane. Profiles live on Google; loading the library is how to retrieve them after restarting the app. This implementation uses stored voice IDs, not expiring stateless voice keys, and does not delete cloud profiles automatically.

## Validation status

Request-shape regression cases cover A/B mapping, style and inline tags, input bounds, and credential redaction. Rust formatting and static UI/callback/schema checks are performed without compiling. App compilation, execution of Rust tests, live Google requests, desktop interaction and Android behavior remain unverified while the user has builds on hold. No keys or recordings were read from user settings or sent during implementation.

References:

- https://ai.google.dev/gemini-api/docs/speech-generation
- https://ai.google.dev/gemini-api/docs/voice-design
- https://ai.google.dev/gemini-api/docs/voice-replication

## Reviewed translation with SRT timing

The Translation app can hand a reviewed Vietnamese script to this studio with per-sentence A/B assignments and source timing slots. In this mode, each sentence is synthesized individually, cached and fitted into its slot; leading silence is preserved. Adjust start/end fields to recover from invalid/overlapping slots or extreme tempo changes. Cached sentence audio can be previewed without another API call. Use **Bỏ mốc SRT** explicitly to return to natural two-speaker generation. Timed passes are limited to 100 sentences and source timestamps 0–30 minutes; longer translations can still export SRT or use the existing single-voice workflow.
