import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { popups } from "../services/popups";

let runWindow: Gtk.Window | null = null;

export function closeRunCommand() {
  if (runWindow) {
    runWindow.close();
    runWindow.destroy();
    runWindow = null;
  }
}

function collectPathExecutables(): string[] {
  const pathEnv = GLib.getenv("PATH") || "";
  const dirs = pathEnv.split(":").filter((d) => d.length > 0);
  const seen = new Set<string>();

  for (const dirPath of dirs) {
    try {
      const dir = Gio.File.new_for_path(dirPath);
      if (!dir.query_exists(null)) continue;
      const enumerator = dir.enumerate_children(
        "standard::name,standard::type,access::can-execute",
        Gio.FileQueryInfoFlags.NONE,
        null,
      );
      let info: Gio.FileInfo | null;
      while ((info = enumerator.next_file(null)) !== null) {
        const name = info.get_name();
        if (!name || seen.has(name)) continue;
        if (info.get_attribute_boolean("access::can-execute")) {
          seen.add(name);
        }
      }
      enumerator.close(null);
    } catch {
      // ignore unreadable PATH entries
    }
  }

  return [...seen].sort((a, b) => a.localeCompare(b));
}

function executeCommand(cmd: string) {
  const trimmed = cmd.trim();
  if (!trimmed) return;
  try {
    Gio.Subprocess.new(["sh", "-c", trimmed], Gio.SubprocessFlags.NONE);
  } catch (err) {
    console.error("[run-command] failed to execute:", trimmed, err);
  }
}

export function toggleRunCommand(monitor?: Gdk.Monitor) {
  if (runWindow) {
    closeRunCommand();
    return;
  }

  popups.close();

  const targetMonitor = monitor ?? app.get_monitors()[0];
  if (!targetMonitor) return;

  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;
  const allCommands = collectPathExecutables();
  let matches: string[] = [];
  let selectedIndex = 0;

  const entry = new Gtk.Entry({
    placeholder_text: "Run a command...",
    primary_icon_name: "utilities-terminal-symbolic",
    halign: Gtk.Align.CENTER,
    width_request: 440,
  });
  entry.add_css_class("launcher-search");

  const listBox = new Gtk.ListBox({
    selection_mode: Gtk.SelectionMode.SINGLE,
    activate_on_single_click: true,
    visible: false,
  });
  listBox.add_css_class("run-completions");

  const updateSelection = () => {
    const row = listBox.get_row_at_index(selectedIndex);
    if (row) listBox.select_row(row);
  };

  const renderMatches = (rawText: string) => {
    let child = listBox.get_first_child();
    while (child) {
      const next = child.get_next_sibling();
      listBox.remove(child);
      child = next;
    }

    const token = rawText.trimStart().split(/\s+/)[0] ?? "";
    if (!token || rawText.includes(" ")) {
      matches = [];
      listBox.set_visible(false);
      return;
    }

    const lower = token.toLowerCase();
    const prefix: string[] = [];
    const substring: string[] = [];

    for (const cmd of allCommands) {
      const cLower = cmd.toLowerCase();
      if (cLower.startsWith(lower)) {
        prefix.push(cmd);
      } else if (cLower.includes(lower)) {
        substring.push(cmd);
      }
      if (prefix.length >= 8) break;
    }

    matches = [...prefix, ...substring].slice(0, 8);
    selectedIndex = 0;

    if (matches.length === 0) {
      listBox.set_visible(false);
      return;
    }

    for (const cmd of matches) {
      const lbl = new Gtk.Label({
        label: cmd,
        halign: Gtk.Align.START,
      });
      lbl.add_css_class("run-completion-label");
      const row = new Gtk.ListBoxRow({ child: lbl });
      row.add_css_class("run-completion-row");
      listBox.append(row);
    }

    updateSelection();
    listBox.set_visible(true);
  };

  listBox.connect("row-activated", (_box, row) => {
    const cmd = matches[row.get_index()];
    if (cmd) {
      closeRunCommand();
      executeCommand(cmd);
    }
  });

  entry.connect("changed", () => {
    renderMatches(entry.text);
  });

  entry.connect("activate", () => {
    const text = entry.text.trim();
    if (!text) return;
    closeRunCommand();
    executeCommand(text);
  });

  const entryKeyCtrl = new Gtk.EventControllerKey();
  entryKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
    if (keyval === Gdk.KEY_Tab && matches.length > 0) {
      const chosen = matches[selectedIndex] ?? matches[0];
      if (chosen) {
        entry.set_text(`${chosen} `);
        entry.set_position(-1);
      }
      return true;
    }
    if (keyval === Gdk.KEY_Down && matches.length > 0) {
      selectedIndex = (selectedIndex + 1) % matches.length;
      updateSelection();
      return true;
    }
    if (keyval === Gdk.KEY_Up && matches.length > 0) {
      selectedIndex = (selectedIndex - 1 + matches.length) % matches.length;
      updateSelection();
      return true;
    }
    return false;
  });
  entry.add_controller(entryKeyCtrl);

  const container = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 10,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.START,
    margin_top: 120,
    width_request: 440,
  });
  container.append(entry);
  container.append(listBox);

  const win = (
    <window
      visible
      name="run-command-dialog"
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
      closeRunCommand();
      return true;
    }
    return false;
  });
  win.add_controller(winKeyCtrl);

  const backdropClick = new Gtk.GestureClick();
  backdropClick.connect("released", (_gesture, _n, x, y) => {
    const picked = runWindow?.pick(x, y, Gtk.PickFlags.DEFAULT);
    if (picked === runWindow) {
      closeRunCommand();
    }
  });
  win.add_controller(backdropClick);

  runWindow = win;

  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    entry.grab_focus();
    return GLib.SOURCE_REMOVE;
  });
}
