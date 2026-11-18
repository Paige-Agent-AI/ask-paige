# PAIGE Desktop

Windows-first local execution surface for PAIGE - a governed command center
where one accountable lead agent coordinates bounded coding, research and
independent review workers.

**Status:** Increment 0 prototype. Not a production build. Not distributed.

## Stack

- **Shell:** Tauri 2 (Rust native host) + React 19 + TypeScript + Vite
- **Native broker:** Rust, least-privilege file/process access (Increment 1+)
- **Inference:** direct OpenRouter, dedicated Desktop credential (Increment 1+)

Pinned versions: React 19.1.0, TypeScript 6.0.3, Vite 8, Rust edition 2024,
Tauri 2.7.6, MSVC 14.44.35207, Windows SDK 10.0.26100.0.

## Security status

**This shell is not hardened.** `tauri.conf.json` currently sets `"csp": null`,
so there is no Content Security Policy. No native broker, no permission
enforcement and no approval gate exist yet. Do not load untrusted content into
this shell until CSP and the broker land.

Known limitations are enumerated in `docs/INCREMENT-0-EVIDENCE.md` §7.

## Scope boundaries

This repository is an independent prototype. It is not a worktree of the
production Paige-Agent-AI repository and has no remote. It consumes PAIGE's
orchestration contracts as a client; it does not implement a second
orchestration engine.

NEXUS Vibe Studio creative and model routers are out of scope and unchanged.

## Development

    npm install
    npm run tauri dev      # requires Rust + MSVC Build Tools
    npm run tauri build    # produces an unsigned local bundle
