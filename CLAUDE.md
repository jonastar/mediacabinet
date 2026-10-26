# Media Cabinet

Self-hosted media library manager and streaming server. Rust backend (`crates/`), React + TypeScript web UI (`web/`). The stack and its reasons are in `docs/decisions.md`; progress toward v1 is tracked in `docs/roadmap.md` (tick items when they land); the product spec and designs are in `media-cabinet-handoff/`.

## Commands

- Backend: `cargo run -p media-cabinet` (port 8420), `cargo test`, `cargo clippy --all-targets`
- Web: `cd web && pnpm dev` (proxies `/api` to 8420), `pnpm build`, `pnpm lint`
- After changing API handlers: `cd web && pnpm api:types`
- Format before committing: `cargo fmt` and `cd web && pnpm format`

## Database

- Migrations in `crates/core/migrations` are append-only: never edit an applied one (existing databases fail their checksum). Add `NNNN_name.sql`.
- Derived state (availability, wanted, gaps, next up) is computed, never stored.

## API

- Auth is deny-by-default in one place, `crates/server/src/api/auth.rs`. A new write endpoint is admin-only unless `is_own_change` lists it, and admin-only reads go in `ADMIN_READS`. Per-user handlers take the `Viewer` extractor, never "the default user".

## Web UI conventions

- Plain React: routing in `web/src/lib/router.ts` (+ `components/Link.tsx`), data via `api/useApi.ts`, typed calls in `api/client.ts`. Add libraries only when needed.
- Styles are CSS Modules using the design tokens in `index.css`; reference designs are in `media-cabinet-handoff/design/screens/Hybrid*.dc.html`.
- State wording per Watch/Manage mode lives in `lib/wording.ts`; controls for features not built yet are shown disabled with a title. Only admins get Manage mode (`useMode`).
- Try the UI with demo data: `CABINET_DATA_DIR=./data-demo cargo run -p media-cabinet -- seed-demo`, then run the server and `pnpm dev`. The first visit asks for an account; it takes over the demo's admin and its watch history.

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
