import app from "ags/gtk4/app";
import { Gdk, Gtk } from "ags/gtk4";
import GLib from "gi://GLib";
import {
  CONFIG_PATH,
  loadConfig,
  saveConfig,
  type PalobarConfig,
} from "./services/config";
import { PREFS_CSS, type PrefsContext } from "./prefs/ui";
import { buildSoundPage } from "./prefs/sound";
import { buildPowerPage } from "./prefs/power";
import { buildAppearancePage } from "./prefs/appearance";
import { buildSecurityPage } from "./prefs/security";

function buildPrefsWindow() {
  let cfg: PalobarConfig = loadConfig();
  let colorDebounce: number | null = null;

  const ctx: PrefsContext = {
    getConfig: () => cfg,
    updateConfig: (next) => {
      cfg = next;
      saveConfig(cfg);
    },
    updateConfigDebounced: (next) => {
      cfg = next;
      if (colorDebounce !== null) GLib.source_remove(colorDebounce);
      colorDebounce = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
        colorDebounce = null;
        saveConfig(cfg);
        return GLib.SOURCE_REMOVE;
      });
    },
  };

  const win = new Gtk.Window({
    title: "Palobar Settings",
    default_width: 740,
    default_height: 480,
  });
  win.add_css_class("prefs-window");

  const stack = new Gtk.Stack({
    transition_type: Gtk.StackTransitionType.CROSSFADE,
    transition_duration: 150,
    vexpand: true,
    hexpand: true,
  });

  stack.add_named(buildSoundPage(), "sound");
  stack.add_named(buildPowerPage(ctx), "power");
  stack.add_named(buildAppearancePage(win, ctx), "appearance");
  stack.add_named(buildSecurityPage(win, ctx), "security");

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

  // --- Right Content Pane + Minimal Footer ---
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
