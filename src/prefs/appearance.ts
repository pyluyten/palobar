import { Gdk, Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";
import type { ColorScheme, WallpaperMode } from "../services/config";
import { makeGroupBox, makeRow, pickImage, type PrefsContext } from "./ui";

const MODES: WallpaperMode[] = ["fill", "fit", "stretch", "center", "tile"];

const ACCENT_COLORS = [
  { name: "Blue", hex: "#3584e4" },
  { name: "Teal", hex: "#2190a4" },
  { name: "Green", hex: "#3a944a" },
  { name: "Yellow", hex: "#c88800" },
  { name: "Orange", hex: "#ed5b00" },
  { name: "Red", hex: "#e62d42" },
  { name: "Pink", hex: "#d56199" },
  { name: "Purple", hex: "#9141ac" },
  { name: "Slate", hex: "#6f8396" },
];

function createStyleCard(
  scheme: ColorScheme,
  title: string,
  onSelect: () => void,
): { button: Gtk.Button; setSelected: (selected: boolean) => void } {
  const button = new Gtk.Button();
  button.add_css_class("prefs-style-card");

  const vbox = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    halign: Gtk.Align.CENTER,
  });

  const preview = new Gtk.Overlay();
  preview.add_css_class("prefs-style-preview");

  const fixed = new Gtk.Box({
    width_request: 128,
    height_request: 68,
  });

  const backWin = new Gtk.Box({
    halign: Gtk.Align.END,
    valign: Gtk.Align.START,
    margin_top: 2,
    margin_end: 4,
  });
  backWin.add_css_class("prefs-mockup-back");

  const frontWin = new Gtk.Box({
    halign: Gtk.Align.START,
    valign: Gtk.Align.END,
    margin_start: 6,
    margin_bottom: 2,
  });
  frontWin.add_css_class("prefs-mockup-front");
  frontWin.add_css_class(scheme === "dark" ? "dark" : "light");

  preview.set_child(fixed);
  preview.add_overlay(backWin);
  preview.add_overlay(frontWin);

  const label = new Gtk.Label({
    label: title,
    halign: Gtk.Align.CENTER,
  });
  label.add_css_class("prefs-style-label");

  vbox.append(preview);
  vbox.append(label);
  button.set_child(vbox);
  button.connect("clicked", onSelect);

  return {
    button,
    setSelected(selected: boolean) {
      if (selected) {
        button.add_css_class("selected");
      } else {
        button.remove_css_class("selected");
      }
    },
  };
}

export function buildAppearancePage(
  win: Gtk.Window,
  ctx: PrefsContext,
  onAppearanceUpdated: () => void,
): Gtk.Widget {
  const cfg = ctx.getConfig();

  const scrolled = new Gtk.ScrolledWindow({
    hscrollbar_policy: Gtk.PolicyType.NEVER,
    vscrollbar_policy: Gtk.PolicyType.AUTOMATIC,
    vexpand: true,
    hexpand: true,
  });

  const appearancePage = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 16,
  });
  appearancePage.add_css_class("prefs-page-body");

  // === Section 1: Style (Default / Dark) ===
  const styleTitle = new Gtk.Label({
    label: "Style",
    halign: Gtk.Align.START,
  });
  styleTitle.add_css_class("prefs-group-title");
  appearancePage.append(styleTitle);

  const styleGroup = makeGroupBox();
  const styleRow = makeRow(true);
  styleRow.set_halign(Gtk.Align.CENTER);
  styleRow.set_spacing(20);

  let defaultCard!: ReturnType<typeof createStyleCard>;
  let darkCard!: ReturnType<typeof createStyleCard>;

  const applyScheme = (scheme: ColorScheme) => {
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      appearance: { ...current.appearance, colorScheme: scheme },
    });
    defaultCard.setSelected(scheme === "default");
    darkCard.setSelected(scheme === "dark");
    execAsync([
      "gsettings",
      "set",
      "org.gnome.desktop.interface",
      "color-scheme",
      scheme === "dark" ? "prefer-dark" : "default",
    ]).catch(() => {});
    onAppearanceUpdated();
  };

  defaultCard = createStyleCard("default", "Default", () =>
    applyScheme("default"),
  );
  darkCard = createStyleCard("dark", "Dark", () => applyScheme("dark"));
  defaultCard.setSelected(cfg.appearance.colorScheme === "default");
  darkCard.setSelected(cfg.appearance.colorScheme === "dark");

  styleRow.append(defaultCard.button);
  styleRow.append(darkCard.button);
  styleGroup.append(styleRow);
  appearancePage.append(styleGroup);

  // === Section 2: Accent Color ===
  const accentTitle = new Gtk.Label({
    label: "Accent Color",
    halign: Gtk.Align.START,
  });
  accentTitle.add_css_class("prefs-group-title");
  appearancePage.append(accentTitle);

  const accentGroup = makeGroupBox();
  const accentRow = makeRow(true);
  accentRow.set_halign(Gtk.Align.CENTER);
  accentRow.set_spacing(10);

  const accentProvider = new Gtk.CssProvider();
  const dotCss = ACCENT_COLORS.map(
    (c, idx) => `
    .accent-dot-${idx} { background-color: ${c.hex}; }
    button.prefs-accent-btn.accent-btn-${idx}.selected {
      border: 2.5px solid ${c.hex};
    }
  `,
  ).join("\n");
  accentProvider.load_from_string(dotCss);
  Gtk.StyleContext.add_provider_for_display(
    Gdk.Display.get_default()!,
    accentProvider,
    Gtk.STYLE_PROVIDER_PRIORITY_USER + 10,
  );

  const accentButtons: Array<{ hex: string; btn: Gtk.Button }> = [];

  const syncAccentSelection = (selectedHex: string) => {
    for (const item of accentButtons) {
      if (item.hex.toLowerCase() === selectedHex.toLowerCase()) {
        item.btn.add_css_class("selected");
      } else {
        item.btn.remove_css_class("selected");
      }
    }
  };

  ACCENT_COLORS.forEach((c, idx) => {
    const btn = new Gtk.Button({
      tooltip_text: c.name,
      width_request: 34,
      height_request: 34,
      halign: Gtk.Align.CENTER,
      valign: Gtk.Align.CENTER,
    });
    btn.add_css_class("prefs-accent-btn");
    btn.add_css_class(`accent-btn-${idx}`);

    const dot = new Gtk.Box({
      width_request: 24,
      height_request: 24,
      halign: Gtk.Align.CENTER,
      valign: Gtk.Align.CENTER,
      can_target: false,
    });
    dot.add_css_class("prefs-accent-dot");
    dot.add_css_class(`accent-dot-${idx}`);
    btn.set_child(dot);

    btn.connect("clicked", () => {
      const current = ctx.getConfig();
      ctx.updateConfig({
        ...current,
        appearance: { ...current.appearance, accentColor: c.hex },
      });
      syncAccentSelection(c.hex);
      onAppearanceUpdated();
    });

    accentButtons.push({ hex: c.hex, btn });
    accentRow.append(btn);
  });

  syncAccentSelection(cfg.appearance.accentColor);
  accentGroup.append(accentRow);
  appearancePage.append(accentGroup);

  // === Section 3: Background (Preview + Image + Mode + Color Swatch) ===
  const wpGroupTitle = new Gtk.Label({
    label: "Background",
    halign: Gtk.Align.START,
  });
  wpGroupTitle.add_css_class("prefs-group-title");
  appearancePage.append(wpGroupTitle);

  const wpGroup = makeGroupBox();

  // Preview Row
  const previewRow = makeRow(false);
  previewRow.set_halign(Gtk.Align.CENTER);

  const wpPreviewStack = new Gtk.Stack({
    width_request: 220,
    height_request: 124,
    overflow: Gtk.Overflow.HIDDEN,
  });
  wpPreviewStack.add_css_class("prefs-wp-preview");

  const wpPicture = new Gtk.Picture({
    content_fit: Gtk.ContentFit.COVER,
    width_request: 220,
    height_request: 124,
  });

  const wpColorBox = new Gtk.Box({
    width_request: 220,
    height_request: 124,
  });
  wpColorBox.add_css_class("wp-preview-solid");

  const previewColorProvider = new Gtk.CssProvider();
  Gtk.StyleContext.add_provider_for_display(
    Gdk.Display.get_default()!,
    previewColorProvider,
    Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
  );

  wpPreviewStack.add_named(wpPicture, "image");
  wpPreviewStack.add_named(wpColorBox, "color");

  const refreshPreview = () => {
    const current = ctx.getConfig();
    const color = current.wallpaper.color || "#131316";
    previewColorProvider.load_from_string(
      `.wp-preview-solid { background-color: ${color}; }`,
    );
    if (
      current.wallpaper.path &&
      GLib.file_test(current.wallpaper.path, GLib.FileTest.EXISTS)
    ) {
      wpPicture.set_file(Gio.File.new_for_path(current.wallpaper.path));
      wpPreviewStack.set_visible_child_name("image");
    } else {
      wpPreviewStack.set_visible_child_name("color");
    }
  };
  refreshPreview();

  previewRow.append(wpPreviewStack);
  wpGroup.append(previewRow);

  // Row 1: Image
  const wpImageRow = makeRow(false);
  const wpImageInfo = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 2,
    hexpand: true,
  });
  const wpImageLabel = new Gtk.Label({
    label: "Wallpaper Image",
    halign: Gtk.Align.START,
  });
  wpImageLabel.add_css_class("prefs-row-title");
  const wpPathLabel = new Gtk.Label({
    label: cfg.wallpaper.path || "None (solid color)",
    halign: Gtk.Align.START,
    ellipsize: 1,
    max_width_chars: 32,
  });
  wpPathLabel.add_css_class("prefs-muted");
  wpImageInfo.append(wpImageLabel);
  wpImageInfo.append(wpPathLabel);

  const wpChooseBtn = new Gtk.Button({ label: "Choose..." });
  wpChooseBtn.add_css_class("prefs-btn");
  wpChooseBtn.connect("clicked", () => {
    pickImage(win, (path) => {
      const current = ctx.getConfig();
      ctx.updateConfig({
        ...current,
        wallpaper: { ...current.wallpaper, path },
      });
      wpPathLabel.label = path;
      refreshPreview();
    });
  });

  const wpClearBtn = new Gtk.Button({ label: "Clear" });
  wpClearBtn.add_css_class("prefs-btn");
  wpClearBtn.connect("clicked", () => {
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      wallpaper: { ...current.wallpaper, path: "" },
    });
    wpPathLabel.label = "None (solid color)";
    refreshPreview();
  });

  wpImageRow.append(wpImageInfo);
  wpImageRow.append(wpChooseBtn);
  wpImageRow.append(wpClearBtn);

  // Row 2: Display Mode
  const wpModeRow = makeRow(false);
  const wpModeLabel = new Gtk.Label({
    label: "Display Mode",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  wpModeLabel.add_css_class("prefs-row-title");
  const modeDropdown = Gtk.DropDown.new_from_strings(MODES);
  modeDropdown.selected = Math.max(0, MODES.indexOf(cfg.wallpaper.mode));
  modeDropdown.connect("notify::selected", () => {
    const mode = MODES[modeDropdown.selected] ?? "fill";
    const current = ctx.getConfig();
    ctx.updateConfig({ ...current, wallpaper: { ...current.wallpaper, mode } });
  });
  wpModeRow.append(wpModeLabel);
  wpModeRow.append(modeDropdown);

  // Row 3: Background Color Swatch (Gtk.ColorDialogButton)
  const wpColorRow = makeRow(true);
  const wpColorLabel = new Gtk.Label({
    label: "Background Color",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  wpColorLabel.add_css_class("prefs-row-title");

  const colorDialog = new Gtk.ColorDialog({ with_alpha: false });
  const colorBtn = new Gtk.ColorDialogButton({ dialog: colorDialog });
  const initRgba = new Gdk.RGBA();
  if (initRgba.parse(cfg.wallpaper.color || "#131316")) {
    colorBtn.set_rgba(initRgba);
  }
  colorBtn.connect("notify::rgba", () => {
    const c = colorBtn.get_rgba();
    const hex =
      "#" +
      [c.red, c.green, c.blue]
        .map((v) =>
          Math.round(v * 255)
            .toString(16)
            .padStart(2, "0"),
        )
        .join("");
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      wallpaper: { ...current.wallpaper, color: hex },
    });
    refreshPreview();
  });

  wpColorRow.append(wpColorLabel);
  wpColorRow.append(colorBtn);

  wpGroup.append(wpImageRow);
  wpGroup.append(wpModeRow);
  wpGroup.append(wpColorRow);
  appearancePage.append(wpGroup);

  scrolled.set_child(appearancePage);
  return scrolled;
}
