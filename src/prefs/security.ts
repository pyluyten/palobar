import { Gdk, Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";
import { DEFAULT_CONFIG, saveConfig } from "../services/config";
import { checkLockSupport } from "../services/lock-support";
import { makeGroupBox, makeRow, pickImage, type PrefsContext } from "./ui";

const LOCK_DELAYS = [
  { label: "Disabled", seconds: null },
  { label: "Immediately", seconds: 0 },
  { label: "30 seconds", seconds: 30 },
  { label: "1 minute", seconds: 60 },
  { label: "2 minutes", seconds: 120 },
  { label: "5 minutes", seconds: 300 },
  { label: "10 minutes", seconds: 600 },
];

export function buildSecurityPage(
  win: Gtk.Window,
  ctx: PrefsContext,
): Gtk.Widget {
  const cfg = ctx.getConfig();

  const securityPage = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 12,
  });
  securityPage.add_css_class("prefs-page-body");

  const lockGroupTitle = new Gtk.Label({
    label: "Lock Screen",
    halign: Gtk.Align.START,
  });
  lockGroupTitle.add_css_class("prefs-group-title");
  securityPage.append(lockGroupTitle);

  const availability = new Gtk.Label({
    label: "Checking lock screen availability…",
    halign: Gtk.Align.START,
    wrap: true,
    xalign: 0,
  });
  availability.add_css_class("prefs-muted");
  securityPage.append(availability);

  const lockGroup = makeGroupBox();
  lockGroup.set_sensitive(false);
  securityPage.connect("map", () => {
    void checkLockSupport().then((support) => {
      lockGroup.set_sensitive(support.available);
      availability.set_visible(!support.available);
      if (!support.available) {
        availability.set_label(support.reason);
        availability.add_css_class("prefs-lock-unavailable");
      }
    });
  });

  const previewRow = makeRow(false);
  previewRow.halign = Gtk.Align.CENTER;
  const previewFrame = new Gtk.Overlay({
    child: new Gtk.Box({ width_request: 220, height_request: 124 }),
    overflow: Gtk.Overflow.HIDDEN,
  });
  previewFrame.add_css_class("prefs-wp-preview");
  const preview = new Gtk.Picture({ content_fit: Gtk.ContentFit.COVER });
  previewFrame.add_overlay(preview);

  const previewColor = new Gtk.CssProvider();
  previewFrame.get_style_context().add_provider(
    previewColor,
    Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
  );
  const currentColor = () => {
    const color = new Gdk.RGBA();
    if (!color.parse(ctx.getConfig().lockscreen.color)) color.parse(DEFAULT_CONFIG.lockscreen.color);
    return color;
  };
  const refreshPreview = () => {
    const current = ctx.getConfig();
    const path = current.lockscreen.image || current.wallpaper.path;
    preview.set_file(path && GLib.file_test(path, GLib.FileTest.IS_REGULAR)
      ? Gio.File.new_for_path(path)
      : null);
    previewColor.load_from_string(`.prefs-wp-preview { background-color: ${currentColor().to_string()}; }`);
  };
  refreshPreview();
  securityPage.connect("map", refreshPreview);
  previewRow.append(previewFrame);

  // Row 1: Lock Screen Image
  const lockImageRow = makeRow(false);
  const lockImageInfo = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 2,
    hexpand: true,
  });
  const lockImageLabel = new Gtk.Label({
    label: "Lock Screen Image",
    halign: Gtk.Align.START,
  });
  lockImageLabel.add_css_class("prefs-row-title");
  const lockPathLabel = new Gtk.Label({
    label: cfg.lockscreen.image || "Defaults to wallpaper",
    halign: Gtk.Align.START,
    ellipsize: 1,
    max_width_chars: 32,
  });
  lockPathLabel.add_css_class("prefs-muted");
  lockImageInfo.append(lockImageLabel);
  lockImageInfo.append(lockPathLabel);

  const lockChooseBtn = new Gtk.Button({ label: "Choose..." });
  lockChooseBtn.add_css_class("prefs-btn");
  lockChooseBtn.connect("clicked", () => {
    pickImage(win, (path) => {
      const current = ctx.getConfig();
      ctx.updateConfig({
        ...current,
        lockscreen: { ...current.lockscreen, image: path },
      });
      lockPathLabel.label = path;
      refreshPreview();
    });
  });

  const lockClearBtn = new Gtk.Button({ label: "Reset" });
  lockClearBtn.add_css_class("prefs-btn");
  lockClearBtn.connect("clicked", () => {
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      lockscreen: { ...current.lockscreen, image: "" },
    });
    lockPathLabel.label = "Defaults to wallpaper";
    refreshPreview();
  });

  lockImageRow.append(lockImageInfo);
  lockImageRow.append(lockChooseBtn);
  lockImageRow.append(lockClearBtn);

  // Row 2: Fallback Color
  const lockColorRow = makeRow(false);
  const lockColorLabel = new Gtk.Label({
    label: "Fallback Color",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  lockColorLabel.add_css_class("prefs-row-title");
  const colorBtn = new Gtk.ColorDialogButton({
    dialog: new Gtk.ColorDialog({ with_alpha: false }),
    rgba: currentColor(),
  });
  colorBtn.connect("notify::rgba", () => {
    const { red, green, blue } = colorBtn.get_rgba();
    const hex = "#" + [red, green, blue]
      .map((value) => Math.round(value * 255).toString(16).padStart(2, "0"))
      .join("");
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      lockscreen: { ...current.lockscreen, color: hex },
    });
    refreshPreview();
  });
  lockColorRow.append(lockColorLabel);
  lockColorRow.append(colorBtn);

  const autoLockRow = makeRow(false);
  const autoLockInfo = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 2,
    hexpand: true,
  });
  const autoLockTitle = new Gtk.Label({
    label: "Automatic Lock",
    halign: Gtk.Align.START,
  });
  autoLockTitle.add_css_class("prefs-row-title");
  const autoLockHint = new Gtk.Label({ halign: Gtk.Align.START });
  autoLockHint.add_css_class("prefs-muted");
  autoLockInfo.append(autoLockTitle);
  autoLockInfo.append(autoLockHint);

  const delay = Gtk.DropDown.new_from_strings(LOCK_DELAYS.map((item) => item.label));
  delay.selected = Math.max(0, LOCK_DELAYS.findIndex(
    (item) => item.seconds === cfg.lockscreen.autoLockDelaySeconds,
  ));
  const syncAutoLock = () => {
    delay.sensitive = ctx.getConfig().power.screenBlankEnabled;
    autoLockHint.label = delay.sensitive
      ? "Wait after the screen turns off"
      : "Enable Automatic Screen Blank in Power";
  };
  syncAutoLock();
  securityPage.connect("map", syncAutoLock);
  delay.connect("notify::selected", () => {
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      lockscreen: {
        ...current.lockscreen,
        autoLockDelaySeconds: LOCK_DELAYS[delay.selected]?.seconds ?? null,
      },
    });
  });
  autoLockRow.append(autoLockInfo);
  autoLockRow.append(delay);

  // Row 3: Test Lock Screen
  const testLockRow = makeRow(true);
  const testLockInfo = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 2,
    hexpand: true,
  });
  const testLockTitle = new Gtk.Label({
    label: "Test Lock Screen",
    halign: Gtk.Align.START,
  });
  testLockTitle.add_css_class("prefs-row-title");
  const testLockSub = new Gtk.Label({
    label: "Saves current settings and launches lock screen",
    halign: Gtk.Align.START,
  });
  testLockSub.add_css_class("prefs-muted");
  testLockInfo.append(testLockTitle);
  testLockInfo.append(testLockSub);

  const testLockBtn = new Gtk.Button({ label: "Lock Now" });
  testLockBtn.add_css_class("prefs-btn");
  testLockBtn.connect("clicked", () => {
    const current = ctx.getConfig();
    saveConfig(current);
    const showError = (detail: string) => new Gtk.AlertDialog({
      message: "Could not lock the screen",
      detail,
    }).show(win);
    execAsync(["palobar-ctl", "lock"]).then((response) => {
      const detail = response.trim();
      if (detail !== "ok") showError(detail || "Palobar did not confirm the lock request.");
    }, (error) => showError(String(error)));
  });
  testLockRow.append(testLockInfo);
  testLockRow.append(testLockBtn);

  lockGroup.append(previewRow);
  lockGroup.append(lockImageRow);
  lockGroup.append(lockColorRow);
  lockGroup.append(autoLockRow);
  lockGroup.append(testLockRow);
  securityPage.append(lockGroup);

  return new Gtk.ScrolledWindow({
    hscrollbar_policy: Gtk.PolicyType.NEVER,
    child: securityPage,
  });
}
