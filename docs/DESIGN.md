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
