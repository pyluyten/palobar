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
        ags bundle src/app.ts $out/bin/palobar

        wrapProgram $out/bin/palobar-prefs \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.swaylock]}

        wrapProgram $out/bin/palobar \
          --prefix PATH : $out/bin \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.swaybg pkgs.swaylock pkgs.swayidle pkgs.networkmanager]}

        cat > $out/bin/palobar-ctl <<EOF
        #!${pkgs.runtimeShell}
        exec ${agsWithDeps}/bin/ags request "\$@"
        EOF
        chmod +x $out/bin/palobar-ctl

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
