# PAIGE Desktop

Windows-first local execution surface for PAIGE - a governed command center
where one accountable lead agent coordinates bounded coding, research and
independent review workers.

**Status:** Increment 0 prototype. Not a production build. Not distributed.

## Stack

- **Shell:** Tauri 2 (Rust native host) + React 19 + TypeScript + Vite
- **Native broker:** Rust, least-privilege file/process access (Increment 1+)
- **Inference:** direct OpenRouter, dedicated Desktop credential (Increment 1+)

Resolved versions (these are **not** pins — versions are caret/tilde ranges;
lockfiles are the actual pinning mechanism):

| Component | Declared | Resolved |
|---|---|---|
| React / ReactDOM | `^19.1.0` | **19.3.1** |
| TypeScript | `~6.0.3` | 6.0.3 |
| Vite | `^8.0.16` | **8.3.8** |
| `tauri` crate | `2` | **2.12.7** |
| `tauri-build` | `2` | 2.7.6 |
| Rust edition | — | 2024 |
| Rust channel | pinned by `rust-toolchain.toml` | 1.100.0 |
| MSVC toolset | — | 14.44.35207 |
| Windows SDK | — | 10.0.26100.0 |

## Security status (Increment 1A)

- **CSP is now enforced.** Production policy is `default-src 'self'`,
  `script-src 'self'`, `style-src 'self'`, `connect-src 'self' ipc:
  http://ipc.localhost`, `object-src 'none'`, `frame-ancestors 'none'`, etc.
  No `unsafe-eval`, no `unsafe-inline`, no external origins, no wildcard.
- **Capability set is minimal:** `core:event:default`, `core:window:default`,
  `core:app:default`. No fs, shell, http, opener, process, dialog, clipboard,
  os, webview-creation or resources permissions.
- **No Tauri plugins** (opener removed; no fs/shell/http/opener crates or npm
  packages).
- Development relaxations live only in `devCsp` and never weaken production.
- **Still not hardened:** no native broker, no approval gate, no file/terminal
  access, `npm audit` is a snapshot. Details and test results:
  `docs/INCREMENT-1A-EVIDENCE.md`.

Known limitations: `docs/INCREMENT-0-EVIDENCE.md` §7.

Known limitations are enumerated in `docs/INCREMENT-0-EVIDENCE.md` §7.

## Scope boundaries

This repository is an independent prototype. It is not a worktree of the
production Paige-Agent-AI repository and has no remote.

**Intended future role:** consume PAIGE's orchestration contracts as a client.
Nothing is implemented yet — there is no contract client, task-state adapter or
agent protocol in this repository. It will not implement a second orchestration
engine.

NEXUS Vibe Studio creative and model routers are out of scope and unchanged.

## Development

    npm ci
    npm run tauri dev      # requires Rust + MSVC Build Tools
    npm run tauri build    # produces an unsigned local bundle
