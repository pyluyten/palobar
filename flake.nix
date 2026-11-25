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
    pixbufLoaders = pkgs.runCommand "palobar-pixbuf-loaders.cache" {} ''
      cat ${pkgs.gdk-pixbuf}/${pkgs.gdk-pixbuf.binaryDir}/loaders.cache \
        ${pkgs.webp-pixbuf-loader}/${pkgs.gdk-pixbuf.binaryDir}/webp-loaders.cache > "$out"
    '';
    swaybg = pkgs.swaybg.overrideAttrs (old: {
      postInstall = (old.postInstall or "") + ''
        export GDK_PIXBUF_MODULE_FILE="${pixbufLoaders}"
      '';
    });

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
      version = pkgs.lib.removeSuffix "\n" (builtins.readFile ./VERSION);
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
          pkgs.gsettings-desktop-schemas
          pkgs.gtk4-layer-shell
          pkgs.gtksourceview5
          pkgs.dart-sass
          swaybg
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
        install -Dm644 VERSION $out/share/palobar/version

        ags bundle src/prefs.ts $out/bin/palobar-prefs
        ags bundle src/run-command.ts $out/bin/palobar-run-command
        ags bundle src/app.ts $out/bin/.palobar-bin

        install -Dm644 assets/palobar-prefs.png $out/share/icons/hicolor/128x128/apps/io.Astal.palobar-prefs.png
        mkdir -p $out/share/applications
        cat > $out/share/applications/io.Astal.palobar-prefs.desktop <<EOF
        [Desktop Entry]
        Type=Application
        Name=Palobar Settings
        Exec=$out/bin/palobar-prefs
        Icon=io.Astal.palobar-prefs
        NoDisplay=true
        EOF

        # Keep the standard GTK loaders alongside the WebP loader for previews.
        mkdir -p $out/share/palobar
        ln -s "${pixbufLoaders}" $out/share/palobar/loaders.cache
        export GDK_PIXBUF_MODULE_FILE="$out/share/palobar/loaders.cache"

        wrapProgram $out/bin/palobar-prefs \
          --set PALOBAR_VERSION_FILE $out/share/palobar/version \
          --set PALOBAR_GTKSOURCE_LANGUAGE_DIR $out/share/palobar/language-specs \
          --prefix PATH : $out/bin \
          --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.bash]}

        wrapProgram $out/bin/.palobar-bin \
          --prefix PATH : $out/bin \
          --prefix PATH : ${pkgs.lib.makeBinPath [swaybg pkgs.swayidle pkgs.wl-clipboard pkgs.cliphist pkgs.networkmanager]}

        cat > $out/bin/palobar-ctl <<EOF
        #!${pkgs.runtimeShell}
        case "\''${1:-}" in
          "")
            printf '%s\n' 'Usage: palobar-ctl {quit|toggle-launcher|toggle-dock|display-workspaces|version}'
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
    palobarRust = pkgs.rustPlatform.buildRustPackage {
      pname = "palobar-rust";
      inherit (palobar) version;
      src = pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [
          ./VERSION
          ./rust/run-command/Cargo.toml
          ./rust/run-command/Cargo.lock
          ./rust/run-command/src
        ];
      };
      cargoRoot = "rust/run-command";
      buildAndTestSubdir = "rust/run-command";
      cargoLock.lockFile = ./rust/run-command/Cargo.lock;
      cargoBuildFlags = ["--bin" "palobar-run-command-rs"];

      nativeBuildInputs = [pkgs.pkg-config pkgs.wrapGAppsHook4];
      buildInputs = [pkgs.gtk4 pkgs.gtk4-layer-shell pkgs.gtksourceview5 pkgs.gsettings-desktop-schemas];

      preFixup = ''
        gappsWrapperArgs+=(
          --set GDK_PIXBUF_MODULE_FILE "${pixbufLoaders}"
          --prefix PATH : "${palobar}/bin"
          --set PALOBAR_GTKSOURCE_LANGUAGE_DIR "${palobar}/share/palobar/language-specs"
          --suffix PATH : "${pkgs.lib.makeBinPath [pkgs.niri pkgs.networkmanager pkgs.wireplumber pkgs.systemd pkgs.coreutils pkgs.wl-clipboard pkgs.cliphist pkgs.cups]}"
          --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath [pkgs.pam]}"
        )
      '';

      postInstall = ''
        mv "$out/bin/palobar-run-command-rs" "$out/bin/palobar-rust"
        cat > "$out/bin/palobar-rust-session" <<'EOF'
        #!@bash@
        set -eu

        backend=
        panel=
        backend_ready=false

        backend_status() {
            @timeout@ 1 @palobarCtl@ get-bar-state >/dev/null 2>&1
        }

        cleanup() {
            if [[ -n "$panel" ]]; then
                kill "$panel" 2>/dev/null || true
                wait "$panel" 2>/dev/null || true
            fi
            if [[ -n "$backend" ]]; then
                if "$backend_ready" && kill -0 "$backend" 2>/dev/null; then
                    @timeout@ 2 @palobarCtl@ quit >/dev/null 2>&1 || true
                fi
                kill "$backend" 2>/dev/null || true
                wait "$backend" 2>/dev/null || true
            fi
        }

        if backend_status; then
            printf '%s\n' 'Palobar is already running. Stop it with palobar-ctl quit before starting the Rust session.' >&2
            exit 1
        fi

        trap cleanup EXIT
        trap 'exit 130' INT
        trap 'exit 143' TERM

        PALOBAR_RUST_COMMAND=@palobarRust@ @palobar@ &
        backend=$!

        for ((attempt = 0; attempt < 50; attempt++)); do
            if ! kill -0 "$backend" 2>/dev/null; then
                break
            fi
            if backend_status; then
                backend_ready=true
                break
            fi
            @sleep@ 0.1
        done

        if ! "$backend_ready"; then
            printf '%s\n' 'The Palobar backend failed to start.' >&2
            exit 1
        fi

        @palobarRust@ run-panel &
        panel=$!
        wait -n "$backend" "$panel"
        EOF
        chmod +x "$out/bin/palobar-rust-session"
        substituteInPlace "$out/bin/palobar-rust-session" \
          --replace-fail '@bash@' '${pkgs.bash}/bin/bash' \
          --replace-fail '@timeout@' '${pkgs.coreutils}/bin/timeout' \
          --replace-fail '@sleep@' '${pkgs.coreutils}/bin/sleep' \
          --replace-fail '@palobar@' '${palobar}/bin/palobar' \
          --replace-fail '@palobarCtl@' '${palobar}/bin/palobar-ctl' \
          --replace-fail '@palobarRust@' "$out/bin/palobar-rust"
      '';

      meta.mainProgram = "palobar-rust";
    };
  in {
    packages.${system} = {
      default = palobar;
      palobar-rust = palobarRust;
    };

    devShells.${system} = {
      rust = pkgs.mkShell {
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
        ];
        shellHook = ''
          export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [pkgs.pam]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
        '';
      };

      default = pkgs.mkShell {
        GDK_PIXBUF_MODULE_FILE = "${pixbufLoaders}";
        PALOBAR_GTKSOURCE_LANGUAGE_DIR = "${self}";
        PALOBAR_VERSION_FILE = "${self}/VERSION";
        packages = [
          agsWithDeps
          pkgs.nodejs
          pkgs.typescript
          pkgs.gtksourceview5
          pkgs.dart-sass
          swaybg
          pkgs.swayidle
          pkgs.wl-clipboard
          pkgs.cliphist
        ];
      };
    };
  };
}
