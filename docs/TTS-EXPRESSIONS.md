# Gemini 3.8 inline expressions

Select Gemini TTS with Gemini 3.8 Flash TTS or Flash-Lite TTS. The **+ Biểu cảm** menu inserts a vocal event at the text caret, both in the main narration script and each dubbing sentence. Labels are Vietnamese; the inserted tags are English as recommended by Google: `<laugh>`, `<cry>`, `<chuckle>`, `<sigh>`, `<cough>`, `<breath>`, `<gasp>`, `<short pause>`, `<long pause>`.

Example: `<laugh> Tôi muốn khóc <cry>`.

Use the Style field for sustained emotion/delivery such as a sad voice, anger or whispering across the turn. Inline tags produce a momentary vocalization or pause. They are sent verbatim to Gemini, remain intact when a long script is chunked, and are excluded from expected spoken words during Whisper sentence matching. A sentence made entirely of non-speech events cannot be reliably aligned by lexical Whisper matching; generate that sentence individually and review its timing.

Editing expression tags invalidates previous sentence audio and approval, just like editing text. Generate again and listen before applying. The expression menu is offered for the two explicit 3.8 Gemini model IDs; it does not imply that Local or Gemini 2.5 reads angle-bracket tags correctly.

Reference: https://ai.google.dev/gemini-api/docs/speech-generation#prompting-guide

Added regression cases cover preserving tag boundaries in chunking, passing tags unchanged into the API request, and ignoring vocal-event tags when matching spoken words. No compilation, test execution or live synthesis has been run while builds are on hold.
