# Building the x86_64 AppImage


## prerequisites

+ Set the release version in the repository's [VERSION](../../VERSION) file.
+ The builder needs Nix, Python 3, `patchelf`, `glib-compile-schemas`, and `file` on `PATH`. 
+ From the repository root, download the two upstream build inputs:

```sh
tools_dir=$(mktemp -d)
curl -fL https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage -o "$tools_dir/appimagetool"
curl -fL https://github.com/sentriz/cliphist/releases/download/v0.7.0/v0.7.0-linux-amd64 -o "$tools_dir/cliphist"
chmod +x "$tools_dir/appimagetool"
```

## Build

Build and check the image:

```sh
python3 packaging/appimage/build.py \
  --appimagetool "$tools_dir/appimagetool" \
  --cliphist "$tools_dir/cliphist" \
  dist/Palobar-Arch-x86_64.AppImage

./dist/Palobar-Arch-x86_64.AppImage ctl version
```

The script builds Palobar with the repository's Nix flake, then assembles the image. It can also use a previously built Palobar package via `--package /nix/store/…-palobar-…`. Running the finished
AppImage uses the host's GTK and GJS.
