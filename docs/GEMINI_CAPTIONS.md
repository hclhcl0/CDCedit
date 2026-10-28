# Gemini Live captions

Open Captions, enable transcription from clip audio, and choose **Gemini 3.5 Transcribe Live** in **Transcription service**. Set a Gemini API key with access to `gemini-3.5-transcribe-live` in **Settings → General** (also shared with Gemini TTS). The captions sheet has a shortcut to these settings. Choose Vietnamese or automatic language detection, then generate captions.

This optional service sends the selected clip's source audio window to Google over TLS. It needs an internet connection and may incur Google API charges. Whisper remains available for local transcription. API keys are masked in Settings. On Windows, saved keys are encrypted with DPAPI for the current account; other platforms use the account-private settings file. Keys never enter project files or activity logs. Obtain a key at https://aistudio.google.com/apikey.

The Live API returns finalized text rather than word timestamps. CDCedit streams 16 kHz mono PCM in six-second windows and gives each returned caption that window's approximate timing. Review wording and cue boundaries before export, especially where a sentence crosses a window. Playback speed and timeline position are applied when creating title clips. This is transcription of existing timeline media, not microphone capture.

Only finalized transcripts are used. A failed or cancelled run adds no captions; a successful run is added as one undoable batch. Connection and response timeouts report an error rather than accepting partial results. API authentication, model access, quota and recognition quality require verification with a real key and sample audio.

API reference: https://ai.google.dev/gemini-api/docs/live-api/live-transcribe
