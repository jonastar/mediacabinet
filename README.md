# Media Cabinet

A self-hosted media library manager and streaming server. See [docs/decisions.md](docs/decisions.md) for the stack, and `media-cabinet-handoff/` for the product spec and designs.

## Layout

| Path | What it is |
| --- | --- |
| `crates/core` | Library model, database and migrations (`crates/core/migrations`), later the scanner and matcher |
| `crates/server` | The `media-cabinet` binary: HTTP API, OpenAPI spec, embedded web UI |
| `web` | React + TypeScript web UI (Vite) |
| `docs` | Project decisions |
| `media-cabinet-handoff` | Original spec and design handoff, for reference |

## Development

Requirements: Rust 1.97+, Node 24+, pnpm. `ffmpeg`/`ffprobe` once scanning lands.

```sh
# Backend on :8420, data in ./data
cargo run -p media-cabinet

# Web UI with hot reload on :5173, proxying /api to the backend
cd web && pnpm install && pnpm dev
```

A fake library for testing (about 40 short clips covering the spec's layouts, naming styles, editions, versions, sidecars and junk; needs ffmpeg):

```sh
scripts/make-fixtures.sh            # writes fixtures/media (gitignored); --force to regenerate
```

After changing API handlers, regenerate the frontend types:

```sh
cd web && pnpm api:types   # writes src/api/openapi.json and src/api/schema.d.ts
```

Checks:

```sh
cargo fmt --check && cargo test && cargo clippy --all-targets
cd web && pnpm format:check && pnpm lint && pnpm build
```

## Code style

Formatting is enforced by `cargo fmt` (Rust) and `pnpm format` (oxfmt and oxlint, web). Run them before committing. The pre-commit hook in `.githooks/` rejects unformatted code; enable it once per clone:

```sh
git config core.hooksPath .githooks
```

The rules beyond the formatters' defaults are listed in [CLAUDE.md](CLAUDE.md#code-style).

## Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `CABINET_DATA_DIR` | `./data` | Database and backups |
| `CABINET_BIND` | `0.0.0.0:8420` | Listen address |
| `RUST_LOG` | `info` | Log filter |

The database is backed up to `cabinet.db.<timestamp>.bak` before any migration runs.

## Building

**Binary.** The UI is embedded at compile time, so build it first:

```sh
(cd web && pnpm install && pnpm build)
cargo build --release
./target/release/media-cabinet
```

If `web/dist` is missing when the server compiles, the binary still runs but serves a "Web UI not built" message. Rebuild the server after building the UI.

**Docker.** Includes ffmpeg.

```sh
docker build -t media-cabinet .
docker run -p 8420:8420 -v cabinet-data:/data -v /path/to/media:/media:ro media-cabinet
```
