import { Gtk } from "ags/gtk4";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";
import { saveConfig } from "../services/config";
import { makeGroupBox, makeRow, pickImage, type PrefsContext } from "./ui";

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

  const lockGroup = makeGroupBox();

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
  const lockColorEntry = new Gtk.Entry({
    text: cfg.lockscreen.color,
    width_chars: 10,
  });
  lockColorEntry.add_css_class("prefs-entry");
  lockColorEntry.connect("changed", () => {
    const current = ctx.getConfig();
    ctx.updateConfigDebounced({
      ...current,
      lockscreen: { ...current.lockscreen, color: lockColorEntry.text },
    });
  });
  lockColorRow.append(lockColorLabel);
  lockColorRow.append(lockColorEntry);

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
    execAsync(["palobar-ctl", "lock"]).catch(() => {
      const fallbackColor =
        current.lockscreen.color.replace("#", "") || "131316";
      const args = ["swaylock", "-c", fallbackColor];
      const img = current.lockscreen.image || current.wallpaper.path;
      if (img && GLib.file_test(img, GLib.FileTest.EXISTS)) {
        args.push("-i", img, "-s", "fill");
      }
      execAsync(args).catch((err) => console.error(err));
    });
  });
  testLockRow.append(testLockInfo);
  testLockRow.append(testLockBtn);

  lockGroup.append(lockImageRow);
  lockGroup.append(lockColorRow);
  lockGroup.append(testLockRow);
  securityPage.append(lockGroup);

  return securityPage;
}
