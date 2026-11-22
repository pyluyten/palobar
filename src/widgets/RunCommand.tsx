import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { execAsync } from "ags/process";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { popups } from "../services/popups";
import { getAccentColor, subscribeAppearance } from "../services/appearance";
import { loadRunHistory, recordRun, type RunHistoryEntry } from "../services/run-history";

let runWindow: Gtk.Window | null = null;
let onCloseCallback: (() => void) | null = null;

export function closeRunCommand() {
  if (runWindow) {
    runWindow.close();
    runWindow.destroy();
    runWindow = null;
    const cb = onCloseCallback;
    onCloseCallback = null;
    cb?.();
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

function findTerminal(): { bin: string; flag: string | null } | null {
  if (GLib.find_program_in_path("xdg-terminal-exec")) {
    return { bin: "xdg-terminal-exec", flag: null };
  }
  const envTerm = GLib.getenv("TERMINAL");
  if (envTerm && GLib.find_program_in_path(envTerm)) {
    return { bin: envTerm, flag: envTerm === "gnome-terminal" ? "--" : "-e" };
  }
  const candidates = [
    "ghostty",
    "kitty",
    "alacritty",
    "foot",
    "wezterm",
    "ptyxis",
    "kgx",
    "gnome-terminal",
    "xterm",
  ];
  for (const term of candidates) {
    if (GLib.find_program_in_path(term)) {
      return { bin: term, flag: term === "gnome-terminal" ? "--" : "-e" };
    }
  }
  return null;
}

async function executeCommand(cmd: string, inTerminal = false) {
  const trimmed = cmd.trim();
  const window = runWindow;
  if (!trimmed || !window?.get_visible()) return;

  window.set_visible(false);

  let terminalLaunched = false;
  let argv: string[];
  if (inTerminal) {
    const term = findTerminal();
    if (term) {
      terminalLaunched = true;
      argv = term.flag
        ? [
            "niri",
            "msg",
            "action",
            "spawn",
            "--",
            term.bin,
            term.flag,
            "sh",
            "-c",
            trimmed,
          ]
        : [
            "niri",
            "msg",
            "action",
            "spawn",
            "--",
            term.bin,
            "sh",
            "-c",
            trimmed,
          ];
    } else {
      argv = ["niri", "msg", "action", "spawn-sh", "--", trimmed];
    }
  } else {
    argv = ["niri", "msg", "action", "spawn-sh", "--", trimmed];
  }

  try {
    await execAsync(argv);
    recordRun(trimmed, terminalLaunched);
  } catch (err) {
    console.error("[run-command] niri spawn failed:", err);
  } finally {
    if (runWindow === window) closeRunCommand();
  }
}

export function toggleRunCommand(
  monitor?: Gdk.Monitor,
  onClose?: () => void,
) {
  if (runWindow) {
    closeRunCommand();
    return;
  }

  popups.close();

  const targetMonitor = monitor ?? app.get_monitors()[0];
  if (!targetMonitor) return;

  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;
  const allCommands = collectPathExecutables();
  const history = loadRunHistory();
  let draft = "";
  let historyMatches: RunHistoryEntry[] = [];
  let historyIndex = -1;
  let recallingHistory = false;
  let matches: string[] = [];
  let selectedIndex = 0;

  const entry = new Gtk.Entry({
    placeholder_text: "Run a command... (Ctrl+Enter for terminal)",
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
      executeCommand(cmd);
    }
  });

  const showTerminalMode = (inTerminal: boolean) => {
    if (inTerminal) entry.add_css_class("run-command-terminal");
    else entry.remove_css_class("run-command-terminal");
    entry.set_icon_tooltip_text(Gtk.EntryIconPosition.PRIMARY, inTerminal ? "Run in Terminal" : null);
  };

  entry.connect("changed", () => {
    if (recallingHistory) return;
    draft = entry.text;
    historyIndex = -1;
    historyMatches = [];
    showTerminalMode(false);
    renderMatches(draft);
  });

  const navigateHistory = (direction: -1 | 1): boolean => {
    if (historyIndex === -1) {
      if (direction === -1) return false;
      historyMatches = history.filter(({ command }) => command.startsWith(draft.trimStart()));
      if (!historyMatches.length) return false;
    }

    historyIndex = Math.min(historyMatches.length - 1, historyIndex + direction);
    const recalled = historyMatches[historyIndex];
    recallingHistory = true;
    try {
      entry.set_text(recalled?.command ?? draft);
      entry.set_position(-1);
    } finally {
      recallingHistory = false;
    }
    showTerminalMode(recalled?.inTerminal ?? false);
    if (recalled) {
      matches = [];
      listBox.set_visible(false);
    } else {
      renderMatches(draft);
    }
    return true;
  };

  entry.connect("activate", () => {
    executeCommand(entry.text, historyMatches[historyIndex]?.inTerminal ?? false);
  });

  const entryKeyCtrl = new Gtk.EventControllerKey();
  entryKeyCtrl.connect("key-pressed", (_ctrl, keyval, _keycode, state) => {
    if (
      (keyval === Gdk.KEY_Return || keyval === Gdk.KEY_KP_Enter) &&
      (state & Gdk.ModifierType.CONTROL_MASK) !== 0
    ) {
      const text = entry.text.trim();
      if (!text) return true;
      executeCommand(text, true);
      return true;
    }
    if (keyval === Gdk.KEY_Tab && matches.length > 0) {
      const chosen = matches[selectedIndex] ?? matches[0];
      if (chosen) {
        entry.set_text(`${chosen} `);
        entry.set_position(-1);
      }
      return true;
    }
    if (keyval === Gdk.KEY_Up && navigateHistory(1)) return true;
    if (keyval === Gdk.KEY_Down && navigateHistory(-1)) return true;
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

  const display = targetMonitor.get_display();
  const accentProvider = new Gtk.CssProvider();
  Gtk.StyleContext.add_provider_for_display(display, accentProvider, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1);
  const unsubscribeAccent = subscribeAppearance("accent-color", () => {
    accentProvider.load_from_string(`
      entry.run-command-terminal image { color: ${getAccentColor().hex}; }
    `);
  });
  onCloseCallback = () => {
    unsubscribeAccent();
    Gtk.StyleContext.remove_provider_for_display(display, accentProvider);
    onClose?.();
  };
  runWindow = win;

  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    entry.grab_focus();
    return GLib.SOURCE_REMOVE;
  });
}
