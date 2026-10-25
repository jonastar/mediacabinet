# Media Cabinet

Self-hosted media library manager and streaming server. Rust backend (`crates/`), React + TypeScript web UI (`web/`). The stack and its reasons are in `docs/decisions.md`; the product spec and designs are in `media-cabinet-handoff/`.

## Commands

- Backend: `cargo run -p media-cabinet` (port 8420), `cargo test`, `cargo clippy --all-targets`
- Web: `cd web && pnpm dev` (proxies `/api` to 8420), `pnpm build`, `pnpm lint`
- After changing API handlers: `cd web && pnpm api:types`
- Format before committing: `cargo fmt` and `cd web && pnpm format`

## Code style

- Never put the body of an `if`, `else`, `for`, `while` or `let ... else` on the same line as its condition. Always braces, body on its own line:

  ```ts
  if (ready) {
    start()
  }
  ```

  This also applies to Rust `if`/`else` used as an expression (`let v = if a { 1 } else { 2 };` is not allowed). Enforced by `rustfmt.toml` and oxlint `curly: all` plus oxfmt.
- Web: no semicolons, single quotes, 100-column lines (oxfmt).
- Rust: rustfmt defaults otherwise.
