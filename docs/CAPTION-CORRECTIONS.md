# AI correction suggestions after Local recognition

Choose Local/Whisper in Captions, enable **AI đề xuất sửa chính tả / từ nhận dạng sai**, and optionally enter character names, places or specialized terms. The Gemini API key comes from Settings. Only recognized text and the glossary are sent for contextual review; the correction step does not upload or listen to audio.

Whisper recognition runs first. Gemini then checks spelling and potentially misrecognized words using nearby subtitle lines. Suspicious lines appear in a review panel with the original, a short reason and up to three full-sentence alternatives. Every selection starts at **Giữ nguyên**. Choose alternatives individually and press **Áp dụng các lựa chọn**, or use **Giữ nguyên toàn bộ** to keep the Local transcript. Subtitle placement follows the existing caption pipeline; correction changes text only and preserves recognized timings before line wrapping.

Closing the panel preserves pending review. Reopen Captions to continue. A timeline change invalidates applying a pending review; recognize again on the intended timeline. Switching projects uses the existing project-worker isolation.

Invalid AI responses or network failures preserve the complete Local transcript and show a warning. No result is automatically substituted merely because it sounds plausible. This is text-based inference: listen to the source when evaluating an ambiguous proposal. This workflow currently runs on new Local recognition, not on existing timeline subtitle clips.

Regression cases are included for invalid/duplicate proposal IDs, limits on alternative counts, default keep-original selection, invalid choices and preservation of timestamps. Compilation, test execution and live Gemini validation are deferred while builds are on hold.
