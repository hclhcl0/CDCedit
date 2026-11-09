---
name: fast-flow-dev
description: Rapid development, debugging, and live iteration workflow for Google Flow features in Concat / CDCedit. Use whenever developing, modifying, or testing Google Flow features (Chrome CDP, Veo 3.1 video, Nano Banana images, Agent instructions, characters, prompts, or UI) to avoid slow 10-minute exe rebuilds and achieve 2-second iteration cycles.
---

# Fast Google Flow Development & Iteration Workflow

When modifying or adding features to Google Flow in Concat / CDCedit, **never jump straight to a full Rust desktop build (`cargo build` or `-Mode Build`)**. The macro-generated Slint UI (`app.rs` ~136 MB) takes 6–10 minutes to link on Windows.

Instead, follow the **Decoupled 3-Tier Rapid Workflow** below to iterate in **2–3 seconds**.

---

## 1. Rule 1: Worker-First Iteration (⚡ 3 Seconds – No Exe Rebuild, No App Restart)

Over 80% of Google Flow features (Chrome CDP automation, Veo 3.1 video generation, character scraping, Agent instructions, prompt crafting, download handling) live in the TypeScript worker:
📂 `tools/google-flow-worker/src/`

### The 3-Second Live Loop:
1. **Leave `cdcedit.exe` running on your desktop.** Do NOT kill or close it.
2. Edit TypeScript code in `tools/google-flow-worker/src/`.
3. Run the fast sync script:
   ```powershell
   .\scripts\sync-flow-worker.ps1
   ```
4. **Done in 3 seconds!** Switch back to the open CDCedit window and click the button or trigger the action. CDCedit spawns the updated worker script immediately.

### When to use:
- Adding/updating tools in `tools/google-flow-worker/src/tools/`.
- Changing selectors, delays, or Chrome CDP actions in `tools/google-flow-worker/src/flow/`.
- Tweaking Veo model parameters, video durations, aspect ratios, or generation prompts.

---

## 2. Rule 2: UI Live Preview with `slint-viewer` (⚡ 0.1 Seconds – Hot Reload)

When designing or tweaking UI in `.slint` files (buttons, colors, margins, tabs, fields):
* Do **NOT** compile Rust just to check how a button or layout looks.
* Use Slint's live preview tool:
  ```powershell
  slint-viewer -I src\crates\concat\ui src\crates\concat\ui\dialogs\google-flow.slint
  ```
* Every time you save the file (`Ctrl + S`), the viewer instantly re-renders in **0.1 seconds**.

---

## 3. Rule 3: `Mode Check` during Rust Coding (⚡ 20 Seconds vs 10 Minutes)

When you must write Rust code (`panes/flow.rs`, `flow_content.rs`, `flow_visual.rs`):
* **Do NOT run `-Mode Build` while writing code.**
* Run `Mode Check` to verify syntax, types, and Slint macro bindings:
  ```powershell
  .\scripts\build-concat-desktop.ps1 -Mode Check
  ```
* Or for AI prompt/content unit tests:
  ```powershell
  .\scripts\build-concat-desktop.ps1 -Mode ContentTest
  ```
* Only run `.\scripts\build-concat-desktop.ps1 -Mode Build` **once** when the entire feature is verified and you are ready to package the final release.

---

## 4. Rule 4: System & Linker Optimizations

1. **SSD Pagefile (Prevent Memory Thrashing):**
   - Run `Chuyen_Pagefile_Sang_O_C.bat` as Administrator to set a high-speed SSD paging file (16–32 GB). This prevents Windows from freezing during `rustc` macro compilation.
2. **Stack Size on Windows:**
   - Slint UI initialization requires a 32 MB native stack. The build script automatically runs `editbin /STACK:33554432 cdcedit.exe`.
3. **Safe Shutdown:**
   - Never kill `cdcedit.exe` abruptly while it is starting up, or Windows will flag `softwareAfterCrash` in `settings.json`. If this happens, reset `"softwareAfterCrash": null` in `%APPDATA%\app.cdcedit.editor\settings.json`.

---

## Quick Reference Summary

| Task | Fast Path | Time | Rebuild Exe? |
| :--- | :--- | :--- | :--- |
| **Flow logic / Chrome / Veo** | Edit TS $\rightarrow$ `.\scripts\sync-flow-worker.ps1` | **~3s** | ❌ NO |
| **Slint UI layout / styling** | `slint-viewer` Live Preview | **~0.1s** | ❌ NO |
| **Rust syntax & type check** | `.\scripts\build-concat-desktop.ps1 -Mode Check` | **~20s** | ❌ NO |
| **AI prompt unit tests** | `.\scripts\build-concat-desktop.ps1 -Mode ContentTest` | **~9s** | ❌ NO |
| **Final Release Binary** | `.\scripts\build-concat-desktop.ps1 -Mode Build` | ~6m | ✅ YES (Once at the end) |
