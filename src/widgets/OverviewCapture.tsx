import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import AstalApps from "gi://AstalApps";
import Cairo from "gi://cairo";
import GLib from "gi://GLib";
import { niri } from "../services/niri";
import { popups } from "../services/popups";
import { createMiniDock } from "./Dock";
import {
  closeLauncher,
  isLauncherOpen,
  onLauncherStateChange,
  openLauncher,
} from "./Launcher";

let captureWindows: Gtk.Window[] = [];
let openedFromOverview = false;

function destroyCaptureWindows() {
  for (const win of captureWindows) {
    win.close();
    win.destroy();
  }
  captureWindows = [];
}

function isTypingKey(keyval: number, state: number): string | null {
  const mods =
    state &
    (Gdk.ModifierType.CONTROL_MASK |
      Gdk.ModifierType.ALT_MASK |
      Gdk.ModifierType.SUPER_MASK);
  if (mods !== 0) return null;

  const codepoint = Gdk.keyval_to_unicode(keyval);
  if (codepoint <= 0x20 || codepoint === 0x7f) return null;

  return String.fromCodePoint(codepoint);
}

function updateInputRegion(win: Gtk.Window, widgets: Gtk.Widget[]) {
  const surface = win.get_surface();
  if (!surface) return;

  const region = new Cairo.Region();
  for (const w of widgets) {
    if (!w.get_visible()) continue;
    const [ok, bounds] = w.compute_bounds(win);
    if (!ok) continue;
    region.unionRectangle({
      x: Math.floor(bounds.get_x()),
      y: Math.floor(bounds.get_y()),
      width: Math.ceil(bounds.get_width()),
      height: Math.ceil(bounds.get_height()),
    });
  }
  surface.set_input_region(region);
}

function createCaptureWindow(monitor: Gdk.Monitor, index: number): Gtk.Window {
  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;
  const appsService = new AstalApps.Apps();

  const searchEntry = new Gtk.Entry({
    placeholder_text: "Type to search",
    primary_icon_name: "system-search-symbolic",
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.START,
    width_request: 400,
  });
  searchEntry.add_css_class("launcher-search");

  let alive = true;

  const dockBox = createMiniDock({
    appsService,
    isAlive: () => alive,
    onFocusWindow: () => destroyCaptureWindows(),
    onShowApps: () => {
      destroyCaptureWindows();
      openedFromOverview = true;
      openLauncher(monitor);
    },
    onPopulated: () => {
      GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
        if (alive) updateInputRegion(win, [searchEntry, dockBox]);
        return GLib.SOURCE_REMOVE;
      });
    },
  });

  const container = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.FILL,
    margin_top: 24,
    margin_bottom: 24,
  });

  const spacer = new Gtk.Box({ vexpand: true });
  container.append(searchEntry);
  container.append(spacer);
  container.append(dockBox);

  const win = (
    <window
      visible
      name={`overview-capture-${index}`}
      class="overview-capture-window"
      gdkmonitor={monitor}
      layer={Astal.Layer.OVERLAY}
      exclusivity={Astal.Exclusivity.IGNORE}
      keymode={Astal.Keymode.EXCLUSIVE}
      anchor={TOP | BOTTOM | LEFT | RIGHT}
      application={app}
    >
      {container}
    </window>
  ) as Gtk.Window;

  const keyCtrl = new Gtk.EventControllerKey({
    propagation_phase: Gtk.PropagationPhase.CAPTURE,
  });
  keyCtrl.connect("key-pressed", (_ctrl, keyval, _keycode, state) => {
    if (
      keyval === Gdk.KEY_Escape ||
      keyval === Gdk.KEY_Return ||
      keyval === Gdk.KEY_KP_Enter
    ) {
      niri.sendAction("toggle-overview");
      return true;
    }
    if (keyval === Gdk.KEY_Left) {
      niri.sendAction("focus-column-left");
      return true;
    }
    if (keyval === Gdk.KEY_Right) {
      niri.sendAction("focus-column-right");
      return true;
    }
    if (keyval === Gdk.KEY_Up) {
      niri.sendAction("focus-workspace-up");
      return true;
    }
    if (keyval === Gdk.KEY_Down) {
      niri.sendAction("focus-workspace-down");
      return true;
    }

    const ch = isTypingKey(keyval, state);
    if (ch) {
      destroyCaptureWindows();
      openedFromOverview = true;
      openLauncher(monitor, ch);
      return true;
    }

    return false;
  });

  win.connect("destroy", () => {
    alive = false;
  });

  win.connect("realize", () => {
    updateInputRegion(win, [searchEntry, dockBox]);
  });

  win.add_controller(keyCtrl);
  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    if (alive) {
      updateInputRegion(win, [searchEntry, dockBox]);
      searchEntry.grab_focus();
    }
    return GLib.SOURCE_REMOVE;
  });
  return win;
}

function syncCapture() {
  const overviewOpen = niri.isOverviewOpen();

  if (openedFromOverview) {
    if (!isLauncherOpen()) {
      openedFromOverview = false;
    } else if (!overviewOpen) {
      openedFromOverview = false;
      destroyCaptureWindows();
      closeLauncher();
      return;
    }
  }

  const shouldCapture =
    overviewOpen && !isLauncherOpen() && popups.active.get() === null;

  if (!shouldCapture) {
    destroyCaptureWindows();
    return;
  }

  if (captureWindows.length > 0) return;

  const monitors = app.get_monitors();
  captureWindows = monitors.map((m, idx) => createCaptureWindow(m, idx));
}

export function initOverviewCapture() {
  niri.subscribeOverview(() => syncCapture());
  onLauncherStateChange(() => syncCapture());
  popups.active.subscribe(() => syncCapture());
}
