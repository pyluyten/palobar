import { Gtk } from "ags/gtk4";
import { execAsync } from "ags/process";
import { makeGroupBox, makeRow } from "./ui";

interface KeyboardLayouts {
  names: string[];
  current_idx: number;
}

export function buildKeyboardPage(win: Gtk.Window): Gtk.Widget {
  const page = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 12 });
  page.add_css_class("prefs-page-body");

  const title = new Gtk.Label({ label: "Keyboard Layouts", xalign: 0 });
  title.add_css_class("prefs-group-title");
  page.append(title);

  const group = makeGroupBox();
  page.append(group);

  const showStatus = (message: string) => {
    const row = makeRow(true);
    row.append(new Gtk.Label({ label: message, xalign: 0, wrap: true }));
    group.append(row);
  };
  showStatus("Loading keyboard layouts…");

  let switching = false;
  let requestId = 0;

  const clearLayouts = () => {
    for (let child = group.get_first_child(); child; child = group.get_first_child()) {
      group.remove(child);
    }
  };

  const alert = (message: string, err: unknown) => {
    console.error("[keyboard]", message, err);
    new Gtk.AlertDialog({ message, detail: String(err) }).show(win);
  };

  const refresh = async () => {
    const request = ++requestId;
    try {
      const json = await execAsync(["niri", "msg", "--json", "keyboard-layouts"]);
      const { names, current_idx } = JSON.parse(json) as KeyboardLayouts;
      if (request !== requestId) return;
      clearLayouts();

      if (names.length === 0) showStatus("No keyboard layouts found");
      names.forEach((name, index) => {
        const current = index === current_idx;
        const label = names.indexOf(name) === names.lastIndexOf(name)
          ? name
          : `${name} (${index + 1})`;
        const row = makeRow(index === names.length - 1);
        const layout = new Gtk.Label({ label, xalign: 0, hexpand: true });
        layout.add_css_class("prefs-row-title");
        const select = new Gtk.Button({
          label: current ? "Active" : "Select",
          sensitive: !current,
          tooltip_text: current ? "Current keyboard layout" : `Switch to ${label}`,
        });
        select.add_css_class("prefs-btn");
        select.connect("clicked", () => void switchLayout(index));
        row.append(layout);
        row.append(select);
        group.append(row);
      });
    } catch (err) {
      if (request !== requestId) return;
      console.error("[keyboard] could not list layouts:", err);
      clearLayouts();
      showStatus("Could not load niri keyboard layouts");
    }
  };

  const switchLayout = async (index: number) => {
    if (switching) return;
    switching = true;
    group.sensitive = false;
    try {
      await execAsync(["niri", "msg", "action", "switch-layout", String(index)]);
    } catch (err) {
      alert("Could not switch keyboard layout", err);
    } finally {
      await refresh();
      group.sensitive = true;
      switching = false;
    }
  };

  const shortcuts = new Gtk.Button({
    label: "Show keyboard shortcuts",
    halign: Gtk.Align.START,
  });
  shortcuts.add_css_class("prefs-btn");
  shortcuts.connect("clicked", () => {
    execAsync(["niri", "msg", "action", "show-hotkey-overlay"])
      .catch((err) => alert("Could not show keyboard shortcuts", err));
  });
  page.append(shortcuts);

  page.connect("map", () => void refresh());
  return new Gtk.ScrolledWindow({ hscrollbar_policy: Gtk.PolicyType.NEVER, child: page });
}
