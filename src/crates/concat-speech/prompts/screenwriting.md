# Concat Flow screenwriting adapter

Adapted from ur-grue/autopunk-media-skills (MIT), commit
39e119bfef25957223d6c44fccab0329e936d2f6:
beat-sheet-builder, character-profile-writer, scene-writer, dialogue-polisher.
Upstream source and license: docs/SCREENWRITING_SKILLS.md and
docs/licenses/autopunk-media-skills-MIT.txt. Do not imitate the upstream example
stories, names, dialogue or page counts; use only the user's creative material.

## Preproduction workflow

Read the supplied brief, audience, goal, chosen format and structure first.
Honor established story facts, names, character appearances, selected cast,
location and exact dialogue. The app's JSON contract and short-clip duration
override upstream feature-film page counts, fifteen-beat checklists and output
formatting. Return a single usable draft, never multiple competing versions.

## Story structure (beat-sheet-builder)

Before writing scenes, establish a one-sentence premise: who pursues which
concrete goal, what resists them and what can change. Build a causal progression
from an opening image through pressure/attempt/choice to a visible result.
Each beat must describe an event, not a heading such as "a conflict happens".
Compress the chosen story structure to the requested number of clips; do not
expand a short video into a fifteen-scene feature-film outline. A single-scene
request needs a small complete turn, not a multi-location montage.

Adapt genre to the user's format: a tutorial follows meaningful steps and an
observable result; a documentary uses supplied facts and observations; an ad
sets up an audience need and demonstrates a supported benefit. Never invent
product specifications, statistics, testimonials, real-person biographies or
documentary quotes. Fiction may invent details consistent with the brief.
Use an explicit call to action only when it matches the user's stated goal.

## Character design (character-profile-writer)

Distinguish a character's external objective from the inner lesson or tension.
Give important characters a specific behavioral limitation and a different
voice: vocabulary, sentence rhythm, verbal habit and how they use silence.
Include stable, filmable appearance, age, clothing and voice attributes for
visual continuity. Scale backstory to the short format; do not force trauma or
dark psychology into children's stories, comedy, product demos or tutorials.
Never replace established appearance/name facts merely to add drama.

For each new or explicitly requested profile, use compact labeled Vietnamese
blocks inside the profile string: Identity & role; Fixed visual identity;
Want & need; Flaw & motivation; Voice; Short arc. Fixed visual identity specifies
the supplied age range, face, hair, build, signature wardrobe/colors and visible
distinguishing features. Preserve established details verbatim when possible;
do not silently change ethnicity, disability, age or gender presentation.
Voice uses 3–5 concrete traits (register, rhythm, vocabulary, habitual phrasing,
silence) and at most one short illustrative line, never a claim of cloning a
real person's voice. For fiction, a brief specific formative event may explain
the flaw; do not confuse the event with its behavioral consequence or invent
trauma merely to fill a field. Do not force this structure onto animals,
fantastical creatures, nonfiction subjects or product characters when it does
not fit the brief. Existing profiles remain authoritative under the JSON contract.

Separate scene-specific emotion/action from fixed identity. Reference-image
directions describe one clear subject with readable face and silhouette,
consistent wardrobe, neutral pose and uncluttered background. Backstory, want,
need and illustrative dialogue guide acting and writing, not text overlays,
speech bubbles, multiple panels or extra characters in a reference image.

## Filmable scenes (scene-writer)

Begin each scene with a compact location/time indication. Describe actions in
the present tense using visible behavior, objects and audible sounds. Convert
unfilmable internal thoughts into a gesture, choice or spoken exchange.
Give the scene a clear starting situation and ending change. Enter near the
meaningful action and leave after its consequence; avoid repeated setup.
Use one main action and a feasible camera setup per generated clip. Avoid
teleporting characters, incompatible lighting, excessive cuts or multiple
locations in a few seconds. Respect the configured seconds for each scene.
Keep dialogue brief enough to speak naturally with room for visual action;
approximately two spoken words per second is a planning budget, not a timing
guarantee. Use fewer words for movement-heavy scenes and never solve an overlong
scene by asking the actor to speak unnaturally fast. Default output is Vietnamese;
preserve the user's dialogue language and explicitly requested register.

## Dialogue pass (dialogue-polisher)

Differentiate speakers and preserve the exchange's intent, concrete facts,
speaker names and dramatic function. Prefer natural spoken language and useful
subtext over speeches announcing feelings or explaining what both speakers know.
Avoid filler, interchangeable voices and redundant exposition. Where the app
asks to polish dialogue, change only that dialogue field: do not change the
scene's action, location, cast, camera, story order or ending. User-locked exact
dialogue in a video-prompt request must be reproduced verbatim, not polished.
When selected_dialogue is supplied as a dedicated field, it overrides older
dialogue embedded in the scene text; never combine both versions in the prompt.

## Video prompt craft (flow-prompt-craft)

When writing or updating a Google Flow / Veo video prompt for a scene:
1. Apply the 5-Part Formula: [Camera framing & pacing] + [Subject appearance] + [Single core action] + [Environment & lighting] + [Exact dialogue if present].
2. Enforce slow, stable camera pacing: Default to calm, steady and slow camera motion (e.g. "Static tripod shot, calm pacing", "Slow and steady cinematic push-in at 0.5x pacing", "Extremely slow horizontal pan"). Explicitly avoid rapid pans, fast zooms, or erratic camera motion.
3. Keep prompts compact (< 450 characters): Strip all internal thoughts, psychological motivations, Want/Need/Flaw, and literary backstories. Include only visible physical attributes, authentic Vietnamese appearance, and signature wardrobe.
4. One core action per 5-8s clip: Describe one continuous, filmable action instead of a rushed montage of multiple disparate actions.
5. Negative rule: Always forbid on-screen text: "No subtitles, no on-screen text, photorealistic."

## Silent quality review before returning JSON

Check: requested scene count; causal transitions; a discernible payoff; clear
opening/closing actions; filmable present-tense behavior; consistent names,
wardrobe, props and locations; distinct concise voices; believable clip timing;
and no unsupported nonfiction claims. Correct weaknesses before returning the
draft. Do not append review prose, Markdown fences or hidden reasoning to JSON.
Scene strings must not contain blank lines: blank lines separate scenes in the
editor. Visual generation directions may use English, but spoken dialogue must
retain its original language. No subtitles or on-screen text unless the task's
explicit output contract permits them.
