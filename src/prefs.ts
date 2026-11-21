import app from "ags/gtk4/app";
import { Gdk, Gtk } from "ags/gtk4";
import { execAsync } from "ags/process";
import GLib from "gi://GLib";
import { getAccentColor, isDarkStyle, subscribeAppearance } from "./services/appearance";
import {
  CONFIG_PATH,
  loadConfig,
  saveConfig,
  type PalobarConfig,
} from "./services/config";
import { PREFS_CSS, type PrefsContext } from "./prefs/ui";
import { buildOutputPage } from "./prefs/output";
import { buildNiriPage } from "./prefs/niri";
import { buildSoundPage } from "./prefs/sound";
import { buildPowerPage } from "./prefs/power";
import { buildAppearancePage } from "./prefs/appearance";
import { buildSecurityPage } from "./prefs/security";
import { buildKeyboardPage } from "./prefs/keyboard";
import { buildPrintersPage } from "./prefs/printers";

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
    icon_name: "io.Astal.palobar-prefs",
    default_width: 740,
    default_height: 540,
  });
  win.add_css_class("prefs-window");

  const accentProvider = new Gtk.CssProvider();
  Gtk.StyleContext.add_provider_for_display(
    Gdk.Display.get_default()!,
    accentProvider,
    Gtk.STYLE_PROVIDER_PRIORITY_USER + 10,
  );

  const unsubscribeStyle = subscribeAppearance("color-scheme", () => {
    if (isDarkStyle()) {
      win.add_css_class("dark");
    } else {
      win.remove_css_class("dark");
    }
  });
  const unsubscribeAccent = subscribeAppearance("accent-color", () => {
    const accent = getAccentColor().hex;
    accentProvider.load_from_string(`
      .prefs-style-card.selected { border-color: ${accent}; }
      .prefs-lock-unavailable { color: ${accent}; font-weight: bold; }
    `);
  });

  const stack = new Gtk.Stack({
    transition_type: Gtk.StackTransitionType.CROSSFADE,
    transition_duration: 150,
    vexpand: true,
    hexpand: true,
  });

  stack.add_named(buildOutputPage(win), "output");
  stack.add_named(buildSoundPage(), "sound");
  stack.add_named(buildPowerPage(ctx), "power");
  stack.add_named(buildNiriPage(win), "niri");
  stack.add_named(buildAppearancePage(win, ctx), "appearance");
  stack.add_named(buildKeyboardPage(win), "keyboard");
  stack.add_named(buildPrintersPage(), "printers");
  stack.add_named(buildSecurityPage(win, ctx), "security");

  const gnomeSettingsButton = new Gtk.Button({
    label: "Open GNOME Settings",
    halign: Gtk.Align.START,
    valign: Gtk.Align.START,
  });
  gnomeSettingsButton.add_css_class("prefs-btn");
  gnomeSettingsButton.connect("clicked", () => {
    execAsync(["env", "XDG_CURRENT_DESKTOP=GNOME", "gnome-control-center"]).catch((err) => {
      console.error("[prefs] failed to open GNOME Settings:", err);
      new Gtk.AlertDialog({
        message: "Could not open GNOME Settings",
        detail: "Make sure gnome-control-center is installed and available in PATH.",
      }).show(win);
    });
  });
  stack.add_named(gnomeSettingsButton, "fallback");

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
      id: "output",
      label: "Output",
      icon: "video-display-symbolic",
    },
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
      id: "niri",
      label: "Niri",
      icon: "text-x-generic-symbolic",
    },
    {
      id: "appearance",
      label: "Appearance",
      icon: "preferences-desktop-wallpaper-symbolic",
    },
    {
      id: "keyboard",
      label: "Keyboard",
      icon: "input-keyboard-symbolic",
    },
    {
      id: "printers",
      label: "Printers",
      icon: "printer-symbolic",
    },
    {
      id: "security",
      label: "Security",
      icon: "system-lock-screen-symbolic",
    },
    {
      id: "fallback",
      label: "Fallback",
      icon: "preferences-system-symbolic",
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
    unsubscribeStyle();
    unsubscribeAccent();
    app.quit();
    return false;
  });

  app.add_window(win);
  win.present();
}

app.start({
  instanceName: "palobar-prefs",
  css: PREFS_CSS,
  requestHandler(_args, respond) {
    const win = app.get_windows()[0];
    if (win) win.present();
    else buildPrefsWindow();
    respond("");
  },
  main() {
    buildPrefsWindow();
  },
});
