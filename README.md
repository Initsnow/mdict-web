# mdict-web

High-performance, safe, API-first Rust MDict web server built on top of `mdict-rs`.

## Current shape

- Rust backend in `crates/`
- React + Vite frontend in `frontend/`
- Entry HTML is served separately from JSON metadata and rendered in a sandboxed iframe
- When a frontend dist is available, `axum` can serve it directly on the same origin
- `flake.nix` exports `packages.${system}.mdict-web`, `packages.${system}.default`, `apps.${system}.default`, and `nixosModules.default`

## Local development

Backend:

```bash
cargo run -p mdict-web-app -- --config ./mdict-web.toml
```

Frontend dev server:

```bash
pnpm --dir frontend install
pnpm --dir frontend dev
```

During frontend development, prefer the Vite dev server. For integrated deployment, build `frontend/dist` and let `axum` host it:

```bash
pnpm --dir frontend build
cargo run -p mdict-web-app -- --config ./mdict-web.toml
```

You can override or disable static hosting with `--frontend-dist`, `MDICT_WEB_FRONTEND_DIST`, `--no-frontend`, or `MDICT_WEB_DISABLE_FRONTEND=true`.

## Nix package

Build the deployable package:

```bash
nix build .#mdict-web
```

Run it directly:

```bash
nix run .#mdict-web -- --config ./mdict-web.toml
```

The packaged `mdict-web` wrapper automatically points `MDICT_WEB_FRONTEND_DIST` at the bundled `frontend/dist`, so the same process serves API and frontend together by default.

## NixOS module

The flake exports `nixosModules.default`. A minimal configuration:

```nix
{
  imports = [ inputs.mdict-web.nixosModules.default ];

  services.mdict-web = {
    enable = true;
    settings = {
      server.bind = "0.0.0.0:8080";
      index.dir = "/var/lib/mdict-web/index";
      catalog.bundles = [
        {
          dictionary_id = "ldoce5pp";
          display_name = "LDOCE5++";
          mdx_path = "/srv/dictionaries/LDOCE5++ V 2-15.mdx";
          mdd_path = "/srv/dictionaries/LDOCE5++ V 2-15.mdd";
          entry_script_mode = "none";
          theme_mode = "auto";
        }
      ];
    };
  };
}
```

Notes:

- Prefer absolute dictionary paths in NixOS configs.
- If `settings` is used, `index.dir` defaults to `/var/lib/mdict-web/index`.
- If you already manage `mdict-web.toml` yourself, set `services.mdict-web.configFile` instead.
- `services.mdict-web.frontendDist` overrides the packaged dist, and `services.mdict-web.noFrontend = true` disables frontend hosting entirely.

## Validation

```bash
cargo fmt --all
cargo test --workspace
cargo bench -p mdict-web-app --bench lookup -- --sample-size 10 --warm-up-time 0.1 --measurement-time 0.1
pnpm --dir frontend build
nix build .#mdict-web
```

## License

This repository is `AGPL-3.0-only`. `mdict-rs` licensing constraints apply to deployment and distribution.
