# 01 - Repo Scaffolding & Build Setup

## Goal
Create the Cargo workspace, all crate skeletons, Web UI project, and basic build/run infrastructure so every subsequent task has a compilable home.

## What to build
- **Cargo.toml** (workspace root) with members: `crates/server`, `crates/worker`, `crates/cli`, `crates/api-types`
- **crates/api-types/** — empty lib crate with `serde`, `serde_json`, `chrono`, `uuid` deps
- **crates/server/** — binary crate with `axum`, `tokio`, `sqlx` (postgres), `tracing`, `tower`, `serde` deps; `main.rs` that prints "server starting"
- **crates/worker/** — binary crate with `tokio`, `reqwest`, `git2`, `tracing`, `serde` deps; `main.rs` that prints "worker starting"
- **crates/cli/** — binary crate with `clap`, `reqwest`, `tokio`, `serde` deps; `main.rs` with clap skeleton
- **web/** — `npm create vite@latest` with React + TypeScript template; add Tailwind CSS + shadcn/ui setup; verify `npm run dev` and `npm run build` work
- **rust-toolchain.toml** — pin stable Rust version
- **.gitignore** — Rust target/, node_modules/, .env, etc.
- **README.md** — minimal "what this is, how to build" pointing to docs/

## Dependencies
None — this is the first task.

## Design decisions
- Use SQLx with compile-time checking disabled initially (runtime mode) until migrations exist
- Web UI: Vite + React (no Next.js) per spec requirement for client-only SPA
- All crates share the workspace `Cargo.lock`

## Test criteria
- [ ] `cargo build --workspace` succeeds with no errors
- [ ] `cargo test --workspace` succeeds (no tests yet, but compiles)
- [ ] `cd web && npm run build` succeeds
- [ ] Each binary runs and exits cleanly (`cargo run -p server`, `cargo run -p worker`, `cargo run -p cli -- --help`)
