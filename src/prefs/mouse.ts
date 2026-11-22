import { Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { makeGroupBox, makeRow } from "./ui";
import { readCursor, renderCursor } from "./cursor-preview";

function cursorRoots(): string[] {
  const home = GLib.get_home_dir();
  return [...new Set([
    ...(GLib.getenv("XCURSOR_PATH") ?? "").split(":").filter(Boolean)
      .map((path) => path.startsWith("~/") ? `${home}/${path.slice(2)}` : path),
    `${home}/.icons`,
    ...[GLib.get_user_data_dir(), ...GLib.get_system_data_dirs()].map((path) => `${path}/icons`),
  ])];
}

function installedCursorThemes(roots: string[]): string[] {
  const themes = new Set<string>();
  for (const root of roots) {
    const dir = Gio.File.new_for_path(root);
    try {
      const entries = dir.enumerate_children("standard::name", Gio.FileQueryInfoFlags.NONE, null);
      try {
        for (let entry = entries.next_file(null); entry; entry = entries.next_file(null)) {
          const name = entry.get_name();
          if (GLib.file_test(`${root}/${name}/cursors`, GLib.FileTest.IS_DIR)) themes.add(name);
        }
      } finally {
        entries.close(null);
      }
    } catch {
      // Search paths can contain missing or unreadable directories.
    }
  }
  return [...themes].sort((a, b) => a.localeCompare(b));
}

function cursorRow(title: string, variable: string, last = false): Gtk.Box {
  const row = makeRow(last);
  const labels = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 2, hexpand: true });
  const label = new Gtk.Label({ label: title, xalign: 0 });
  label.add_css_class("prefs-row-title");
  const hint = new Gtk.Label({ label: variable, xalign: 0 });
  hint.add_css_class("prefs-muted");
  labels.append(label);
  labels.append(hint);
  row.append(labels);
  return row;
}

export function buildMousePage(): Gtk.Widget {
  const page = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 16 });
  page.add_css_class("prefs-page-body");

  const group = makeGroupBox();
  const theme = new Gtk.DropDown({ enable_search: true, width_request: 180 });
  const themeRow = cursorRow("Cursor Theme", "XCURSOR_THEME");
  themeRow.append(theme);
  group.append(themeRow);

  const size = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 8, 96, 1);
  size.set_draw_value(true);
  size.set_digits(0);
  size.set_format_value_func((_scale, value) => `${value} px`);
  size.set_size_request(180, -1);
  const sizeRow = cursorRow("Cursor Size", "XCURSOR_SIZE", true);
  sizeRow.append(size);
  group.append(sizeRow);
  page.append(group);

  const preview = makeGroupBox();
  const previewRow = makeRow(true);
  previewRow.set_size_request(-1, 108);
  const previewImage = new Gtk.Picture({ can_shrink: false });
  previewRow.append(previewImage);
  const previewHint = new Gtk.Label({
    label: "Move the pointer here to preview the cursor.",
    hexpand: true,
    wrap: true,
  });
  previewHint.add_css_class("prefs-muted");
  previewRow.append(previewHint);
  preview.append(previewRow);
  page.append(preview);

  const instructions = new Gtk.Label({
    label: "Add or update this cursor block in your Niri configuration. Niri uses it to set XCURSOR_THEME and XCURSOR_SIZE.",
    xalign: 0,
    wrap: true,
  });
  instructions.add_css_class("prefs-muted");
  page.append(instructions);

  const example = new Gtk.Label({ xalign: 0, selectable: true, wrap: true });
  example.add_css_class("monospace");
  const exampleGroup = makeGroupBox();
  const exampleRow = makeRow(true);
  exampleRow.append(example);
  exampleGroup.append(exampleRow);
  page.append(exampleGroup);

  let roots: string[] = [];
  let loadedTheme: string | null = null;
  let cursorBytes: Uint8Array | null = null;

  const updateCursor = () => {
    const selected = theme.get_selected_item();
    if (!(selected instanceof Gtk.StringObject)) {
      example.set_label("");
      return;
    }
    const cursorTheme = selected.get_string();
    const cursorSize = Math.round(size.get_value());
    example.set_label([
      "cursor {",
      `    xcursor-theme ${JSON.stringify(cursorTheme)}`,
      `    xcursor-size ${cursorSize}`,
      "}",
    ].join("\n"));
    if (!roots.length) return;
    if (cursorTheme !== loadedTheme) {
      loadedTheme = cursorTheme;
      cursorBytes = readCursor(roots, cursorTheme);
    }
    const cursor = cursorBytes ? renderCursor(cursorBytes, cursorSize) : null;
    previewImage.set_paintable(cursor?.get_texture() ?? null);
    previewRow.set_cursor(cursor);
    previewHint.set_label(cursor
      ? "Move the pointer here to preview the cursor."
      : "No pointer preview is available for this theme.");
  };

  theme.connect("notify::selected-item", updateCursor);
  size.connect("value-changed", updateCursor);
  page.connect("map", () => {
    const settings = Gtk.Settings.get_default();
    const currentTheme = GLib.getenv("XCURSOR_THEME") || settings?.gtk_cursor_theme_name || "default";
    const searchPaths = cursorRoots();
    const themes = installedCursorThemes(searchPaths);
    if (!themes.includes(currentTheme)) themes.unshift(currentTheme);
    theme.set_model(Gtk.StringList.new(themes));
    theme.set_selected(themes.indexOf(currentTheme));

    const configuredSize = Number(GLib.getenv("XCURSOR_SIZE")) || settings?.gtk_cursor_theme_size || 24;
    const initialSize = Number.isSafeInteger(configuredSize) && configuredSize > 0 ? configuredSize : 24;
    size.set_range(Math.min(8, initialSize), Math.max(96, initialSize));
    size.set_value(initialSize);
    roots = searchPaths;
    updateCursor();
  });
  page.connect("unmap", () => {
    roots = [];
    loadedTheme = null;
    cursorBytes = null;
    previewImage.set_paintable(null);
    previewRow.set_cursor(null);
    theme.set_model(null);
    size.set_value(24);
  });

  return new Gtk.ScrolledWindow({ child: page, hscrollbar_policy: Gtk.PolicyType.NEVER });
}
