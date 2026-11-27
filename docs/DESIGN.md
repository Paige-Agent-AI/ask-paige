# Ask PAIGE — Design Standard

Authoritative design reference for every implementation stage. Written at the end of
Stage 1 (application shell, design foundations, Ask PAIGE branding); later stages extend
this file rather than forking it. Planning artifacts live outside the product
(`../askpaige-design-previews/`) and are never runtime evidence.

## 1. Identity

- **Product name (user-facing): Ask PAIGE.** Native window title and document title both
  read "Ask PAIGE". Technical identifiers are unchanged and must stay unchanged:
  repository path, bundle `productName` ("PAIGE Desktop", installer continuity),
  `identifier` `com.paige.desktop`, `%APPDATA%\paige-desktop` storage, scope store,
  permissions, Linear references.
- **Brand mark:** the current PAIGE Command Mark (`src/components/brand/PaigeCommandMark`),
  slash-and-orb geometry in the mark's inherent gold. It is never a letter substitute and
  never rendered as a temporary glyph. The retired orbital PaigeMark stays prohibited.
- **Palette discipline:** brand gold rides the mark and status meaning only (full-access
  dot, awakening); purple remains the interactive accent. Restraint over decoration.

## 2. The four regions

| Region | Owner | Stage 1 state |
|---|---|---|
| A — persistent rail | `src/components/Rail.tsx` | Live on every destination: brand mark, Ask PAIGE, Projects, Models & Connections, bottom Settings + trust dot + emergency stop (existing `scope_revoke_all`; no new authority). |
| B — contextual sidebar | per destination | Ask PAIGE carries its own conversation sidebar inside the work surface. Other destinations gain theirs in their own stages — never as empty placeholders. |
| C — main workspace | `src/components/Layout.tsx` | All destinations re-housed behavior-preserving; non-Ask destinations keep the Header. |
| D — Work inspector | Ask workbench (existing) | Collapsed by default (`rightOpen = false`), toggle verified 0 → 312px → 0. Generalized slot arrives with Stage 5. |

The rail is 64px, icon-only, tooltip-labelled, and identical in every destination; the
application never swaps shells when switching experiences.

## 3. Design tokens (`src/styles/tokens.css`)

- **Themes:** dark is default; light is a full token override; "system" resolves through
  `prefers-color-scheme` and live OS changes (existing mechanism, preserved).
- **Typography:** Inter/Segoe UI stack; scale xs 0.7rem → 2xl 1.6rem; mono stack for
  paths, receipts, hashes.
- **Spacing:** 0.25 → 2.5rem on a 4px base grid.
- **Color:** `--pd-bg/-elevated/-panel/-input`, borders, text + muted + dim, accent purple,
  brand gold (`--pd-gold`, `--pd-gold-bright`), success/warning/danger, focus color.
- **Motion:** `--pd-transition-fast` 120ms, `--pd-transition` 180ms,
  `--pd-ease-out cubic-bezier(0.23,1,0.32,1)`, `--pd-ease-in-out cubic-bezier(0.77,0,0.175,1)`.
  UI motion stays under 300ms; no `ease-in` entrances; `transform`/`opacity` only.
- **Reduced motion:** `@media (prefers-reduced-motion: reduce)` zeroes durations and steps
  easing — same information, no movement. Verified in capture `stage1-10-reduced-motion.jpeg`.

## 4. Motion rules (Animate discipline)

Frequency gates decide: rail navigation (tens/day) gets **no animation** — instant state
change. Color/hover feedback uses 120ms transitions. No element animates on keyboard
shortcuts. Anything a user can fire twice in a second uses transitions, never keyframes.
Motion must communicate real state only; it never impersonates execution and never delays
work. Future stages add their feedback states to this table, not new motion systems.

| Interaction (Stage 1) | Feedback |
|---|---|
| Rail navigation | instant; active tint `--pd-accent-dim` + inset border |
| Rail hover | 120ms background/color |
| Emergency stop hover | 120ms fill inversion |
| Awakening | 250ms mark reveal (opacity+scale 0.94→1), dismisses by unmount after 480ms (instant; the reveal's exit is the shell appearing — no lingering overlay) |
| Inspector toggle | instant (high frequency; motion deferred by gate) |

## 5. Focus, keyboard and accessibility

- `:focus-visible` rings use `--pd-focus` with 2px/2px offset on every interactive
  element (rail, stop button); focus is visually distinct from hover.
- Emergency stop and every rail control carry `title` + `aria-label` + `aria-current`.
- Contrast (WCAG): dark text/bg **17.09**, muted **7.65**, gold **8.63**, danger **9.93**;
  light text/bg **15.19**, muted **7.25**, danger **6.95** — all ≥ 4.5:1. Light gold is
  decorative-only (mark glyph, status dot) at **3.49:1**, above the 3:1 graphics
  requirement; it is never body text.
- No state is signalled by color alone: the trust dot pairs with a tooltip label; the
  emergency stop pairs an icon with its border and label.

## 6. Startup experience (branded awakening)

One restrained reveal of the real Command Mark while React mounts ("Preparing workspace"),
dismissing itself after 480ms into the live shell — no cinematic intro, no fabricated
progress, no blocking waits. It waits for nothing consequential: no model, MCP, or scope
round-trip. Readiness is progressive: the shell is interactive immediately; the trust
dot fills when `scope_status` returns, and shows an honest neutral state until then.
Launch never resumes consequential work and never expands computer permissions. If the
frontend fails to render, no overlay masks it (disclosed limitation: an in-app
recoverable-failure state needs a Stage 3 lifecycle decision).

## 7. Performance (measured, installed builds)

Method: process spawn → debug page reachable → React shell mounted → first native command.
Device: owner's Windows machine, WebView2.

| Build | spawn→page | page→shell | first `scope_status` |
|---|---|---|---|
| Pre-Stage-1 baseline (cold) | 313ms | 242ms | ~2,840–3,070ms |
| Pre-Stage-1 (warm) | 332–345ms | 169–231ms | ~2,840–2,940ms |
| **Stage 1 (cold)** | ~350ms | ~190–335ms | 28ms |
| **Stage 1 (warm)** | 231–353ms | 159–261ms | 10–25ms (one 2.5s first-run-after-install disk outlier) |

Objectives: first visible window < 1s (**met**: ~0.35s), interactive workspace < 3s where
practical (**met**: shell interactive ≈ 0.55–0.7s), prompt local feedback (**met**), zero
artificial loading delay (**met** — awakening is 480ms and informational only). Idle
overhead: no new background work introduced; the rail's status refresh is event-driven
(navigation + revoke), not polling. Nothing was optimized speculatively.

## 8. Scaling and layout

Anchored flex/grid layouts only — no fixed-position screens. The rail narrows to 52px
below 560px viewport. Windows display scaling verified by forcing WebView2 device scale
factor: captures at 100% (`stage1-11`), 125% (`stage1-12-scaling-125-v2`), 150%
(`stage1-13-scaling-150-v2`) — no clipped or overlapping shell elements.

## 9. Stage ownership map

| Destination | Region B sidebar | Region D inspector | Stage |
|---|---|---|---|
| Ask PAIGE | existing conversation sidebar | existing workbench (collapsed default) | Stages 2–5 refine |
| Projects | Stage 6 | Stage 6 | 6 |
| Models & Connections → Customize | Stage 7 | — | 7 |
| Settings → Profile & Settings | Stage 8 | — | 8 |

Future interactions recorded for later stages, **not implemented**: composer attachment
processing feedback, submission/stop motion, approval emphasis, completion summaries,
artifact expansion, project-switch transitions. Unsupported capabilities (voice dictation
route, scheduled, more) stay invisible rather than placeholder-operational.

## 10. Verification gate (every stage)

Behavior-preserving re-housing plus the full suite: `cargo test --lib` (155), static
security (45), ask runtime (24), installed-device scope acceptance (48) — then installed
Windows screenshots from the compiled binary only, independent NON-AUTHOR review, and
explicit owner acceptance before the next stage begins.

## 11. Stage 1B — premium experience standards (authoritative)

**Cinematic awakening.** One gold light, the actual Command Mark, the wordmark
"Ask PAIGE", tagline "Your intelligent Windows workstation". Pure
transform/opacity keyframes; no media dependency; audio signature reserved
(separately gated). Three startup experiences, a stored preference
(`paige.startup`): Cinematic (~4s), Express (~1.2s), Instant (none). The
overlay auto-enters; Skip and Escape enter now; reduced-motion shows the same
information as calm crossfades. Application readiness is never blocked or
faked — the shell initializes underneath and measurements are taken
independently of presentation duration.

**Appearance model.** Two primary choices, Light and Dark; "Match Windows
automatically" is one optional preference, not a competing theme button. The
stored value keeps its previous meaning (light/dark/system) — migration is
honest and documented; nothing silently changes.

**Conversation foundations.** Human turns: right-aligned surface with a
4px-corner radius asymmetry. Assistant turns: full-width prose, one Markdown
hierarchy (18/16.5/15 headings, hairline tables, mono code), identity by
label and alignment rather than chrome. Existing actions keep behavior;
presentation only. New reactions/artifact rendering remain gated to their
stages.

**Settings architecture.** Category navigation: Personal (Personalization) —
Application (Appearance, General, Skills) — Trust and data (Privacy &
Security: workspace access + Computer Access) — Information (About). Every
listed control is real; Personalization records the approved personality
architecture and the owner's test profile without activating it.

**Personality architecture (approved, implementation separately scoped).**
Profiles: Executive & Strategic — Professional & Direct — Creative &
Exploratory — Friendly & Conversational — Technical & Analytical. Owner test
profile recorded: Communication style Friendly; Response detail Balanced;
Emojis when appropriate; Challenge assumptions Yes. Personality never
overrides Trust, permissions or factual integrity. Requires new persistence
and prompt integration — no implementation in Stage 1B.

**Access presentation.** A grant covering the canonical Windows user profile
is labeled "your user profile — profile-wide reach" wherever grants are
listed, so its reach is understandable without expanding or revoking
anything. A proposed change to the native confirmation dialog text for
profile-wide adds is reported separately for approval before any
security-related source is touched.

**Shared refinements.** Themed thin scrollbars; one pressed-feedback standard
(120ms, scale .97) and one focus ring (`--pd-focus`, 2px/2px offset) across
every interactive element; hairline separators.

## 12. Future requirement — adaptive conversation types (recorded, not implemented)

Ask PAIGE will support adaptive, user-selectable conversation types inside
one unified conversation experience: **Auto** (default intelligent
adaptation), Conversation, Work, Engineering, Research, Creative, Strategy.

Principles recorded as binding design intent:

- A user can switch the conversation type of an existing conversation without
  losing its history, context or artifacts.
- Conversation type, personality, execution mode, model selection and Trust
  authority are **distinct responsibilities** — changing one never implies
  another, and changing conversation type never expands permissions or
  bypasses approvals.
- The selected type eventually guides communication, specialist selection,
  skill discovery, model routing, authorized tool selection and output
  presentation — through the **original PAIGE architecture** (no duplicate
  agents, orchestration engines or Model Fabrics).

Status: design requirement for the **Stage 2 composer design planning**. The
type selector belongs in the unified composer; any runtime, persistence or
orchestration changes it needs (a persisted per-conversation type field; type
handoff through the existing ask pipeline) must be reconciled and proposed
before implementation authorization is requested. Not part of Stage 1B;
backend expansion hold unaffected.

## 13. Future observation — project-context Git reconciliation (recorded, not implemented)

During an owner conversation with an active pinned project, a Git inspection
call was issued with `root: "."` and rejected as unauthorized. The permission
enforcement was correct and stays exactly as strong; the defect is context
plumbing: the conversational execution layer should resolve tool requests
against the active project's canonical root instead of a relative working
directory. Recorded for subsequent conversational-execution and
project-context reconciliation. Do not weaken permission enforcement; no
scope change here.

## 14. Stage 2A — the unified composer (authoritative)

One consistent composer for new and existing conversations:
- **Context line above the input:** environment and project identity in one
  control (opens the existing project palette), the single access-truth chip
  (gold only for Full), and the standing note "files stay on this computer
  until you send" — the local-reference versus model-transmission boundary,
  stated where the choice is made.
- **Compact controls below the input:** + options menu, working-style menu,
  model, gateway health, Send/Stop. Advanced preferences live in menus.
- **Working style maps honestly to real behavior:** "Plan first" = the
  existing plan mode (read-only investigation); "Build directly" = the
  existing build mode (broker approvals unchanged). No "automatic" option is
  presented because none is implemented; the slot awaits Stage 2C.
- **Not rendered until their stages:** conversation-type control (2C),
  dictation (2D), paste/bulk/drag-drop (2B). The architecture accommodates
  them without redesign.
- **Welcome** (new conversation, no mandatory project): Command Mark, "What
  are we working on today?", four suggestions that each seed the composer —
  one genuine supported action each.
- Five responsibilities remain distinct; none of them — environment,
  project, type, style, model — ever expands authority.

## 15. Visual engineering — nine-step design loop

Use [Flow by Flow and Impeccable](UI-DESIGN.md) and the most relevant task-specific skill: Game UI/UX for hierarchy, fit and focus; Level Design for journeys and recovery; Animate for feedback and motion. Preserve SD-01/EAS-01 qualification and approval gates.

1. **Understand.** State the user goal, real entry point, current behavior and constraints. Inspect the existing tokens, components and native flow. Define success, refusal and recovery.
2. **Reference.** Consult the [design reference index](design-references/README.md). Name the pattern worth carrying forward and why it fits. A prototype, third-party example or existing screenshot is not automatically approved.
3. **Design.** Map the path and edge states. Prototype new or substantially changed journeys with safe fixtures in the existing Flow Prototype and owner review process. Label simulations and unresolved capabilities; request approval for bounded implementation.
4. **Implement.** Once authorized, take the smallest coherent slice using existing tokens, components, workflows and native approval paths. Do not add dependencies, authority or backend scope just to match a mockup.
5. **Inspect.** Exercise the actual flow. Capture safe before/after evidence from the compiled, installed Windows build. Check keyboard/focus, light and dark, window fit, 100/125/150% scaling and reduced motion where applicable. Browser and prototype checks supplement, but do not replace, installed Windows proof.
6. **Critique.** Compare with the goal and accepted baseline: hierarchy, density, copy, accessibility, motion, and truthful state and permission feedback. Separate observations from guesses and state what the evidence cannot establish.
7. **Refine.** Fix the highest-impact findings in scope and repeat affected checks, including failure and recovery. Return for authorization if a correction needs new capability or authority.
8. **Independent review.** Give a non-author the exact revision, diff, tests, safe evidence and limitations. Resolve findings and verify the affected paths again. Keep independent clearance distinct from explicit owner acceptance before the next stage.
9. **Capture owner-approved lessons.** Record the accepted pattern and its context, rejected alternatives and why, and the exact owner decision. Add only safe, expressly approved evidence to the index; implementation and reviewer opinions are not standing owner preferences.

## 16. Permanent test isolation and pre-send checks

- Use an isolated test profile or harness with separate chat/draft state and synthetic fixtures in an authorized lab project or worktree. Verify isolation before any test write or send. A new chat in the owner's active profile is not isolation. If isolation is unverified, stop before the send or mutation and report the blocker.
- Immediately before **each** test send, identify the test profile, exact conversation (record ID or verified new draft with no existing record), expected project or **No project**, and destination/model and payload. Recheck after switching, reopening or retrying; stop on ambiguity or mismatch. Never send into an existing owner conversation. External model testing requires separate authorization for the destination and synthetic payload.
- Preserve owner history, drafts, projects, preferences (including theme, startup and model settings), and permissions. Do not delete or rewrite existing records, even accidental test turns or old empty records, without explicit authority. Exercise preference and permission scenarios only in approved isolated state. Clean up only disposable test state you are authorized to remove.
- The native Scope Registry remains the authority; environment, project, conversation type, working style, model selection and attachments never expand it. Preserve native approvals and revocation; never impersonate the owner. Verify switched and reopened chats cannot use stale broker context from another project. Do not claim OS-level subprocess confinement.

## 17. Design reference library and tool qualification

The [reference index](design-references/README.md) separates accepted patterns, proposals and exclusions. Start with this standard and the accepted revision's tokens and components. Record provenance, exact approval scope, the lesson and use limits. Keep private chats, screenshots, filenames and access details out of the library. Prefer a safe text pointer or a synthetic recreation labeled illustrative, not historical runtime evidence. Approval of a feature or review of a screenshot does not approve reuse of that image.

Retain [Taste Skill](https://github.com/Leonxlnx/taste-skill) as a candidate for future, separately authorized web, funnel or app work, not a Stage 2B dependency or a replacement for current methods. Link upstream without copying instructions. Figma and Figma MCP are deferred. Qualify future skills and integrations through the existing paths; developer tools are distinct from user-facing Ask PAIGE capabilities. This standard does not authorize installation, spending, a provider or routing change, or external data transfer.

## 18. Stage 2B — attachment visual standards (planning only)

Use the unified composer and existing PAIGE tokens and focus patterns. A clearly bounded drop target shows what release will do without submitting the draft. Keep picker and keyboard paths alongside Explorer drag/drop and screenshot/file clipboard paste; distinguish attachment paste from ordinary text. Show bulk staging as one draft action with per-item outcomes. Render controls only for behavior supported by the installed build.

- **Recognizable, removable items.** Show a local image thumbnail when available, otherwise a typed icon and readable label. Use compact document/file cards and a distinct folder-reference label where supported. A folder reference is not implicit enumeration, ingestion, upload or a grant. Keep items bound to their draft so chat switches cannot silently carry them into another send. Remove excludes the item from the draft; it does not delete the original.
- **Truthful progress and recovery.** Label queued, processing, ready, unsupported and failed states from actual outcomes. Ready is not sent or understood. Show partial bulk outcomes and per-item reasons; offer retry only for recoverable failures, without duplicates or implicit send. Do not silently omit failures or present filenames as transmitted contents. Keep controls keyboard-accessible, focus visible and status changes announced; color and motion cannot be the only cues.
- **Make the boundary visible.** Distinguish **local reference** (only a local link/identifier), **local processing** (e.g., an on-device thumbnail), and **authorized external transmission** through the existing send/approval path. Attach, paste, drop, preview and local retry must not transmit content. Before an external send, show the destination and whether originals, derived contents or metadata will be sent, or whether no attachment data will leave. Do not promise local-only handling if any of those leave the device, or claim delivery without evidence. For an unsupported route, explain the limitation rather than uploading elsewhere or implying the model inspected the item.
- **Verify, do not infer.** Test supported picker/paste/drop paths, mixed batches, remove/retry, draft switching and no transmission before Send in the isolated installed Windows build. Record verified format/count/size and route limits; do not invent numbers or capabilities. Screenshot capture and thumbnails do not prove model visual understanding. The Stage 2B preview is a planning fixture, not runtime evidence.

Future **qualified vision QA** belongs to [ANT-175](https://linear.app/paigeagentai/issue/ANT-175), **not implemented here**. Start with an owner-selected safe screenshot and a verified vision-capable route; obtain explicit consent for capture scope, retention and any external processing. Use existing Work/Harness/Trust/Model Fabric and native approvals for one bounded correction. Verify actual installed before/after and interactions, with independent review and owner acceptance. Video and broader Windows capture or operations require separate qualification and authorization. No parallel vision engine or routing change is implied.
