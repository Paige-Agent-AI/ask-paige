# PAIGE Desktop

Windows-first local execution surface for PAIGE - a governed command center
where one accountable lead agent coordinates bounded coding, research and
independent review workers.

**Status:** Increment 0 prototype. Not a production build. Not distributed.

## Stack

- **Shell:** Tauri 2 (Rust native host) + React 18 + TypeScript + Vite
- **Native broker:** Rust, least-privilege file/process access (Increment 1+)
- **Inference:** direct OpenRouter, dedicated Desktop credential (Increment 1+)

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
