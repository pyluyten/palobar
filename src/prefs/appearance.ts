import { Gtk } from "ags/gtk4";
import type { WallpaperMode } from "../services/config";
import { makeGroupBox, makeRow, pickImage, type PrefsContext } from "./ui";

const MODES: WallpaperMode[] = ["fill", "fit", "stretch", "center", "tile"];

export function buildAppearancePage(
  win: Gtk.Window,
  ctx: PrefsContext,
): Gtk.Widget {
  const cfg = ctx.getConfig();

  const appearancePage = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 12,
  });
  appearancePage.add_css_class("prefs-page-body");

  const wpGroupTitle = new Gtk.Label({
    label: "Background",
    halign: Gtk.Align.START,
  });
  wpGroupTitle.add_css_class("prefs-group-title");
  appearancePage.append(wpGroupTitle);

  const wpGroup = makeGroupBox();

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

  // Row 3: Background Color
  const wpColorRow = makeRow(true);
  const wpColorLabel = new Gtk.Label({
    label: "Fallback Color",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  wpColorLabel.add_css_class("prefs-row-title");
  const wpColorEntry = new Gtk.Entry({
    text: cfg.wallpaper.color,
    width_chars: 10,
  });
  wpColorEntry.add_css_class("prefs-entry");
  wpColorEntry.connect("changed", () => {
    const current = ctx.getConfig();
    ctx.updateConfigDebounced({
      ...current,
      wallpaper: { ...current.wallpaper, color: wpColorEntry.text },
    });
  });
  wpColorRow.append(wpColorLabel);
  wpColorRow.append(wpColorEntry);

  wpGroup.append(wpImageRow);
  wpGroup.append(wpModeRow);
  wpGroup.append(wpColorRow);
  appearancePage.append(wpGroup);

  return appearancePage;
}
