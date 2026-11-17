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
  in {
    devShells.${system}.default = pkgs.mkShell {
      packages = [
        (agsPackages.default.override {
          extraPackages = with agsPackages; [
            astal4
            io
            apps
            battery
            network
            notifd
            tray
            wireplumber
          ];
        })
        pkgs.nodejs
        pkgs.typescript
        pkgs.dart-sass
        pkgs.swaybg
      ];
    };
  };
}
