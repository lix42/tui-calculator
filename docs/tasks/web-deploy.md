# web-deploy: Trunk Release Build + Cloudflare (Workers Static Assets)

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06).

## Goal

Ship `calculator-web` to a public Cloudflare URL with a repeatable
build, and document the web build for users.

## Design

- Trunk `index.html` (monospace web font, dark background) + `Trunk.toml` if
  needed; `trunk build --release` → `dist/` (static HTML + JS glue + `.wasm`).
- **Workers Static Assets, not Pages** (changed after `web-spike` Q7): wrangler
  4.148 delegates `pages` commands to Workers and Cloudflare's docs steer static
  sites there. An assets-only `wrangler.jsonc` (`name`, `compatibility_date`,
  `assets.directory = "./dist"`, no `main`) + `wrangler deploy`. Proven by the
  spike at `tui-calculator-spike.i-70e.workers.dev`. Delete that throwaway
  Worker once this ships.
- **Deploy path (recommended):** a GitHub Action in `.github/workflows/` that
  installs the wasm target + Trunk, builds, and runs `wrangler deploy` (API
  token + account id as repo secrets). Preferred over Workers Builds' Git
  integration, whose build image has no Rust/Trunk toolchain. *Not yet
  confirmed by Lix — settle before starting.*
- Add a favicon (the spike's only console error was its 404).
- Size: release profile + `wasm-opt` (Trunk `data-wasm-opt`); record the final
  `.wasm` size.
- **README:** a web section — URL, how to run locally (`trunk serve`), and the
  web differences (`q` / Ctrl-C do nothing; clipboard needs the page focused;
  paste support per `web-paste`).

## How to Verify

Load the deployed URL and repeat `web-entry`'s smoke test against the deployed build (retry briefly — assets can 404 for ~10–30 s right
after a deploy).
A push to `main` (or whatever trigger is chosen) redeploys.

## Dependencies

- [web-entry](web-entry.md)
