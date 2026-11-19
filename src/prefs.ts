import app from "ags/gtk4/app";
import { Gdk, Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import AstalWp from "gi://AstalWp";
import { execAsync } from "ags/process";
import {
  CONFIG_PATH,
  loadConfig,
  saveConfig,
  type PalobarConfig,
  type WallpaperMode,
} from "./services/config";

const MODES: WallpaperMode[] = ["fill", "fit", "stretch", "center", "tile"];

const BLANK_DELAYS: Array<{ label: string; minutes: number }> = [
  { label: "1 minute", minutes: 1 },
  { label: "2 minutes", minutes: 2 },
  { label: "5 minutes", minutes: 5 },
  { label: "10 minutes", minutes: 10 },
  { label: "15 minutes", minutes: 15 },
  { label: "30 minutes", minutes: 30 },
  { label: "1 hour", minutes: 60 },
];

const SUSPEND_DELAYS: Array<{ label: string; minutes: number }> = [
  { label: "15 minutes", minutes: 15 },
  { label: "30 minutes", minutes: 30 },
  { label: "45 minutes", minutes: 45 },
  { label: "1 hour", minutes: 60 },
  { label: "2 hours", minutes: 120 },
];

const PREFS_CSS = `
window.prefs-window {
  background-color: #fafafa;
  color: #2e2e32;
  font-family: "Cantarell", "Inter", sans-serif;
  font-size: 13.5px;
}

.prefs-sidebar {
  background-color: #f2f2f4;
  border-right: 1px solid rgba(0, 0, 0, 0.08);
  min-width: 210px;
  padding: 14px 10px;
}

.prefs-sidebar-title {
  font-size: 16px;
  font-weight: 700;
  color: #2e2e32;
  padding: 4px 10px 12px 10px;
}

.prefs-nav-btn {
  background: transparent;
  background-image: none;
  box-shadow: none;
  border: none;
  border-radius: 10px;
  padding: 9px 12px;
  color: rgba(46, 46, 50, 0.85);
  font-weight: 500;
}

.prefs-nav-btn:hover {
  background-color: rgba(0, 0, 0, 0.05);
}

.prefs-nav-btn.active {
  background-color: rgba(0, 0, 0, 0.09);
  color: #1c1c20;
  font-weight: 700;
}

.prefs-content-pane {
  background-color: #fafafa;
}

.prefs-page-header {
  font-size: 18px;
  font-weight: 700;
  color: #2e2e32;
  padding: 18px 24px 8px 24px;
}

.prefs-page-body {
  padding: 8px 24px 16px 24px;
}

.prefs-group-title {
  font-size: 13.5px;
  font-weight: 700;
  color: #2e2e32;
  margin-bottom: 6px;
}

.prefs-group {
  background-color: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
}

.prefs-row {
  padding: 12px 16px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}

.prefs-row.last {
  border-bottom: none;
}

.prefs-row-title {
  font-weight: 500;
  color: #2e2e32;
}

.prefs-muted {
  font-size: 12px;
  color: rgba(46, 46, 50, 0.62);
}

.prefs-btn {
  background-color: #e8e8ec;
  background-image: none;
  box-shadow: none;
  border: none;
  border-radius: 9px;
  padding: 7px 14px;
  color: #2e2e32;
}

.prefs-btn:hover {
  background-color: #dcdce2;
}

.prefs-btn.primary {
  background-color: #3584e4;
  color: #ffffff;
  font-weight: 600;
}

.prefs-btn.primary:hover {
  background-color: #2a76d4;
}

.prefs-entry {
  background-color: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.16);
  border-radius: 8px;
  padding: 6px 10px;
  color: #2e2e32;
}

.prefs-footer {
  border-top: 1px solid rgba(0, 0, 0, 0.08);
  padding: 6px 24px;
  background-color: #f2f2f4;
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

function makeGroupBox(): Gtk.Box {
  const box = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL });
  box.add_css_class("prefs-group");
  return box;
}

function makeRow(isLast = false): Gtk.Box {
  const row = new Gtk.Box({
    spacing: 12,
    valign: Gtk.Align.CENTER,
  });
  row.add_css_class("prefs-row");
  if (isLast) row.add_css_class("last");
  return row;
}

function buildPrefsWindow() {
  let cfg: PalobarConfig = loadConfig();

  const updateConfig = (next: PalobarConfig) => {
    cfg = next;
    saveConfig(cfg);
  };

  let colorDebounce: number | null = null;
  const updateConfigDebounced = (next: PalobarConfig) => {
    cfg = next;
    if (colorDebounce !== null) GLib.source_remove(colorDebounce);
    colorDebounce = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
      colorDebounce = null;
      saveConfig(cfg);
      return GLib.SOURCE_REMOVE;
    });
  };

  const win = new Gtk.Window({
    title: "Palobar Settings",
    default_width: 740,
    default_height: 480,
  });
  win.add_css_class("prefs-window");

  // --- Pages Stack ---
  const stack = new Gtk.Stack({
    transition_type: Gtk.StackTransitionType.CROSSFADE,
    transition_duration: 150,
    vexpand: true,
    hexpand: true,
  });

  // === Page 1: Appearance (Wallpaper) ===
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
      updateConfig({ ...cfg, wallpaper: { ...cfg.wallpaper, path } });
      wpPathLabel.label = path;
    });
  });

  const wpClearBtn = new Gtk.Button({ label: "Clear" });
  wpClearBtn.add_css_class("prefs-btn");
  wpClearBtn.connect("clicked", () => {
    updateConfig({ ...cfg, wallpaper: { ...cfg.wallpaper, path: "" } });
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
    updateConfig({ ...cfg, wallpaper: { ...cfg.wallpaper, mode } });
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
    updateConfigDebounced({
      ...cfg,
      wallpaper: { ...cfg.wallpaper, color: wpColorEntry.text },
    });
  });
  wpColorRow.append(wpColorLabel);
  wpColorRow.append(wpColorEntry);

  wpGroup.append(wpImageRow);
  wpGroup.append(wpModeRow);
  wpGroup.append(wpColorRow);
  appearancePage.append(wpGroup);

  // === Page 2: Security (Lock Screen) ===
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
      updateConfig({ ...cfg, lockscreen: { ...cfg.lockscreen, image: path } });
      lockPathLabel.label = path;
    });
  });

  const lockClearBtn = new Gtk.Button({ label: "Reset" });
  lockClearBtn.add_css_class("prefs-btn");
  lockClearBtn.connect("clicked", () => {
    updateConfig({ ...cfg, lockscreen: { ...cfg.lockscreen, image: "" } });
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
    updateConfigDebounced({
      ...cfg,
      lockscreen: { ...cfg.lockscreen, color: lockColorEntry.text },
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
    label: "Saves current settings and launches swaylock",
    halign: Gtk.Align.START,
  });
  testLockSub.add_css_class("prefs-muted");
  testLockInfo.append(testLockTitle);
  testLockInfo.append(testLockSub);

  const testLockBtn = new Gtk.Button({ label: "Lock Now" });
  testLockBtn.add_css_class("prefs-btn");
  testLockBtn.connect("clicked", () => {
    saveConfig(cfg);
    execAsync(["palobar-ctl", "lock"]).catch(() => {
      const fallbackColor = cfg.lockscreen.color.replace("#", "") || "131316";
      const args = ["swaylock", "-c", fallbackColor];
      const img = cfg.lockscreen.image || cfg.wallpaper.path;
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

  stack.add_named(appearancePage, "appearance");

  // === Page 2: Power ===
  const powerPage = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 14,
  });
  powerPage.add_css_class("prefs-page-body");

  const powerGroupTitle = new Gtk.Label({
    label: "Power Saving",
    halign: Gtk.Align.START,
  });
  powerGroupTitle.add_css_class("prefs-group-title");
  powerPage.append(powerGroupTitle);

  // Group 1: Screen Blank
  const blankGroup = makeGroupBox();

  const blankToggleRow = makeRow(false);
  const blankInfo = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 2,
    hexpand: true,
  });
  const blankLabel = new Gtk.Label({
    label: "Automatic Screen Blank",
    halign: Gtk.Align.START,
  });
  blankLabel.add_css_class("prefs-row-title");
  const blankSub = new Gtk.Label({
    label: "Turn off the screen after a period of inactivity",
    halign: Gtk.Align.START,
  });
  blankSub.add_css_class("prefs-muted");
  blankInfo.append(blankLabel);
  blankInfo.append(blankSub);

  const blankSwitch = new Gtk.Switch({
    active: cfg.power.screenBlankEnabled,
    valign: Gtk.Align.CENTER,
  });
  blankToggleRow.append(blankInfo);
  blankToggleRow.append(blankSwitch);

  const blankDelayRow = makeRow(true);
  const blankDelayLabel = new Gtk.Label({
    label: "Delay",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  blankDelayLabel.add_css_class("prefs-row-title");
  const blankDropdown = Gtk.DropDown.new_from_strings(
    BLANK_DELAYS.map((d) => d.label),
  );
  const blankIdx = BLANK_DELAYS.findIndex(
    (d) => d.minutes === cfg.power.screenBlankMinutes,
  );
  blankDropdown.selected = blankIdx >= 0 ? blankIdx : 3;
  blankDelayRow.sensitive = cfg.power.screenBlankEnabled;

  blankSwitch.connect("notify::active", () => {
    updateConfig({
      ...cfg,
      power: { ...cfg.power, screenBlankEnabled: blankSwitch.active },
    });
    blankDelayRow.sensitive = blankSwitch.active;
  });
  blankDropdown.connect("notify::selected", () => {
    const minutes = BLANK_DELAYS[blankDropdown.selected]?.minutes ?? 10;
    updateConfig({
      ...cfg,
      power: { ...cfg.power, screenBlankMinutes: minutes },
    });
  });

  blankDelayRow.append(blankDelayLabel);
  blankDelayRow.append(blankDropdown);

  blankGroup.append(blankToggleRow);
  blankGroup.append(blankDelayRow);
  powerPage.append(blankGroup);

  // Group 2: Automatic Suspend
  const suspendGroup = makeGroupBox();

  const suspendToggleRow = makeRow(false);
  const suspendLabel = new Gtk.Label({
    label: "Automatic Suspend",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  suspendLabel.add_css_class("prefs-row-title");

  const suspendSwitch = new Gtk.Switch({
    active: cfg.power.autoSuspendEnabled,
    valign: Gtk.Align.CENTER,
  });
  suspendToggleRow.append(suspendLabel);
  suspendToggleRow.append(suspendSwitch);

  const suspendDelayRow = makeRow(true);
  const suspendDelayLabel = new Gtk.Label({
    label: "Delay",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  suspendDelayLabel.add_css_class("prefs-row-title");
  const suspendDropdown = Gtk.DropDown.new_from_strings(
    SUSPEND_DELAYS.map((d) => d.label),
  );
  const suspendIdx = SUSPEND_DELAYS.findIndex(
    (d) => d.minutes === cfg.power.autoSuspendMinutes,
  );
  suspendDropdown.selected = suspendIdx >= 0 ? suspendIdx : 4;
  suspendDelayRow.sensitive = cfg.power.autoSuspendEnabled;

  suspendSwitch.connect("notify::active", () => {
    updateConfig({
      ...cfg,
      power: { ...cfg.power, autoSuspendEnabled: suspendSwitch.active },
    });
    suspendDelayRow.sensitive = suspendSwitch.active;
  });
  suspendDropdown.connect("notify::selected", () => {
    const minutes = SUSPEND_DELAYS[suspendDropdown.selected]?.minutes ?? 120;
    updateConfig({
      ...cfg,
      power: { ...cfg.power, autoSuspendMinutes: minutes },
    });
  });

  suspendDelayRow.append(suspendDelayLabel);
  suspendDelayRow.append(suspendDropdown);

  suspendGroup.append(suspendToggleRow);
  suspendGroup.append(suspendDelayRow);
  powerPage.append(suspendGroup);

  stack.add_named(powerPage, "power");

  // === Page 3: Sound (Output & Input) ===
  const soundPage = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 14,
  });
  soundPage.add_css_class("prefs-page-body");

  const wp = AstalWp.get_default();

  // --- Output Section ---
  const outTitle = new Gtk.Label({
    label: "Output",
    halign: Gtk.Align.START,
  });
  outTitle.add_css_class("prefs-group-title");
  soundPage.append(outTitle);

  const outGroup = makeGroupBox();

  const outDevRow = makeRow(false);
  const outDevLabel = new Gtk.Label({
    label: "Output Device",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  outDevLabel.add_css_class("prefs-row-title");
  const outDropdown = Gtk.DropDown.new_from_strings(["No Output Device"]);
  outDevRow.append(outDevLabel);
  outDevRow.append(outDropdown);

  const outVolRow = makeRow(true);
  const outVolLabel = new Gtk.Label({
    label: "Output Volume",
    halign: Gtk.Align.START,
  });
  outVolLabel.add_css_class("prefs-row-title");

  const outMuteIcon = new Gtk.Image({
    icon_name: "audio-volume-medium-symbolic",
  });
  const outMuteBtn = new Gtk.Button({ child: outMuteIcon });
  outMuteBtn.add_css_class("prefs-btn");

  const outScale = Gtk.Scale.new_with_range(
    Gtk.Orientation.HORIZONTAL,
    0,
    1,
    0.01,
  );
  outScale.set_hexpand(true);
  outScale.set_draw_value(false);

  const outPctLabel = new Gtk.Label({
    label: "0%",
    width_chars: 4,
    halign: Gtk.Align.END,
  });
  outPctLabel.add_css_class("prefs-muted");

  outVolRow.append(outVolLabel);
  outVolRow.append(outMuteBtn);
  outVolRow.append(outScale);
  outVolRow.append(outPctLabel);

  outGroup.append(outDevRow);
  outGroup.append(outVolRow);
  soundPage.append(outGroup);

  // --- Input Section ---
  const inTitle = new Gtk.Label({
    label: "Input",
    halign: Gtk.Align.START,
  });
  inTitle.add_css_class("prefs-group-title");
  soundPage.append(inTitle);

  const inGroup = makeGroupBox();

  const inDevRow = makeRow(false);
  const inDevLabel = new Gtk.Label({
    label: "Input Device",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  inDevLabel.add_css_class("prefs-row-title");
  const inDropdown = Gtk.DropDown.new_from_strings(["No Input Device"]);
  inDevRow.append(inDevLabel);
  inDevRow.append(inDropdown);

  const inVolRow = makeRow(true);
  const inVolLabel = new Gtk.Label({
    label: "Input Volume",
    halign: Gtk.Align.START,
  });
  inVolLabel.add_css_class("prefs-row-title");

  const inMuteIcon = new Gtk.Image({
    icon_name: "microphone-sensitivity-medium-symbolic",
  });
  const inMuteBtn = new Gtk.Button({ child: inMuteIcon });
  inMuteBtn.add_css_class("prefs-btn");

  const inScale = Gtk.Scale.new_with_range(
    Gtk.Orientation.HORIZONTAL,
    0,
    1,
    0.01,
  );
  inScale.set_hexpand(true);
  inScale.set_draw_value(false);

  const inPctLabel = new Gtk.Label({
    label: "0%",
    width_chars: 4,
    halign: Gtk.Align.END,
  });
  inPctLabel.add_css_class("prefs-muted");

  inVolRow.append(inVolLabel);
  inVolRow.append(inMuteBtn);
  inVolRow.append(inScale);
  inVolRow.append(inPctLabel);

  inGroup.append(inDevRow);
  inGroup.append(inVolRow);
  soundPage.append(inGroup);

  if (wp) {
    const audio = wp.audio;
    let speakers: AstalWp.Endpoint[] = [];
    let mics: AstalWp.Endpoint[] = [];
    let syncingOut = false;
    let syncingIn = false;
    let currentSpeaker: AstalWp.Endpoint | null = null;
    let speakerSignals: number[] = [];
    let currentMic: AstalWp.Endpoint | null = null;
    let micSignals: number[] = [];

    const syncSpeakerValues = () => {
      syncingOut = true;
      if (currentSpeaker) {
        outScale.set_value(currentSpeaker.volume);
        outPctLabel.label = `${Math.round(currentSpeaker.volume * 100)}%`;
        outMuteIcon.icon_name =
          currentSpeaker.volumeIcon || "audio-volume-medium-symbolic";
        outVolRow.sensitive = true;
      } else {
        outScale.set_value(0);
        outPctLabel.label = "0%";
        outVolRow.sensitive = false;
      }
      syncingOut = false;
    };

    const bindDefaultSpeaker = () => {
      if (currentSpeaker) {
        for (const id of speakerSignals) currentSpeaker.disconnect(id);
        speakerSignals = [];
      }
      currentSpeaker = audio.defaultSpeaker;
      if (currentSpeaker) {
        speakerSignals.push(
          currentSpeaker.connect("notify::volume", syncSpeakerValues),
          currentSpeaker.connect("notify::mute", syncSpeakerValues),
          currentSpeaker.connect("notify::volume-icon", syncSpeakerValues),
        );
      }
      syncSpeakerValues();
    };

    const syncSpeakersList = () => {
      syncingOut = true;
      speakers = [...audio.speakers];
      if (speakers.length === 0) {
        outDropdown.model = Gtk.StringList.new(["No Output Device"]);
        outDropdown.sensitive = false;
      } else {
        outDropdown.model = Gtk.StringList.new(
          speakers.map((s) => s.description || s.name || `Device ${s.id}`),
        );
        const defIdx = speakers.findIndex((s) => s.isDefault);
        outDropdown.selected = defIdx >= 0 ? defIdx : 0;
        outDropdown.sensitive = true;
      }
      syncingOut = false;
    };

    outDropdown.connect("notify::selected", () => {
      if (syncingOut) return;
      const target = speakers[outDropdown.selected];
      if (target) target.isDefault = true;
    });

    outScale.connect("value-changed", () => {
      if (syncingOut || !currentSpeaker) return;
      const val = outScale.get_value();
      currentSpeaker.volume = val;
      if (currentSpeaker.mute && val > 0) currentSpeaker.mute = false;
    });

    outMuteBtn.connect("clicked", () => {
      if (currentSpeaker) currentSpeaker.mute = !currentSpeaker.mute;
    });

    const syncMicValues = () => {
      syncingIn = true;
      if (currentMic) {
        inScale.set_value(currentMic.volume);
        inPctLabel.label = `${Math.round(currentMic.volume * 100)}%`;
        inMuteIcon.icon_name = currentMic.mute
          ? "microphone-sensitivity-muted-symbolic"
          : "microphone-sensitivity-medium-symbolic";
        inVolRow.sensitive = true;
      } else {
        inScale.set_value(0);
        inPctLabel.label = "0%";
        inVolRow.sensitive = false;
      }
      syncingIn = false;
    };

    const bindDefaultMic = () => {
      if (currentMic) {
        for (const id of micSignals) currentMic.disconnect(id);
        micSignals = [];
      }
      currentMic = audio.defaultMicrophone;
      if (currentMic) {
        micSignals.push(
          currentMic.connect("notify::volume", syncMicValues),
          currentMic.connect("notify::mute", syncMicValues),
        );
      }
      syncMicValues();
    };

    const syncMicsList = () => {
      syncingIn = true;
      mics = [...audio.microphones];
      if (mics.length === 0) {
        inDropdown.model = Gtk.StringList.new(["No Input Device"]);
        inDropdown.sensitive = false;
      } else {
        inDropdown.model = Gtk.StringList.new(
          mics.map((m) => m.description || m.name || `Device ${m.id}`),
        );
        const defIdx = mics.findIndex((m) => m.isDefault);
        inDropdown.selected = defIdx >= 0 ? defIdx : 0;
        inDropdown.sensitive = true;
      }
      syncingIn = false;
    };

    inDropdown.connect("notify::selected", () => {
      if (syncingIn) return;
      const target = mics[inDropdown.selected];
      if (target) target.isDefault = true;
    });

    inScale.connect("value-changed", () => {
      if (syncingIn || !currentMic) return;
      const val = inScale.get_value();
      currentMic.volume = val;
      if (currentMic.mute && val > 0) currentMic.mute = false;
    });

    inMuteBtn.connect("clicked", () => {
      if (currentMic) currentMic.mute = !currentMic.mute;
    });

    syncSpeakersList();
    bindDefaultSpeaker();
    syncMicsList();
    bindDefaultMic();

    audio.connect("speaker-added", syncSpeakersList);
    audio.connect("speaker-removed", syncSpeakersList);
    audio.connect("notify::default-speaker", () => {
      syncSpeakersList();
      bindDefaultSpeaker();
    });
    audio.connect("microphone-added", syncMicsList);
    audio.connect("microphone-removed", syncMicsList);
    audio.connect("notify::default-microphone", () => {
      syncMicsList();
      bindDefaultMic();
    });
  }

  stack.add_named(soundPage, "sound");
  stack.add_named(securityPage, "security");

  // --- Left Sidebar ---
  const sidebar = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 4,
  });
  sidebar.add_css_class("prefs-sidebar");

  const sidebarTitle = new Gtk.Label({
    label: "Settings",
    halign: Gtk.Align.START,
  });
  sidebarTitle.add_css_class("prefs-sidebar-title");
  sidebar.append(sidebarTitle);

  const pageHeader = new Gtk.Label({
    label: "Appearance",
    halign: Gtk.Align.START,
  });
  pageHeader.add_css_class("prefs-page-header");

  const navItems = [
    {
      id: "sound",
      label: "Sound",
      icon: "multimedia-volume-control-symbolic",
    },
    {
      id: "power",
      label: "Power",
      icon: "battery-good-symbolic",
    },
    {
      id: "appearance",
      label: "Appearance",
      icon: "preferences-desktop-wallpaper-symbolic",
    },
    {
      id: "security",
      label: "Security",
      icon: "system-lock-screen-symbolic",
    },
  ];

  const navButtons = new Map<string, Gtk.Button>();

  const selectPage = (id: string, title: string) => {
    stack.set_visible_child_name(id);
    pageHeader.label = title;
    for (const [key, btn] of navButtons) {
      if (key === id) {
        btn.add_css_class("active");
      } else {
        btn.remove_css_class("active");
      }
    }
  };

  for (const item of navItems) {
    const btn = new Gtk.Button();
    btn.add_css_class("prefs-nav-btn");
    const row = new Gtk.Box({ spacing: 10 });
    row.append(new Gtk.Image({ icon_name: item.icon }));
    row.append(new Gtk.Label({ label: item.label, halign: Gtk.Align.START }));
    btn.set_child(row);
    btn.connect("clicked", () => selectPage(item.id, item.label));
    navButtons.set(item.id, btn);
    sidebar.append(btn);
  }

  selectPage("appearance", "Appearance");

  // --- Right Content Pane + Footer ---
  const rightPane = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    hexpand: true,
    vexpand: true,
  });
  rightPane.add_css_class("prefs-content-pane");

  const footer = new Gtk.Box({ spacing: 10 });
  footer.add_css_class("prefs-footer");

  const configNote = new Gtk.Label({
    label: CONFIG_PATH,
    hexpand: true,
    halign: Gtk.Align.START,
    ellipsize: 2,
  });
  configNote.add_css_class("prefs-muted");

  footer.append(configNote);

  rightPane.append(pageHeader);
  rightPane.append(stack);
  rightPane.append(footer);

  // --- Root Split Layout ---
  const root = new Gtk.Box({
    orientation: Gtk.Orientation.HORIZONTAL,
    hexpand: true,
    vexpand: true,
  });
  root.append(sidebar);
  root.append(rightPane);

  const keyCtrl = new Gtk.EventControllerKey();
  keyCtrl.connect("key-pressed", (_ctrl, keyval, _keycode, state) => {
    if (
      (state & Gdk.ModifierType.CONTROL_MASK) !== 0 &&
      (keyval === Gdk.KEY_q || keyval === Gdk.KEY_Q)
    ) {
      win.close();
      return true;
    }
    return false;
  });
  win.add_controller(keyCtrl);

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
