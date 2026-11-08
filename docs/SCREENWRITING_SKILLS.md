# GitHub screenwriting skills in Concat

Source: [ur-grue/autopunk-media-skills](https://github.com/ur-grue/autopunk-media-skills),
commit `39e119bfef25957223d6c44fccab0329e936d2f6`, MIT, copyright 2026 ur-grue.
The original skill files are installed in `.agents/skills/` and remain unchanged.
The original license is retained in each installed folder and in
`docs/licenses/autopunk-media-skills-MIT.txt`.

Selected and read in full before use:

- [beat-sheet-builder](https://github.com/ur-grue/autopunk-media-skills/blob/39e119bfef25957223d6c44fccab0329e936d2f6/skills/screenwriting/development/beat-sheet-builder/SKILL.md): concrete causal beats and visible turns.
- [character-profile-writer](https://github.com/ur-grue/autopunk-media-skills/blob/39e119bfef25957223d6c44fccab0329e936d2f6/skills/screenwriting/development/character-profile-writer/SKILL.md): want/need, behavioral limitation and distinct voice.
- [scene-writer](https://github.com/ur-grue/autopunk-media-skills/blob/39e119bfef25957223d6c44fccab0329e936d2f6/skills/screenwriting/scripting/scene-writer/SKILL.md): location/time, filmable action and a meaningful scene change.
- [dialogue-polisher](https://github.com/ur-grue/autopunk-media-skills/blob/39e119bfef25957223d6c44fccab0329e936d2f6/skills/screenwriting/scripting/dialogue-polisher/SKILL.md): speaker-specific, concise, purposeful dialogue with preserved intent.

Other candidates reviewed included xnomad-dev/claude-skills/video-script-writer
and advisorita/storytelling-frameworks; their repositories did not declare a
license in GitHub's metadata at review time, so their text was not installed or
copied into the app. No remote scripts, external model endpoints, hooks or
example story content from any skill are executed by Concat.

## Runtime adaptation

`src/crates/concat-speech/prompts/screenwriting.md` adapts the craft principles
for short Flow clips. `flow_content.rs` embeds that local prompt at build time;
merely installing a Codex skill does not change Gemini's behavior. This explicit
adapter is what makes the app use the skills. Runtime never fetches GitHub.

Flow's screenplay tab now supplies video format, structure, audience, viewer
goal and a requested 1–12 scene count. Per-clip seconds come from the Tạo video step.
The app compresses feature-film beat concepts to that duration/count rather
than copying fifteen beats or multi-page scenes. Nonfiction modes preserve
provided facts instead of inventing documentary quotes or product claims.

The Cảnh step holds per-character action/emotion, attributed dialogue and the
collapsible prompt editor. AI screenplays also synchronize the shared character
registry and per-scene cast. Existing screenplays have an extraction action in
Kịch bản. Speaker identities are stable across profile renames.

The AI dialogue pass only writes the selected scene's dedicated dialogue fields.
Structured dialogue keeps the same number/order of lines and speaker IDs.
It backs up current settings to `flow-before-dialogue-ai.json` before requesting
the edit; action text and other scenes stay untouched. A changed dialogue marks
the video prompt stale. Dedicated dialogue takes priority over older dialogue
inside a scene description when composing the next video prompt.

Structured replies are checked for the requested scene count and scene-internal
blank lines are normalized to prevent the bulk editor from splitting one scene
into several. This is craft guidance and structural validation, not a guarantee
of story quality or exact speech/render timing. Automated tests do not call
Gemini or submit a paid Flow generation.

## Character references for Flow images

The character-profile-writer adaptation now asks for concise labeled identity,
fixed appearance, want/need, flaw/motivation, voice and short-arc notes within
the existing profile string. Genre and established facts take precedence;
nonfiction and children's material do not acquire invented trauma to satisfy
a dramatic checklist. Voice notes guide dialogue, not automatic voice cloning.

Flow image prompts request one readable character, a neutral pose, stable
wardrobe and a simple background. Psychological notes are expressed through
subtle posture/expression, without rendering biography or dialogue as text.
An existing profile reference is attached to support visual identity across
regenerations. This does not guarantee exact likeness or modify the saved
native Flow character/voice identity.
