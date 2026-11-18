import app from "ags/gtk4/app";
import { Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";
import {
  CONFIG_PATH,
  loadConfig,
  saveConfig,
  type PalobarConfig,
  type WallpaperMode,
} from "./services/config";

const MODES: WallpaperMode[] = ["fill", "fit", "stretch", "center", "tile"];

const PREFS_CSS = `
window.prefs-window {
  background-color: #1e1e22;
  color: #f6f5f4;
  font-family: "Cantarell", "Inter", sans-serif;
  font-size: 13.5px;
}

.prefs-section {
  background-color: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 14px;
  padding: 16px;
}

.prefs-section-title {
  font-size: 15px;
  font-weight: 700;
}

.prefs-muted {
  font-size: 12px;
  color: rgba(246, 245, 244, 0.65);
}

.prefs-btn {
  background-color: rgba(255, 255, 255, 0.1);
  background-image: none;
  box-shadow: none;
  border: none;
  border-radius: 10px;
  padding: 7px 14px;
  color: #f6f5f4;
}

.prefs-btn:hover {
  background-color: rgba(255, 255, 255, 0.16);
}

.prefs-btn.primary {
  background-color: #3584e4;
  font-weight: 600;
}

.prefs-btn.primary:hover {
  background-color: #4a93eb;
}

.prefs-entry {
  background-color: rgba(0, 0, 0, 0.25);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  padding: 6px 10px;
  color: #f6f5f4;
}
`;

function pickImage(parent: Gtk.Window, onSelected: (path: string) => void) {
  const filter = new Gtk.FileFilter();
  filter.set_name("Images (PNG, JPEG, WebP)");
  filter.add_mime_type("image/png");
  filter.add_mime_type("image/jpeg");
  filter.add_mime_type("image/webp");

  const filters = new Gio.ListStore({ item_type: Gtk.FileFilter.$gtype });
  filters.append(filter);

  const dialog = new Gtk.FileDialog({
    title: "Select Image",
    filters,
    default_filter: filter,
  });

  dialog.open(parent, null, (_dlg, res) => {
    try {
      const file = dialog.open_finish(res);
      const path = file?.get_path();
      if (path) onSelected(path);
    } catch {
      // cancelled
    }
  });
}

function buildPrefsWindow() {
  let cfg: PalobarConfig = loadConfig();

  const win = new Gtk.Window({
    title: "Palobar Preferences",
    default_width: 480,
    default_height: 440,
    resizable: false,
  });
  win.add_css_class("prefs-window");

  const root = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 16,
    margin_top: 20,
    margin_bottom: 20,
    margin_start: 20,
    margin_end: 20,
  });

  // --- Wallpaper Section ---
  const wpSection = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 10,
  });
  wpSection.add_css_class("prefs-section");

  const wpTitle = new Gtk.Label({
    label: "Wallpaper",
    halign: Gtk.Align.START,
  });
  wpTitle.add_css_class("prefs-section-title");
  wpSection.append(wpTitle);

  const wpPathLabel = new Gtk.Label({
    label: cfg.wallpaper.path || "None (solid color)",
    halign: Gtk.Align.START,
    ellipsize: 1,
    max_width_chars: 42,
  });
  wpPathLabel.add_css_class("prefs-muted");

  const wpBtnRow = new Gtk.Box({ spacing: 8 });
  const wpChooseBtn = new Gtk.Button({ label: "Choose Image..." });
  wpChooseBtn.add_css_class("prefs-btn");
  wpChooseBtn.connect("clicked", () => {
    pickImage(win, (path) => {
      cfg = { ...cfg, wallpaper: { ...cfg.wallpaper, path } };
      wpPathLabel.label = path;
    });
  });

  const wpClearBtn = new Gtk.Button({ label: "Clear" });
  wpClearBtn.add_css_class("prefs-btn");
  wpClearBtn.connect("clicked", () => {
    cfg = { ...cfg, wallpaper: { ...cfg.wallpaper, path: "" } };
    wpPathLabel.label = "None (solid color)";
  });

  wpBtnRow.append(wpChooseBtn);
  wpBtnRow.append(wpClearBtn);

  const modeRow = new Gtk.Box({ spacing: 10 });
  const modeLabel = new Gtk.Label({
    label: "Display Mode",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  const modeDropdown = Gtk.DropDown.new_from_strings(MODES);
  modeDropdown.selected = Math.max(0, MODES.indexOf(cfg.wallpaper.mode));
  modeDropdown.connect("notify::selected", () => {
    const mode = MODES[modeDropdown.selected] ?? "fill";
    cfg = { ...cfg, wallpaper: { ...cfg.wallpaper, mode } };
  });
  modeRow.append(modeLabel);
  modeRow.append(modeDropdown);

  const wpColorRow = new Gtk.Box({ spacing: 10 });
  const wpColorLabel = new Gtk.Label({
    label: "Background Color",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  const wpColorEntry = new Gtk.Entry({
    text: cfg.wallpaper.color,
    width_chars: 10,
  });
  wpColorEntry.add_css_class("prefs-entry");
  wpColorEntry.connect("changed", () => {
    cfg = { ...cfg, wallpaper: { ...cfg.wallpaper, color: wpColorEntry.text } };
  });
  wpColorRow.append(wpColorLabel);
  wpColorRow.append(wpColorEntry);

  wpSection.append(wpPathLabel);
  wpSection.append(wpBtnRow);
  wpSection.append(modeRow);
  wpSection.append(wpColorRow);

  // --- Lock Screen Section ---
  const lockSection = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 10,
  });
  lockSection.add_css_class("prefs-section");

  const lockTitle = new Gtk.Label({
    label: "Lock Screen",
    halign: Gtk.Align.START,
  });
  lockTitle.add_css_class("prefs-section-title");
  lockSection.append(lockTitle);

  const lockPathLabel = new Gtk.Label({
    label: cfg.lockscreen.image || "Defaults to wallpaper",
    halign: Gtk.Align.START,
    ellipsize: 1,
    max_width_chars: 42,
  });
  lockPathLabel.add_css_class("prefs-muted");

  const lockBtnRow = new Gtk.Box({ spacing: 8 });
  const lockChooseBtn = new Gtk.Button({ label: "Choose Image..." });
  lockChooseBtn.add_css_class("prefs-btn");
  lockChooseBtn.connect("clicked", () => {
    pickImage(win, (path) => {
      cfg = { ...cfg, lockscreen: { ...cfg.lockscreen, image: path } };
      lockPathLabel.label = path;
    });
  });

  const lockClearBtn = new Gtk.Button({ label: "Reset" });
  lockClearBtn.add_css_class("prefs-btn");
  lockClearBtn.connect("clicked", () => {
    cfg = { ...cfg, lockscreen: { ...cfg.lockscreen, image: "" } };
    lockPathLabel.label = "Defaults to wallpaper";
  });

  lockBtnRow.append(lockChooseBtn);
  lockBtnRow.append(lockClearBtn);

  const lockColorRow = new Gtk.Box({ spacing: 10 });
  const lockColorLabel = new Gtk.Label({
    label: "Fallback Color",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  const lockColorEntry = new Gtk.Entry({
    text: cfg.lockscreen.color,
    width_chars: 10,
  });
  lockColorEntry.add_css_class("prefs-entry");
  lockColorEntry.connect("changed", () => {
    cfg = {
      ...cfg,
      lockscreen: { ...cfg.lockscreen, color: lockColorEntry.text },
    };
  });
  lockColorRow.append(lockColorLabel);
  lockColorRow.append(lockColorEntry);

  lockSection.append(lockPathLabel);
  lockSection.append(lockBtnRow);
  lockSection.append(lockColorRow);

  // --- Footer ---
  const configNote = new Gtk.Label({
    label: `Config: ${CONFIG_PATH}`,
    halign: Gtk.Align.START,
    ellipsize: 2,
  });
  configNote.add_css_class("prefs-muted");

  const footer = new Gtk.Box({ spacing: 8, halign: Gtk.Align.END });

  const testLockBtn = new Gtk.Button({ label: "Test Lock" });
  testLockBtn.add_css_class("prefs-btn");
  testLockBtn.connect("clicked", () => {
    saveConfig(cfg);
    execAsync(["ags", "request", "lock"]).catch(() => {
      const fallbackColor = cfg.lockscreen.color.replace("#", "") || "131316";
      const args = ["swaylock", "-c", fallbackColor];
      const img = cfg.lockscreen.image || cfg.wallpaper.path;
      if (img && GLib.file_test(img, GLib.FileTest.EXISTS)) {
        args.push("-i", img, "-s", "fill");
      }
      execAsync(args).catch((err) => console.error(err));
    });
  });

  const applyBtn = new Gtk.Button({ label: "Save & Apply" });
  applyBtn.add_css_class("prefs-btn");
  applyBtn.add_css_class("primary");
  applyBtn.connect("clicked", () => {
    saveConfig(cfg);
  });

  const closeBtn = new Gtk.Button({ label: "Close" });
  closeBtn.add_css_class("prefs-btn");
  closeBtn.connect("clicked", () => {
    win.close();
  });

  footer.append(testLockBtn);
  footer.append(applyBtn);
  footer.append(closeBtn);

  root.append(wpSection);
  root.append(lockSection);
  root.append(configNote);
  root.append(footer);

  win.set_child(root);
  win.connect("close-request", () => {
    app.quit();
    return false;
  });

  app.add_window(win);
  win.present();
}

app.start({
  instanceName: "palobar-prefs",
  css: PREFS_CSS,
  main() {
    buildPrefsWindow();
  },
});
