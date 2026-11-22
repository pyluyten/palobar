import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { getApps, scheduleGc } from "../services/runtime";
import { createMiniDock } from "./Dock";
import { toggleLauncher } from "./Launcher";

let closeDock: (() => void) | null = null;

export function toggleStandaloneDock() {
  if (closeDock) {
    closeDock();
    return;
  }

  const monitor = app.get_monitors()[0];
  if (!monitor) return;

  const dock = createMiniDock({
    appsService: getApps(),
    onActivateApp: () => closeDock?.(),
    acceptFavorites: true,
    applicationsToggle: {
      active: false,
      onToggle: () => {
        closeDock?.();
        toggleLauncher();
      },
    },
  });
  const win = new Astal.Window({
    visible: false,
    name: "standalone-dock",
    application: app,
    gdkmonitor: monitor,
    anchor: Astal.WindowAnchor.BOTTOM,
    layer: Astal.Layer.OVERLAY,
    keymode: Astal.Keymode.ON_DEMAND,
    margin_bottom: 24,
    child: dock.widget,
  });
  win.add_css_class("popup-window");

  const keys = new Gtk.EventControllerKey();
  const keyHandler = keys.connect("key-pressed", (_controller, keyval) => {
    if (keyval !== Gdk.KEY_Escape) return false;
    closeDock?.();
    return true;
  });
  win.add_controller(keys);

  closeDock = () => {
    closeDock = null;
    keys.disconnect(keyHandler);
    dock.dispose();
    win.set_child(null);
    win.destroy();
    scheduleGc();
  };
  win.set_visible(true);
}
