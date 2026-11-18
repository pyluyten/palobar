import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import AstalApps from "gi://AstalApps";
import GLib from "gi://GLib";
import { popups } from "../services/popups";

let launcherWindow: Gtk.Window | null = null;
const stateListeners = new Set<(open: boolean) => void>();

function notifyLauncherState(open: boolean) {
  for (const listener of stateListeners) {
    listener(open);
  }
}

export function onLauncherStateChange(
  listener: (open: boolean) => void,
): () => void {
  stateListeners.add(listener);
  return () => stateListeners.delete(listener);
}

export function closeLauncher() {
  if (launcherWindow) {
    launcherWindow.close();
    launcherWindow.destroy();
    launcherWindow = null;
    notifyLauncherState(false);
  }
}

export function isLauncherOpen(): boolean {
  return launcherWindow !== null;
}

export function openLauncher(monitor?: Gdk.Monitor, initialQuery?: string) {
  if (launcherWindow) return;
  toggleLauncher(monitor, initialQuery);
}

export function toggleLauncher(monitor?: Gdk.Monitor, initialQuery?: string) {
  if (launcherWindow) {
    closeLauncher();
    return;
  }

  popups.close();

  const targetMonitor = monitor ?? app.get_monitors()[0];
  if (!targetMonitor) return;

  const appsService = new AstalApps.Apps();
  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;

  const flowBox = new Gtk.FlowBox({
    valign: Gtk.Align.START,
    halign: Gtk.Align.CENTER,
    homogeneous: true,
    max_children_per_line: 6,
    min_children_per_line: 3,
    row_spacing: 16,
    column_spacing: 16,
    selection_mode: Gtk.SelectionMode.SINGLE,
    activate_on_single_click: true,
  });
  flowBox.add_css_class("launcher-grid");

  let currentApps: AstalApps.Application[] = [];

  const launchApp = (application: AstalApps.Application) => {
    closeLauncher();
    application.launch();
  };

  const renderApps = (query: string) => {
    let child = flowBox.get_first_child();
    while (child) {
      const next = child.get_next_sibling();
      flowBox.remove(child);
      child = next;
    }

    const trimmed = query.trim();
    const list = trimmed
      ? appsService.fuzzy_query(trimmed)
      : [...appsService.list].sort((a, b) => a.name.localeCompare(b.name));

    currentApps = list.slice(0, 42);

    for (const item of currentApps) {
      const tile = new Gtk.Box({
        orientation: Gtk.Orientation.VERTICAL,
        spacing: 8,
        halign: Gtk.Align.CENTER,
        valign: Gtk.Align.CENTER,
      });
      tile.add_css_class("launcher-tile");

      const icon = new Gtk.Image({
        icon_name: item.iconName || "application-x-executable",
        pixel_size: 56,
      });
      icon.add_css_class("launcher-tile-icon");

      const label = new Gtk.Label({
        label: item.name,
        max_width_chars: 14,
        ellipsize: 3,
        justify: Gtk.Justification.CENTER,
      });
      label.add_css_class("launcher-tile-label");

      tile.append(icon);
      tile.append(label);

      const flowChild = new Gtk.FlowBoxChild({
        child: tile,
        tooltip_text: item.description || item.name,
      });
      flowChild.add_css_class("launcher-flow-child");
      flowBox.append(flowChild);
    }

    const first = flowBox.get_child_at_index(0);
    if (first) {
      flowBox.select_child(first);
    }
  };

  flowBox.connect("child-activated", (_box, child) => {
    const idx = child.get_index();
    const selected = currentApps[idx];
    if (selected) {
      launchApp(selected);
    }
  });

  const searchEntry = new Gtk.Entry({
    placeholder_text: "Type to search...",
    primary_icon_name: "system-search-symbolic",
    halign: Gtk.Align.CENTER,
    width_request: 420,
    text: initialQuery ?? "",
  });
  searchEntry.add_css_class("launcher-search");

  searchEntry.connect("changed", () => {
    renderApps(searchEntry.text);
  });

  searchEntry.connect("activate", () => {
    if (currentApps.length > 0) {
      launchApp(currentApps[0]);
    }
  });

  const searchKeyCtrl = new Gtk.EventControllerKey();
  searchKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
    if (keyval === Gdk.KEY_Down) {
      const first = flowBox.get_child_at_index(0);
      if (first) {
        flowBox.select_child(first);
        first.grab_focus();
        return true;
      }
    }
    return false;
  });
  searchEntry.add_controller(searchKeyCtrl);

  renderApps(initialQuery ?? "");

  const scrolled = new Gtk.ScrolledWindow({
    vexpand: true,
    hexpand: true,
    hscrollbar_policy: Gtk.PolicyType.NEVER,
    vscrollbar_policy: Gtk.PolicyType.AUTOMATIC,
    child: flowBox,
  });
  scrolled.add_css_class("launcher-scroll");

  const container = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 28,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.FILL,
    margin_top: 48,
    margin_bottom: 48,
    margin_start: 48,
    margin_end: 48,
    width_request: 860,
  });
  container.add_css_class("launcher-container");
  container.append(searchEntry);
  container.append(scrolled);

  const backdropClick = new Gtk.GestureClick();
  backdropClick.connect("released", (_gesture, _n, x, y) => {
    const picked = launcherWindow?.pick(x, y, Gtk.PickFlags.DEFAULT);
    if (picked === launcherWindow) {
      closeLauncher();
    }
  });

  const win = (
    <window
      visible
      name="activities-launcher"
      class="launcher-window"
      gdkmonitor={targetMonitor}
      layer={Astal.Layer.OVERLAY}
      exclusivity={Astal.Exclusivity.IGNORE}
      keymode={Astal.Keymode.EXCLUSIVE}
      anchor={TOP | BOTTOM | LEFT | RIGHT}
      application={app}
    >
      {container}
    </window>
  ) as Gtk.Window;

  const winKeyCtrl = new Gtk.EventControllerKey();
  winKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
    if (keyval === Gdk.KEY_Escape) {
      closeLauncher();
      return true;
    }
    return false;
  });
  win.add_controller(winKeyCtrl);
  win.add_controller(backdropClick);

  launcherWindow = win;
  notifyLauncherState(true);

  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    searchEntry.grab_focus();
    searchEntry.set_position(-1);
    return GLib.SOURCE_REMOVE;
  });
}
