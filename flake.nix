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
      mpris
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
      version = "26.11.20";
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
          pkgs.gtksourceview5
          pkgs.dart-sass
          pkgs.swaybg
          pkgs.swaylock
          pkgs.swayidle
          pkgs.wl-clipboard
          pkgs.cliphist
          pkgs.networkmanager
        ]
        ++ astalDeps;

      installPhase = ''
        runHook preInstall
        export HOME=$(mktemp -d)
        mkdir -p $out/bin
        install -Dm644 kdl.lang $out/share/palobar/language-specs/kdl.lang

        ags bundle src/prefs.ts $out/bin/palobar-prefs
        ags bundle src/run-command.ts $out/bin/palobar-run-command
        ags bundle src/app.ts $out/bin/.palobar-bin

        wrapProgram $out/bin/palobar-prefs \
          --set PALOBAR_GTKSOURCE_LANGUAGE_DIR $out/share/palobar/language-specs \
          --prefix PATH : $out/bin \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.bash pkgs.swaylock]}

        wrapProgram $out/bin/.palobar-bin \
          --prefix PATH : $out/bin \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.swaybg pkgs.swaylock pkgs.swayidle pkgs.wl-clipboard pkgs.cliphist pkgs.networkmanager]}

        cat > $out/bin/palobar-ctl <<EOF
        #!${pkgs.runtimeShell}
        case "\''${1:-}" in
          "")
            printf '%s\n' 'Usage: palobar-ctl {quit|toggle-launcher|version}'
            exit 0
            ;;
          version)
            printf '%s\n' '$version'
            exit 0
            ;;
        esac
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
      PALOBAR_GTKSOURCE_LANGUAGE_DIR = "${self}";
      packages = [
        agsWithDeps
        pkgs.nodejs
        pkgs.typescript
        pkgs.gtksourceview5
        pkgs.dart-sass
        pkgs.swaybg
        pkgs.swaylock
        pkgs.swayidle
        pkgs.wl-clipboard
        pkgs.cliphist
      ];
    };
  };
}
