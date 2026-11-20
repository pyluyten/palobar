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

function trackInputRegion(win: Gtk.Window, widgets: Gtk.Widget[]) {
  win.connect("realize", () => {
    const surface = win.get_surface();
    const clock = win.get_frame_clock();
    if (!surface || !clock) return;

    let previousBounds = "[]";
    surface.set_input_region(new Cairo.Region());

    // The dock is populated asynchronously, so measure after GTK allocates it.
    const layoutHandler = clock.connect_after("layout", () => {
      const rectangles = widgets.flatMap((widget) => {
        if (!widget.get_mapped()) return [];
        const [ok, bounds] = widget.compute_bounds(win);
        if (!ok || bounds.get_width() <= 0 || bounds.get_height() <= 0) return [];

        const x = Math.floor(bounds.get_x());
        const y = Math.floor(bounds.get_y());
        return [{
          x,
          y,
          width: Math.ceil(bounds.get_x() + bounds.get_width()) - x,
          height: Math.ceil(bounds.get_y() + bounds.get_height()) - y,
        }];
      });

      const currentBounds = JSON.stringify(rectangles);
      if (currentBounds === previousBounds) return;
      previousBounds = currentBounds;

      const region = new Cairo.Region();
      for (const rectangle of rectangles) region.unionRectangle(rectangle);
      surface.set_input_region(region);
    });

    const unrealizeHandler = win.connect("unrealize", () => {
      clock.disconnect(layoutHandler);
      win.disconnect(unrealizeHandler);
    });
  });
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
  });

  const container = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.FILL,
    margin_top: 42,
    margin_bottom: 24,
  });

  const spacer = new Gtk.Box({ vexpand: true });
  container.append(searchEntry);
  container.append(spacer);
  container.append(dockBox);

  const win = (
    <window
      visible={false}
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

  trackInputRegion(win, [searchEntry, dockBox]);
  win.add_controller(keyCtrl);
  win.set_visible(true);
  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    if (alive) searchEntry.grab_focus();
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
