{
  description = "mdict-web development shell, package, and NixOS module";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    let
      nixosModule = import ./nix/module.nix { inherit self; };
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        lib = pkgs.lib;
        version = "0.1.0";

        cleanRepo = lib.cleanSourceWith {
          src = ./.;
          filter =
            path: _type:
            let
              rel = lib.removePrefix "${toString ./.}/" (toString path);
            in
            !(
              rel == ".direnv"
              || lib.hasPrefix ".direnv/" rel
              || rel == "target"
              || lib.hasPrefix "target/" rel
              || rel == "result"
              || lib.hasPrefix "result/" rel
              || rel == "frontend/node_modules"
              || lib.hasPrefix "frontend/node_modules/" rel
              || rel == "frontend/dist"
              || lib.hasPrefix "frontend/dist/" rel
            );
        };

        frontendSrc = lib.cleanSourceWith {
          src = ./frontend;
          filter =
            path: _type:
            let
              rel = lib.removePrefix "${toString ./frontend}/" (toString path);
            in
            !(
              rel == "node_modules"
              || lib.hasPrefix "node_modules/" rel
              || rel == "dist"
              || lib.hasPrefix "dist/" rel
            );
        };

        cargoSrc = pkgs.runCommand "mdict-web-src" { } ''
          cp -r ${cleanRepo} $out
          chmod -R u+w $out
          sed -i 's#mdict-rs = { version = "0.1.4", path = "../mdict-rs" }#mdict-rs = { version = "0.1.4" }#' \
            "$out/Cargo.toml"
          if ! grep -Fq 'checksum = "b0affda41b39511d522de4474f6191d7a6f5571b42a1e09a5c552bd265b4c917"' "$out/Cargo.lock"; then
            awk '
              { print }
              $0 == "name = \"mdict-rs\"" { in_mdict = 1; next }
              in_mdict && $0 == "version = \"0.1.4\"" {
                print "source = \"registry+https://github.com/rust-lang/crates.io-index\""
                print "checksum = \"b0affda41b39511d522de4474f6191d7a6f5571b42a1e09a5c552bd265b4c917\""
                in_mdict = 0
              }
            ' "$out/Cargo.lock" > "$out/Cargo.lock.tmp"
            mv "$out/Cargo.lock.tmp" "$out/Cargo.lock"
          fi
        '';

        frontend = pkgs.stdenvNoCC.mkDerivation {
          pname = "mdict-web-frontend";
          inherit version;
          src = frontendSrc;

          nativeBuildInputs = [
            pkgs.nodejs
            pkgs.pnpm
            pkgs.pnpmConfigHook
          ];

          pnpmDeps = pkgs.fetchPnpmDeps {
            pname = "mdict-web-frontend";
            inherit version;
            src = frontendSrc;
            fetcherVersion = 1;
            hash = "sha256-0Rkgl8uBJ5ZuTcQnLXVMSygDL7U/MSYxjepiqzI+dl4=";
          };

          buildPhase = ''
            runHook preBuild
            pnpm build
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p "$out"
            cp -r dist "$out/dist"
            runHook postInstall
          '';
        };

        mdict-web = pkgs.rustPlatform.buildRustPackage {
          pname = "mdict-web";
          inherit version;
          src = cargoSrc;
          cargoHash = "sha256-KBwf3CuYTcjheV1EehGsaKEJEEJrxyYNmaN0fXKx77o=";
          cargoBuildFlags = [
            "-p"
            "mdict-web-app"
          ];
          doCheck = false;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postInstall = ''
            mkdir -p "$out/share/mdict-web/frontend"
            cp -r ${frontend}/dist "$out/share/mdict-web/frontend/dist"

            makeWrapper "$out/bin/mdict-web-app" "$out/bin/mdict-web" \
              --set-default MDICT_WEB_FRONTEND_DIST "$out/share/mdict-web/frontend/dist"
          '';

          meta = with lib; {
            description = "High-performance, safe, API-first Rust MDict web server";
            license = licenses.agpl3Only;
            platforms = platforms.linux;
            mainProgram = "mdict-web";
          };
        };
      in
      {
        packages = {
          default = mdict-web;
          mdict-web = mdict-web;
          mdict-web-frontend = frontend;
        };

        apps.default = {
          type = "app";
          program = "${mdict-web}/bin/mdict-web";
        };

        checks = {
          inherit frontend mdict-web;
        };

        formatter = pkgs.nixfmt;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            nodejs
            pnpm
          ];

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };
      }
    )
    // {
      nixosModules.default = nixosModule;
      nixosModules.mdict-web = nixosModule;
    };
}
