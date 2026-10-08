# web-deploy-cf: Cloudflare Workers Static Assets

> Sub-task of [web-ratzilla](web-ratzilla.md), split out of
> [web-deploy](web-deploy.md) on 2026-10-08.

## Goal

Publish the same `dist/` that `web-deploy` sends to GitHub Pages to Cloudflare as
well, at `tui-calculator.<account>.workers.dev`. Mapping a custom domain (DNS)
comes later.

## Design

- **Workers Static Assets, not Pages** (`web-spike` Q7): wrangler 4.148
  delegates `pages` commands to Workers. Use an assets-only
  `crates/web/wrangler.jsonc` with no `main`:

  ```jsonc
  {
    "name": "tui-calculator",
    "compatibility_date": "<date>",
    "assets": { "directory": "./dist" }
  }
  ```

  Never point `assets.directory` at a directory that contains `target/`. Debug
  `.wasm` files exceed the 25 MiB per-file limit.
- **Deploy:** a second job in `.github/workflows/deploy-web.yml`. It needs
  `build`, downloads the same artifact (`github-pages`, a tar; extract it, or
  upload `dist/` a second time under its own name), and runs
  `cloudflare/wrangler-action@v4` (`workingDirectory: crates/web`,
  `command: deploy`).
- **Secrets (Lix):**
  - `CLOUDFLARE_API_TOKEN`: create one from the "Edit Cloudflare Workers"
    template.
  - `CLOUDFLARE_ACCOUNT_ID`: from `wrangler whoami`.
- Gitignore `crates/web/.wrangler`. `wrangler deploy` also appends entries to
  the nearest `.gitignore`.
- Delete the throwaway `tui-calculator-spike` Worker once this is live.
- README: add the Cloudflare URL next to the Pages one.

## How to Verify

The workflow's Cloudflare job goes green. The workers.dev URL passes
`web-entry`'s smoke test; retry for 10–30 s first, because assets can return 404
just after a deploy.

## Dependencies

- [web-deploy](web-deploy.md)
