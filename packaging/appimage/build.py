#!/usr/bin/env python3
"""Build the experimental x86_64 AppImage with host GTK and GJS."""

import argparse
import base64
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PACKAGING = Path(__file__).resolve().parent
BUNDLES = {
    "...palobar-bin-wrapped-wrapped": "daemon",
    "..palobar-prefs-wrapped-wrapped": "prefs",
    ".palobar-run-command-wrapped": "run-command",
}
TYPELIBS = (
    "Astal-4.0", "AstalIO-0.1", "AstalApps-0.1", "AstalAuth-0.1",
    "AstalMpris-0.1", "AstalNetwork-0.1", "AstalNotifd-0.1", "AstalTray-0.1",
    "AstalWp-0.1", "Gtk4LayerShell-1.0", "Gtk4SessionLock-1.0",
    "AppmenuGLibTranslator-24.02",
)
LIBRARIES = ("libastal*.so.*", "libgtk4-layer-shell.so.*", "libappmenu-glib-translator.so.*")
TOOLS = ("swayidle", "swaybg", "wl-copy", "wl-paste", "cliphist")
HOST_ENV = b"""
import GLibForAppImage from 'gi://GLib';
for (const name of ['LD_PRELOAD', 'LD_LIBRARY_PATH']) {
    const value = GLibForAppImage.getenv('PALOBAR_HOST_' + name);
    if (value) GLibForAppImage.setenv(name, value, true);
    else GLibForAppImage.unsetenv(name);
    GLibForAppImage.unsetenv('PALOBAR_HOST_' + name);
}
"""


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def assemble(appdir, package, patchelf, cliphist):
    closure = sorted(map(Path, run("nix-store", "-qR", package).splitlines()))
    binary = appdir / "usr/bin"
    lib = appdir / "usr/lib/palobar"
    typelibs = lib / "girepository-1.0"
    data = appdir / "usr/share"
    schemas = data / "glib-2.0/schemas"
    for directory in (binary, typelibs, schemas):
        directory.mkdir(parents=True)

    for name, target in BUNDLES.items():
        wrapper = (package / "bin" / name).read_bytes()
        bundle = re.search(rb"^([A-Za-z0-9+/=]{1000,})\nEOF$", wrapper, re.MULTILINE)
        if bundle is None:
            raise ValueError(f"Could not find AGS bundle in {name}")
        js = base64.b64decode(bundle[1], validate=True)
        js = re.sub(rb"^// (?:\.\./)*/?nix/store/[a-z0-9]{32}-([^/]+/)", rb"// \1", js, flags=re.MULTILINE)
        (lib / f"{target}.mjs").write_bytes(HOST_ENV + js)

    for name in TYPELIBS:
        source = next((dep / "lib/girepository-1.0" / f"{name}.typelib"
                       for dep in closure
                       if (dep / "lib/girepository-1.0" / f"{name}.typelib").is_file()), None)
        if source is None:
            raise FileNotFoundError(f"Missing typelib: {name}")
        # GI stores offsets into a NUL-terminated string table. Preserve offsets
        # while shortening absolute library names to their SONAMEs.
        module = re.sub(
            rb"/nix/store/[^\x00 ,]+/(lib[^/\x00 ,]+\.so(?:\.[0-9]+)*)",
            lambda match: match[1].ljust(len(match[0]), b"\0"),
            source.read_bytes(),
        )
        (typelibs / source.name).write_bytes(module)

    for dep in closure:
        for pattern in LIBRARIES:
            for source in (dep / "lib").glob(pattern):
                # AGS also depends on Astal GTK3. The GTK4 image needs only libastal-4.
                if source.name.startswith("libastal.so."):
                    continue
                target = lib / source.name
                shutil.copyfile(source, target)
                subprocess.run([patchelf, "--set-rpath", "$ORIGIN", target], check=True)
        for schema in (dep / "share").glob("**/*astal*.gschema.xml"):
            shutil.copy2(schema, schemas / schema.name)

    for tool in TOOLS:
        if tool == "cliphist":
            target = binary / tool
            shutil.copyfile(cliphist, target)
            target.chmod(0o755)
            continue
        candidates = [dep / "bin" / tool for dep in closure if (dep / "bin" / tool).is_file()]
        if len(candidates) != 1:
            raise ValueError(f"Expected one {tool}, got {candidates}")
        source = candidates[0]
        unwrapped = source.with_name(f".{tool}-wrapped")
        target = shutil.copy2(unwrapped if unwrapped.is_file() else source, binary / tool)
        target.chmod(0o755)
        subprocess.run([patchelf, "--set-rpath", "$ORIGIN/../lib/palobar", "--set-interpreter",
                        "/lib64/ld-linux-x86-64.so.2", target], check=True)

        if tool == "swayidle":
            # The Nix derivation embeds these paths in swayidle's C strings.
            payload = target.read_bytes()
            for pattern, replacement in (
                (rb"/nix/store/[^\x00 ]+/bin/bash\x00", b"/bin/sh"),
                (rb"/nix/store/[^\x00 ]+/etc/swayidle/config\x00", b"/etc/swayidle/config"),
            ):
                payload = re.sub(pattern, lambda match: replacement.ljust(len(match[0]), b"\0"), payload)
            target.write_bytes(payload)

    subprocess.run(["glib-compile-schemas", "--strict", schemas], check=True)
    shutil.copytree(package / "share/palobar/language-specs", data / "palobar/language-specs")
    shutil.copyfile(package / "share/palobar/version", data / "palobar/version")
    shutil.copytree(package / "share/icons", data / "icons", ignore=shutil.ignore_patterns("icon-theme.cache"))
    desktop = package / "share/applications/io.Astal.palobar-prefs.desktop"
    (data / "applications").mkdir()
    info = re.sub(r"^Exec=.*", "Exec=palobar-prefs", desktop.read_text(), flags=re.MULTILINE)
    (data / "applications" / desktop.name).write_text(info)
    (appdir / "palobar.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=Palobar\nExec=palobar\n"
        "Icon=palobar\nCategories=Utility;\nTerminal=false\n"
    )
    shutil.copy2(ROOT / "assets/palobar-prefs.png", appdir / "palobar.png")
    (appdir / ".DirIcon").symlink_to("palobar.png")
    shutil.copy2(PACKAGING / "AppRun", appdir / "AppRun")
    (appdir / "AppRun").chmod(0o755)
    shutil.copy2(PACKAGING / "ctl.mjs", lib)
    for launcher in ("palobar", "palobar-prefs", "palobar-run-command", "palobar-ctl"):
        (binary / launcher).symlink_to("../../AppRun")

    for target in appdir.rglob("*"):
        if target.is_dir():
            target.chmod(0o755)
        if target.is_file() and not target.is_symlink():
            if re.search(rb"/nix/store/[a-z0-9]{32}-", target.read_bytes()):
                raise ValueError(f"File still references the Nix store: {target.relative_to(appdir)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--appimagetool", required=True, type=Path)
    parser.add_argument("--patchelf", default="patchelf")
    parser.add_argument("--package", type=Path)
    parser.add_argument("--cliphist", required=True, type=Path, help="Upstream x86_64 cliphist binary")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    package = (args.package or Path(run("nix", "build", "--no-link", "--print-out-paths", ROOT))).resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="palobar-appimage-") as temp:
        appdir = Path(temp) / "Palobar.AppDir"
        assemble(appdir, package, args.patchelf, args.cliphist)
        subprocess.run([args.appimagetool.resolve(), appdir, args.output.resolve()],
                       env={**os.environ, "APPIMAGE_EXTRACT_AND_RUN": "1"}, check=True)


if __name__ == "__main__":
    main()
