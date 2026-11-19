import { Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import type { PalobarConfig } from "../services/config";

export interface PrefsContext {
  getConfig: () => PalobarConfig;
  updateConfig: (next: PalobarConfig) => void;
  updateConfigDebounced: (next: PalobarConfig) => void;
}

export const PREFS_CSS = `
window.prefs-window {
  background-color: #fafafa;
  color: #2e2e32;
  font-family: "Cantarell", "Inter", sans-serif;
  font-size: 13.5px;
}

.prefs-sidebar {
  background-color: #f2f2f4;
  border-right: 1px solid rgba(0, 0, 0, 0.08);
  min-width: 210px;
  padding: 14px 10px;
}

.prefs-sidebar-title {
  font-size: 16px;
  font-weight: 700;
  color: #2e2e32;
  padding: 4px 10px 12px 10px;
}

.prefs-nav-btn {
  background: transparent;
  background-image: none;
  box-shadow: none;
  border: none;
  border-radius: 10px;
  padding: 9px 12px;
  color: rgba(46, 46, 50, 0.85);
  font-weight: 500;
}

.prefs-nav-btn:hover {
  background-color: rgba(0, 0, 0, 0.05);
}

.prefs-nav-btn.active {
  background-color: rgba(0, 0, 0, 0.09);
  color: #1c1c20;
  font-weight: 700;
}

.prefs-content-pane {
  background-color: #fafafa;
}

.prefs-page-header {
  font-size: 18px;
  font-weight: 700;
  color: #2e2e32;
  padding: 18px 24px 8px 24px;
}

.prefs-page-body {
  padding: 8px 24px 16px 24px;
}

.prefs-group-title {
  font-size: 13.5px;
  font-weight: 700;
  color: #2e2e32;
  margin-bottom: 6px;
}

.prefs-group {
  background-color: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
}

.prefs-row {
  padding: 12px 16px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}

.prefs-row.last {
  border-bottom: none;
}

.prefs-row-title {
  font-weight: 500;
  color: #2e2e32;
}

.prefs-muted {
  font-size: 12px;
  color: rgba(46, 46, 50, 0.62);
}

.prefs-btn {
  background-color: #e8e8ec;
  background-image: none;
  box-shadow: none;
  border: none;
  border-radius: 9px;
  padding: 7px 14px;
  color: #2e2e32;
}

.prefs-btn:hover {
  background-color: #dcdce2;
}

.prefs-btn.primary {
  background-color: #3584e4;
  color: #ffffff;
  font-weight: 600;
}

.prefs-btn.primary:hover {
  background-color: #2a76d4;
}

.prefs-entry {
  background-color: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.16);
  border-radius: 8px;
  padding: 6px 10px;
  color: #2e2e32;
}

.prefs-footer {
  border-top: 1px solid rgba(0, 0, 0, 0.08);
  padding: 6px 24px;
  background-color: #f2f2f4;
}
`;

export function pickImage(
  parent: Gtk.Window,
  onSelected: (path: string) => void,
) {
  const filter = new Gtk.FileFilter();
  filter.set_name("Images (PNG, JPEG, WebP)");
  filter.add_mime_type("image/png");
  filter.add_mime_type("image/jpeg");
  filter.add_mime_type("image/webp");

  const filters = new Gio.ListStore({ item_type: Gtk.FileFilter.$gtype });
  filters.append(filter);

  const dialog = new Gtk.FileDialog({
    title: "Select Image",
    filters,
    default_filter: filter,
  });

  dialog.open(parent, null, (_dlg, res) => {
    try {
      const file = dialog.open_finish(res);
      const path = file?.get_path();
      if (path) onSelected(path);
    } catch {
      // cancelled
    }
  });
}

export function makeGroupBox(): Gtk.Box {
  const box = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL });
  box.add_css_class("prefs-group");
  return box;
}

export function makeRow(isLast = false): Gtk.Box {
  const row = new Gtk.Box({
    spacing: 12,
    valign: Gtk.Align.CENTER,
  });
  row.add_css_class("prefs-row");
  if (isLast) row.add_css_class("last");
  return row;
}
