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

## 11. Stage 1B � premium experience standards (authoritative)

**Cinematic awakening.** One gold light, the actual Command Mark, the wordmark
"Ask PAIGE", tagline "Your intelligent Windows workstation". Pure
transform/opacity keyframes; no media dependency; audio signature reserved
(separately gated). Three startup experiences, a stored preference
(`paige.startup`): Cinematic (~4s), Express (~1.2s), Instant (none). The
overlay auto-enters; Skip and Escape enter now; reduced-motion shows the same
information as calm crossfades. Application readiness is never blocked or
faked � the shell initializes underneath and measurements are taken
independently of presentation duration.

**Appearance model.** Two primary choices, Light and Dark; "Match Windows
automatically" is one optional preference, not a competing theme button. The
stored value keeps its previous meaning (light/dark/system) � migration is
honest and documented; nothing silently changes.

**Conversation foundations.** Human turns: right-aligned surface with a
4px-corner radius asymmetry. Assistant turns: full-width prose, one Markdown
hierarchy (18/16.5/15 headings, hairline tables, mono code), identity by
label and alignment rather than chrome. Existing actions keep behavior;
presentation only. New reactions/artifact rendering remain gated to their
stages.

**Settings architecture.** Category navigation: Personal (Personalization) �
Application (Appearance, General, Skills) � Trust and data (Privacy &
Security: workspace access + Computer Access) � Information (About). Every
listed control is real; Personalization records the approved personality
architecture and the owner's test profile without activating it.

**Personality architecture (approved, implementation separately scoped).**
Profiles: Executive & Strategic � Professional & Direct � Creative &
Exploratory � Friendly & Conversational � Technical & Analytical. Owner test
profile recorded: Communication style Friendly; Response detail Balanced;
Emojis when appropriate; Challenge assumptions Yes. Personality never
overrides Trust, permissions or factual integrity. Requires new persistence
and prompt integration � no implementation in Stage 1B.

**Access presentation.** A grant covering the canonical Windows user profile
is labeled "your user profile � profile-wide reach" wherever grants are
listed, so its reach is understandable without expanding or revoking
anything. A proposed change to the native confirmation dialog text for
profile-wide adds is reported separately for approval before any
security-related source is touched.

**Shared refinements.** Themed thin scrollbars; one pressed-feedback standard
(120ms, scale .97) and one focus ring (`--pd-focus`, 2px/2px offset) across
every interactive element; hairline separators.

## 12. Future requirement � adaptive conversation types (recorded, not implemented)

Ask PAIGE will support adaptive, user-selectable conversation types inside
one unified conversation experience: **Auto** (default intelligent
adaptation), Conversation, Work, Engineering, Research, Creative, Strategy.

Principles recorded as binding design intent:

- A user can switch the conversation type of an existing conversation without
  losing its history, context or artifacts.
- Conversation type, personality, execution mode, model selection and Trust
  authority are **distinct responsibilities** � changing one never implies
  another, and changing conversation type never expands permissions or
  bypasses approvals.
- The selected type eventually guides communication, specialist selection,
  skill discovery, model routing, authorized tool selection and output
  presentation � through the **original PAIGE architecture** (no duplicate
  agents, orchestration engines or Model Fabrics).

Status: design requirement for the **Stage 2 composer design planning**. The
type selector belongs in the unified composer; any runtime, persistence or
orchestration changes it needs (a persisted per-conversation type field; type
handoff through the existing ask pipeline) must be reconciled and proposed
before implementation authorization is requested. Not part of Stage 1B;
backend expansion hold unaffected.

## 13. Future observation � project-context Git reconciliation (recorded, not implemented)

During an owner conversation with an active pinned project, a Git inspection
call was issued with `root: "."` and rejected as unauthorized. The permission
enforcement was correct and stays exactly as strong; the defect is context
plumbing: the conversational execution layer should resolve tool requests
against the active project's canonical root instead of a relative working
directory. Recorded for subsequent conversational-execution and
project-context reconciliation. Do not weaken permission enforcement; no
scope change here.
