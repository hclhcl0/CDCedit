# Concat Google Flow worker

Native Rust/Slint integration, milestone 1. This directory vendors the MIT worker from ParkSangGwon/google-flow-mcp 0.3.1, with explicit Omni Flash 1.2 matching and 3–15 second duration support. See LICENSE. No Sesame implementation is used.

Build: `npm ci --ignore-scripts`, then `node node_modules/typescript/bin/tsc -p tsconfig.build.json`. Concat finds this development directory automatically, or an adjacent `google-flow-worker` folder in a packaged app. `CONCAT_FLOW_WORKER_DIR` overrides the location.

Package Windows runtime with `scripts/package-flow-worker.ps1 -Destination <absolute-path-next-to-cdcedit.exe>\google-flow-worker`. It includes Node and dependencies so the installed app does not need global Node. Chrome is required separately. Include Node's third-party notices when distributing its runtime.

Bridge: one request/response JSON object per line on stdio. Native code serializes requests; no local HTTP listener. Tools: health, flow_connect, flow_status, flow_generate_video, recent. Generation uses allowCredit=true in addition to auto_confirm=true. Jobs are never automatically retried.

Data lives in Concat's data directory under google-flow/: Chrome profile, worker job state, receipts and output. An unknown result must be checked in Flow before submitting again. Reconnect keeps the same Chrome session while the editor is open. Signing in is manual. This profile is separate from the standalone Flow Studio prototype.

Open Google Flow from the desktop title bar. Enter the Flow project URL and prompt, choose model/length, prepare without submission, then explicitly enable credits to generate. Downloaded files are imported through the existing Concat media pipeline, not auto-inserted into the timeline. Late results from a closed project are not imported into the next project; use saved results to recover the file explicitly.

This milestone does not yet port the storyboard, synchronized prompt editor or AI screenplay UI to Slint. The standalone prototype remains available for those features.
