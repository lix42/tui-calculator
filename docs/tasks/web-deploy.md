# web-deploy: Trunk Release Build + Cloudflare Pages

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06).

## Goal

Ship `calculator-web` to a public Cloudflare Pages URL with a repeatable
build, and document the web build for users.

## Design

- Trunk `index.html` (monospace web font, dark background) + `Trunk.toml` if
  needed; `trunk build --release` → `dist/` (static HTML + JS glue + `.wasm`).
- **Cloudflare Pages**, not a Worker — the build is pure static assets.
- **Deploy path (recommended):** a GitHub Action in `.github/workflows/` that
  installs the wasm target + Trunk, builds, and runs `wrangler pages deploy dist`
  (API token + account id as repo secrets). Preferred over Pages' Git integration
  because Pages' build image has no Rust/Trunk toolchain. *Not yet confirmed by
  Lix — settle before starting.*
- Size: release profile + `wasm-opt` (Trunk `data-wasm-opt`); record the final
  `.wasm` size.
- **README:** a web section — URL, how to run locally (`trunk serve`), and the
  web differences (`q` / Ctrl-C do nothing; clipboard needs the page focused;
  paste support per `web-paste`).

## How to Verify

Load the Pages URL and repeat `web-entry`'s smoke test against the deployed build.
A push to `main` (or whatever trigger is chosen) redeploys.

## Dependencies

- [web-entry](web-entry.md)
