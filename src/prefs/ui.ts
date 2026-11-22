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

.prefs-btn:disabled {
  opacity: 0.5;
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

/* Appearance: Style cards */
.prefs-style-card {
  background: transparent;
  background-image: none;
  box-shadow: none;
  border: 2px solid transparent;
  border-radius: 14px;
  padding: 6px;
}

.prefs-style-card:hover .prefs-style-preview {
  border-color: rgba(0, 0, 0, 0.25);
}

.prefs-style-card.selected {
  border-color: #3584e4;
}

.prefs-style-preview {
  border-radius: 10px;
  border: 1px solid rgba(0, 0, 0, 0.14);
  min-width: 152px;
  min-height: 92px;
  padding: 12px;
  background: linear-gradient(135deg, #1c71d8, #1a5fb4);
}

.prefs-mockup-back {
  background-color: #242428;
  border-radius: 7px;
  min-width: 78px;
  min-height: 52px;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
}

.prefs-mockup-front.light {
  background-color: #ffffff;
  border-radius: 7px;
  min-width: 78px;
  min-height: 52px;
  box-shadow: 0 3px 10px rgba(0, 0, 0, 0.35);
}

.prefs-mockup-front.dark {
  background-color: #2c2c32;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 7px;
  min-width: 78px;
  min-height: 52px;
  box-shadow: 0 3px 10px rgba(0, 0, 0, 0.45);
}

.prefs-style-label {
  font-size: 13px;
  font-weight: 500;
  margin-top: 6px;
}

/* Accent Color circles */
.prefs-accent-btn {
  background: transparent;
  background-image: none;
  box-shadow: none;
  border: 2px solid transparent;
  border-radius: 999px;
  padding: 3px;
  min-width: 0;
  min-height: 0;
}

.prefs-accent-dot {
  min-width: 24px;
  min-height: 24px;
  border-radius: 999px;
}

/* Wallpaper Preview */
.prefs-wp-preview {
  border-radius: 10px;
  border: 1px solid rgba(0, 0, 0, 0.12);
  min-width: 220px;
  min-height: 124px;
}

/* Dark theme for prefs window */
window.prefs-window.dark {
  background-color: #1c1c20;
  color: #f6f5f4;
}

window.prefs-window.dark .prefs-sidebar {
  background-color: #242429;
  border-right-color: rgba(255, 255, 255, 0.08);
}

window.prefs-window.dark .prefs-sidebar-title,
window.prefs-window.dark .prefs-page-header,
window.prefs-window.dark .prefs-group-title,
window.prefs-window.dark .prefs-row-title {
  color: #f6f5f4;
}

window.prefs-window.dark .prefs-nav-btn {
  color: rgba(246, 245, 244, 0.85);
}

window.prefs-window.dark .prefs-nav-btn:hover {
  background-color: rgba(255, 255, 255, 0.07);
}

window.prefs-window.dark .prefs-nav-btn.active {
  background-color: rgba(255, 255, 255, 0.13);
  color: #ffffff;
}

window.prefs-window.dark .prefs-content-pane {
  background-color: #1c1c20;
}

window.prefs-window.dark .prefs-group {
  background-color: #28282e;
  border-color: rgba(255, 255, 255, 0.08);
}

window.prefs-window.dark .prefs-row {
  border-bottom-color: rgba(255, 255, 255, 0.06);
}

window.prefs-window.dark .prefs-muted {
  color: rgba(246, 245, 244, 0.62);
}

window.prefs-window.dark .prefs-btn {
  background-color: rgba(255, 255, 255, 0.1);
  color: #f6f5f4;
}

window.prefs-window.dark .prefs-btn:hover {
  background-color: rgba(255, 255, 255, 0.16);
}

window.prefs-window.dark .prefs-entry {
  background-color: rgba(0, 0, 0, 0.25);
  border-color: rgba(255, 255, 255, 0.12);
  color: #f6f5f4;
}

window.prefs-window.dark .prefs-footer {
  background-color: #202025;
  border-top-color: rgba(255, 255, 255, 0.08);
}

window.prefs-window.dark .prefs-wp-preview {
  border-color: rgba(255, 255, 255, 0.12);
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
