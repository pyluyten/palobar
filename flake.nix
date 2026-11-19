{
  description = "palobar - GNOME Shell-like desktop shell for niri using Astal + TypeScript";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    ags = {
      url = "github:aylur/ags";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    ags,
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    agsPackages = ags.packages.${system};

    astalDeps = with agsPackages; [
      astal4
      io
      apps
      auth
      battery
      network
      notifd
      tray
      wireplumber
    ];

    agsWithDeps = agsPackages.default.override {
      extraPackages = astalDeps;
    };

    palobar = pkgs.stdenv.mkDerivation {
      pname = "palobar";
      version = "0.1.0";
      src = ./.;

      nativeBuildInputs = [
        pkgs.wrapGAppsHook4
        pkgs.gobject-introspection
        pkgs.makeWrapper
        agsWithDeps
      ];

      buildInputs =
        [
          pkgs.gjs
          pkgs.gtk4
          pkgs.gtk4-layer-shell
          pkgs.dart-sass
          pkgs.swaybg
          pkgs.swaylock
          pkgs.swayidle
          pkgs.networkmanager
        ]
        ++ astalDeps;

      installPhase = ''
        runHook preInstall
        export HOME=$(mktemp -d)
        mkdir -p $out/bin

        ags bundle src/prefs.ts $out/bin/palobar-prefs
        ags bundle src/run-command.ts $out/bin/palobar-run-command
        ags bundle src/app.ts $out/bin/.palobar-bin

        wrapProgram $out/bin/palobar-prefs \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.swaylock]}

        wrapProgram $out/bin/.palobar-bin \
          --prefix PATH : $out/bin \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.swaybg pkgs.swaylock pkgs.swayidle pkgs.networkmanager]}

        cat > $out/bin/palobar-ctl <<EOF
        #!${pkgs.runtimeShell}
        if ${agsWithDeps}/bin/ags request "\$@" 2>/dev/null; then
          exit 0
        fi
        if [ "\''${1:-}" = "run-command" ]; then
          exec $out/bin/palobar-run-command
        fi
        exec ${agsWithDeps}/bin/ags request "\$@"
        EOF
        chmod +x $out/bin/palobar-ctl

        cat > $out/bin/palobar <<EOF
        #!${pkgs.runtimeShell}
        if [ "\''${1:-}" = "ctl" ]; then
          shift
          exec $out/bin/palobar-ctl "\$@"
        fi
        exec $out/bin/.palobar-bin "\$@"
        EOF
        chmod +x $out/bin/palobar

        runHook postInstall
      '';
    };
  in {
    packages.${system}.default = palobar;

    devShells.${system}.default = pkgs.mkShell {
      packages = [
        agsWithDeps
        pkgs.nodejs
        pkgs.typescript
        pkgs.dart-sass
        pkgs.swaybg
        pkgs.swaylock
        pkgs.swayidle
      ];
    };
  };
}
