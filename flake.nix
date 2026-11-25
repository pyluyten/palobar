{
  description = "palobar - GNOME Shell-like desktop shell for niri using Rust and GTK4";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs = {nixpkgs, ...}: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    pixbufLoaders = pkgs.runCommand "palobar-pixbuf-loaders.cache" {} ''
      cat ${pkgs.gdk-pixbuf}/${pkgs.gdk-pixbuf.binaryDir}/loaders.cache \
        ${pkgs.webp-pixbuf-loader}/${pkgs.gdk-pixbuf.binaryDir}/webp-loaders.cache > "$out"
    '';
    swaybg = pkgs.swaybg.overrideAttrs (old: {
      postInstall = (old.postInstall or "") + ''
        export GDK_PIXBUF_MODULE_FILE="${pixbufLoaders}"
      '';
    });
    desktopTools = [swaybg pkgs.swayidle];
    runtimeTools = with pkgs; [
      niri
      networkmanager
      wireplumber
      systemd
      coreutils
      wl-clipboard
      cliphist
      cups
    ];

    palobar = pkgs.rustPlatform.buildRustPackage {
      pname = "palobar";
      version = pkgs.lib.removeSuffix "\n" (builtins.readFile ./VERSION);
      src = pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [
          ./VERSION
          ./kdl.lang
          ./assets/palobar-prefs.png
          ./Cargo.toml
          ./Cargo.lock
          ./src
        ];
      };
      cargoLock.lockFile = ./Cargo.lock;

      nativeBuildInputs = [pkgs.pkg-config pkgs.wrapGAppsHook4];
      buildInputs = [pkgs.gtk4 pkgs.gtk4-layer-shell pkgs.gtksourceview5 pkgs.gsettings-desktop-schemas];

      preFixup = ''
        gappsWrapperArgs+=(
          --set GDK_PIXBUF_MODULE_FILE "${pixbufLoaders}"
          --set PALOBAR_GTKSOURCE_LANGUAGE_DIR "$out/share/palobar/language-specs"
          --prefix PATH : "${pkgs.lib.makeBinPath desktopTools}"
          --suffix PATH : "${pkgs.lib.makeBinPath runtimeTools}"
          --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath [pkgs.pam]}"
        )
      '';

      postInstall = ''
        install -Dm644 "$NIX_BUILD_TOP/$sourceRoot/kdl.lang" "$out/share/palobar/language-specs/kdl.lang"
        install -Dm644 "$NIX_BUILD_TOP/$sourceRoot/assets/palobar-prefs.png" "$out/share/icons/hicolor/128x128/apps/io.Astal.palobar-prefs.png"
      '';

      meta.mainProgram = "palobar";
      meta.priority = 4;
    };
    rustShell = pkgs.mkShell {
      GDK_PIXBUF_MODULE_FILE = "${pixbufLoaders}";
      packages = [
        pkgs.cargo
        pkgs.rustc
        pkgs.rustfmt
        pkgs.pkg-config
        pkgs.gtk4
        pkgs.gtk4-layer-shell
        pkgs.gtksourceview5
        pkgs.gsettings-desktop-schemas
      ] ++ desktopTools;
      shellHook = ''
        export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [pkgs.pam]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
        export PATH="$PATH:${pkgs.lib.makeBinPath runtimeTools}"
      '';
    };

  in {
    packages.${system} = {
      default = palobar;
      inherit palobar;
    };

    devShells.${system} = {
      default = rustShell;
      rust = rustShell;
    };
  };
}
