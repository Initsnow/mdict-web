{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.mdict-web;
  tomlFormat = pkgs.formats.toml { };
  generatedConfig = tomlFormat.generate "mdict-web.toml" (
    lib.recursiveUpdate {
      index.dir = "${cfg.stateDir}/index";
    } cfg.settings
  );
  runtimeConfigPath =
    if cfg.configFile != null then cfg.configFile else "${cfg.stateDir}/mdict-web.toml";
  frontendArgs =
    lib.optionals cfg.noFrontend [ "--no-frontend" ]
    ++ lib.optionals (cfg.frontendDist != null) [
      "--frontend-dist"
      cfg.frontendDist
    ];
in
{
  options.services.mdict-web = {
    enable = lib.mkEnableOption "mdict-web dictionary service";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.default;
      defaultText = lib.literalExpression "inputs.mdict-web.packages.${pkgs.system}.default";
      description = "mdict-web package to run.";
    };

    configFile = lib.mkOption {
      type = with lib.types; nullOr path;
      default = null;
      description = ''
        Existing `mdict-web.toml` to run. When set, `settings` is ignored and
        relative paths are resolved from that file's directory.
      '';
    };

    settings = lib.mkOption {
      type = tomlFormat.type;
      default = { };
      example = lib.literalExpression ''
        {
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
        }
      '';
      description = ''
        TOML settings used to generate `mdict-web.toml` when `configFile` is not set.
        Prefer absolute dictionary paths. If `index.dir` is omitted, it defaults to
        `${cfg.stateDir}/index`.
      '';
    };

    stateDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/mdict-web";
      description = "Writable state directory for generated config and index data.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "mdict-web";
      description = "User account running the service.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "mdict-web";
      description = "Group running the service.";
    };

    frontendDist = lib.mkOption {
      type = with lib.types; nullOr path;
      default = null;
      description = ''
        Override the frontend dist directory. Leave null to use the dist bundled
        inside the package wrapper.
      '';
    };

    noFrontend = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Disable axum static frontend hosting.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Extra environment variables passed to the service.";
    };

    extraArgs = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Additional CLI arguments passed to `mdict-web`.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = !(cfg.configFile != null && cfg.settings != { });
        message = "services.mdict-web.configFile and services.mdict-web.settings are mutually exclusive.";
      }
    ];

    users.groups = lib.mkIf (cfg.group == "mdict-web") {
      mdict-web = { };
    };

    users.users = lib.mkIf (cfg.user == "mdict-web") {
      mdict-web = {
        isSystemUser = true;
        group = cfg.group;
        home = cfg.stateDir;
        createHome = true;
      };
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0750 ${cfg.user} ${cfg.group} -"
    ];

    systemd.services.mdict-web = {
      description = "mdict-web dictionary service";
      wantedBy = [ "multi-user.target" ];
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      preStart = lib.optionalString (cfg.configFile == null) ''
        install -Dm640 ${generatedConfig} ${cfg.stateDir}/mdict-web.toml
      '';
      environment = cfg.environment;
      serviceConfig = {
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.stateDir;
        ExecStart = lib.escapeShellArgs (
          [
            "${cfg.package}/bin/mdict-web"
            "--config"
            (toString runtimeConfigPath)
          ]
          ++ frontendArgs
          ++ cfg.extraArgs
        );
        Restart = "on-failure";
        RestartSec = "5s";
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "full";
        ProtectHome = false;
        LimitNOFILE = 65536;
      };
    };
  };
}
