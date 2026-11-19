import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { execAsync } from "ags/process";
import Cairo from "gi://cairo";
import GLib from "gi://GLib";
import { niri } from "../services/niri";
import { popups } from "../services/popups";
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

function createCaptureWindow(monitor: Gdk.Monitor, index: number): Gtk.Window {
  const { TOP, LEFT } = Astal.WindowAnchor;

  const entry = new Gtk.Entry({
    width_request: 1,
    height_request: 1,
    halign: Gtk.Align.START,
    valign: Gtk.Align.START,
    opacity: 0,
  });

  const win = (
    <window
      visible
      name={`overview-capture-${index}`}
      class="overview-capture-window"
      gdkmonitor={monitor}
      layer={Astal.Layer.OVERLAY}
      exclusivity={Astal.Exclusivity.IGNORE}
      keymode={Astal.Keymode.EXCLUSIVE}
      anchor={TOP | LEFT}
      application={app}
    >
      {entry}
    </window>
  ) as Gtk.Window;

  const keyCtrl = new Gtk.EventControllerKey({
    propagation_phase: Gtk.PropagationPhase.CAPTURE,
  });
  keyCtrl.connect("key-pressed", (_ctrl, keyval, _keycode, state) => {
    console.log("[overview-capture] key-pressed:", keyval, state);
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

    return true;
  });

  win.connect("realize", () => {
    const surface = win.get_surface();
    if (surface) {
      surface.set_input_region(new Cairo.Region());
    }
  });

  win.connect("notify::is-active", () => {
    console.log("[overview-capture] win.is_active:", win.is_active);
  });

  win.add_controller(keyCtrl);
  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    const surface = win.get_surface();
    if (surface) {
      surface.set_input_region(new Cairo.Region());
    }
    const ok = entry.grab_focus();
    console.log(
      "[overview-capture] entry.grab_focus():",
      ok,
      "is_active:",
      win.is_active,
    );
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

  console.log(
    "[overview-capture] syncCapture overviewOpen=",
    overviewOpen,
    "shouldCapture=",
    shouldCapture,
    "is launcher opened=",
    isLauncherOpen(),
    "popups.active=",
    popups.active.get(),
  );

  if (!shouldCapture) {
    destroyCaptureWindows();
    return;
  }

  if (captureWindows.length > 0) return;

  const monitors = app.get_monitors();
  captureWindows = monitors.map((m, idx) => createCaptureWindow(m, idx));

  GLib.timeout_add(GLib.PRIORITY_DEFAULT, 300, () => {
    execAsync(["niri", "msg", "-j", "layers"])
      .then((out) => console.log("[overview-capture] layers:", out))
      .catch((err) => console.error("[overview-capture] layers err:", err));
    return GLib.SOURCE_REMOVE;
  });
}

export function initOverviewCapture() {
  niri.subscribeOverview(() => syncCapture());
  onLauncherStateChange(() => syncCapture());
  popups.active.subscribe(() => syncCapture());
}
