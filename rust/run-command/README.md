# Rust run-command prototype

This isolated prototype replaces the AGS/Astal window for the standalone run-command component with GTK4 and gtk4-layer-shell. It currently supports command submission through Niri, Ctrl+Enter terminal launch, and Escape to close.

It is a technical spike, not a drop-in replacement yet. History, executable completion, appearance tracking, backdrop dismissal, and integration with the running Palobar process still need to be ported before replacing the existing component.

Build and launch it from a Niri session with:

```sh
cargo run --manifest-path rust/run-command/Cargo.toml
```

The system needs GTK4, gtk4-layer-shell, Rust, and Cargo. The Rust dependencies are declared in `Cargo.toml`.

## Bar preview

The separate Rust bar follows the existing `src/widgets/Bar.tsx` layout and workspace interactions. It overlays the existing top bar without reserving extra screen space. Stop any running Palobar instance first, then build and launch the updated AGS daemon so its calendar, clipboard, and quick-settings popups handle the Rust bar's requests:

```sh
mkdir -p build
ags bundle src/app.ts build/palobar-preview
./build/palobar-preview
```

Then, in a second terminal from the worktree root, run:

```sh
cargo run --manifest-path rust/run-command/Cargo.toml --bin palobar-bar-rs
```

The preview ports the workspace switcher, clock, notification count, network and volume indicators, and popup buttons. Stop it with Ctrl+C in the second terminal. The StatusNotifier tray is not yet ported, so it is absent while the Rust overlay is running; it will reappear with the original bar after the preview exits.
