import { Gtk } from "ags/gtk4";
import { makeGroupBox, makeRow, type PrefsContext } from "./ui";

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

export function buildPowerPage(ctx: PrefsContext): Gtk.Widget {
  const cfg = ctx.getConfig();

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
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      power: { ...current.power, screenBlankEnabled: blankSwitch.active },
    });
    blankDelayRow.sensitive = blankSwitch.active;
  });
  blankDropdown.connect("notify::selected", () => {
    const minutes = BLANK_DELAYS[blankDropdown.selected]?.minutes ?? 10;
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      power: { ...current.power, screenBlankMinutes: minutes },
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
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      power: { ...current.power, autoSuspendEnabled: suspendSwitch.active },
    });
    suspendDelayRow.sensitive = suspendSwitch.active;
  });
  suspendDropdown.connect("notify::selected", () => {
    const minutes = SUSPEND_DELAYS[suspendDropdown.selected]?.minutes ?? 120;
    const current = ctx.getConfig();
    ctx.updateConfig({
      ...current,
      power: { ...current.power, autoSuspendMinutes: minutes },
    });
  });

  suspendDelayRow.append(suspendDelayLabel);
  suspendDelayRow.append(suspendDropdown);

  suspendGroup.append(suspendToggleRow);
  suspendGroup.append(suspendDelayRow);
  powerPage.append(suspendGroup);

  return powerPage;
}
