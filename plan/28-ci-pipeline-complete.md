# 28 - CI Pipeline

## Goal
Set up continuous integration that catches build failures, lint issues, and test regressions for both Rust and Web UI code. Platform-agnostic design per CICD_DESIGN.md.

## What to build

### CI workflow (e.g. `.github/workflows/ci.yml` if GitHub Actions)

**Rust job:**
- Trigger: push to main, pull requests
- Steps:
  1. `cargo fmt --all -- --check` — format check
  2. `cargo clippy --all-targets --all-features -- -D warnings` — lint
  3. `cargo build --all-targets` — build
  4. `cargo test --all-targets` — test
- Cache: `~/.cargo/registry`, `target/`
- Rust toolchain from `rust-toolchain.toml`

**Web UI job:**
- Trigger: same as Rust
- Steps:
  1. `npm ci` — install deps
  2. `npm run lint` — ESLint
  3. `npm run typecheck` — TypeScript check (tsc --noEmit)
  4. `npm run build` — production build
  5. `npm run test` — unit tests (Vitest)
- Cache: `node_modules/`

**Integration test job (optional, may need DB):**
- Start PostgreSQL (service container or docker-compose)
- Run integration tests: `cargo test --test integration`
- Depends on Rust build

### Tooling configuration

**Rust:**
- `rust-toolchain.toml` with stable channel + pinned version
- `.rustfmt.toml` with project formatting preferences
- `clippy.toml` or workspace-level clippy config

**Web:**
- ESLint config (`.eslintrc.js` or `eslint.config.js`)
- Prettier (optional, or rely on ESLint formatting rules)
- TypeScript strict mode in `tsconfig.json`
- Vitest config in `vite.config.ts`

### Future: OpenAPI drift detection
- Stub for later: CI step that fails when OpenAPI spec and server handlers diverge
- Placeholder comment in CI config: "TODO: Add OpenAPI drift check when spec is checked in"

## Dependencies
- All Rust crates must compile (Tasks 01-21)
- Web UI must build (Tasks 22-25)

## Test criteria
- [ ] CI runs on push to main
- [ ] CI runs on pull requests
- [ ] `cargo fmt` check passes (code is formatted)
- [ ] `cargo clippy` passes with no warnings
- [ ] `cargo build` succeeds
- [ ] `cargo test` all tests pass
- [ ] `npm run lint` passes
- [ ] `npm run typecheck` passes
- [ ] `npm run build` succeeds
- [ ] `npm run test` passes
- [ ] CI caches work (second run faster than first)
- [ ] Failing test causes CI to fail (verify red path)
