# web-deploy: Trunk Release Build + GitHub Pages

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06). **Rescoped
> 2026-10-08:** the site goes to GitHub Pages here; the Cloudflare deploy moved
> to [web-deploy-cf](web-deploy-cf.md), which publishes the same build.

## Goal

Ship `calculator-web` to a public URL with a repeatable build, and document the
web build for users.

## Design

- **Release build:** `trunk build --release --cargo-profile wasm-release
  --public-url ./` in `crates/web`.
  - `[profile.wasm-release]` (root `Cargo.toml`) inherits `release` with
    `opt-level = "z"`, LTO and one codegen unit. It is a separate profile so
    the native release build is untouched.
  - `data-wasm-opt="z"` on the Trunk link in `index.html` runs wasm-opt, in
    release builds only.
- **`--public-url ./`:** every asset URL in `dist/` is relative. One build then
  works under Pages' `/tui-calculator/` subpath and at a host's root
  (Cloudflare, `web-deploy-cf`).
- **Deploy:** `.github/workflows/deploy-web.yml` runs on a push to `main` or by
  hand.
  - A `build` job installs the wasm target and a pinned Trunk release binary,
    builds, and uploads `dist/` with `actions/upload-pages-artifact`.
  - A `pages` job runs `actions/deploy-pages`.
  - The repo's Pages source is set to **GitHub Actions**
    (`build_type=workflow`); there is no `gh-pages` branch.
  - Splitting build from publish lets `web-deploy-cf` add a second publish job
    for the same artifact.
- **Favicon:** an inline SVG data URI in `index.html`, so there's no extra file
  and no 404.
- **README:** URL, how to run locally, and the web differences.

## How to Verify

After merge, the workflow goes green and https://lix42.github.io/tui-calculator/
loads. Repeat `web-entry`'s smoke test there: type, `=`, `y` copies, clicks,
resize, and no console errors. Before merge, serving `dist/` under a
`/tui-calculator/` subpath locally checks the relative URLs.

## Dependencies

- [web-entry](web-entry.md)
