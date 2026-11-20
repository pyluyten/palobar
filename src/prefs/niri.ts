import { Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import GtkSource from "gi://GtkSource?version=5";

function loadConfigPreview(
  buffer: GtkSource.Buffer,
  languages: GtkSource.LanguageManager,
  onLoaded: (file: Gio.File) => void,
) {
  try {
    const proc = Gio.Subprocess.new(
      ["niri", "validate"],
      Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_MERGE,
    );
    proc.communicate_utf8_async(null, null, (_proc, result) => {
      try {
        const [, log] = proc.communicate_utf8_finish(result);
        const path = log?.match(/loaded config from "([^"\n]+)"/)?.[1]
          ?? GLib.getenv("NIRI_CONFIG")
          ?? GLib.build_filenamev([GLib.get_user_config_dir(), "niri", "config.kdl"]);
        const file = Gio.File.new_for_path(path);
        const [, bytes] = file.load_contents(null);
        const language = languages.guess_language(path, null);
        buffer.set_language(language);
        const status = proc.get_successful() ? "" : "Niri configuration validation failed.\n\n";
        buffer.set_text(status + new TextDecoder().decode(bytes), -1);
        onLoaded(file);
      } catch (err) {
        buffer.set_text("Could not read the niri configuration.", -1);
        console.error("[niri preferences] could not read niri config:", err);
      }
    });
  } catch (err) {
    buffer.set_text("Could not run niri validate.", -1);
    console.error("[niri preferences] could not validate niri config:", err);
  }
}

export function buildNiriPage(win: Gtk.Window): Gtk.Widget {
  const page = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 12 });
  page.add_css_class("prefs-page-body");

  const previewLabel = new Gtk.Label({ label: "Niri Configuration", halign: Gtk.Align.START });
  previewLabel.add_css_class("prefs-group-title");
  previewLabel.hexpand = true;
  const previewHeader = new Gtk.Box({ spacing: 8 });
  previewHeader.append(previewLabel);

  let configFile: Gio.File | null = null;
  const edit = new Gtk.Button({ label: "Edit", sensitive: false });
  edit.add_css_class("prefs-btn");
  edit.connect("clicked", () => {
    if (!configFile) return;
    try {
      const editor = Gio.AppInfo.get_default_for_type("text/plain", false);
      if (!editor) throw new Error("No default text editor is configured.");
      const context = win.get_display().get_app_launch_context();
      if (!editor.launch([configFile], context)) {
        throw new Error("The default text editor could not be launched.");
      }
    } catch (err) {
      console.error("[niri preferences] could not open niri config:", err);
      new Gtk.AlertDialog({
        message: "Could not open the niri configuration",
        detail: err instanceof Error ? err.message : String(err),
      }).show(win);
    }
  });
  previewHeader.append(edit);

  GtkSource.init();
  const languages = new GtkSource.LanguageManager();
  const bundledLanguages = GLib.getenv("PALOBAR_GTKSOURCE_LANGUAGE_DIR");
  if (bundledLanguages) languages.prepend_search_path(bundledLanguages);
  const buffer = new GtkSource.Buffer();
  const preview = new GtkSource.View({
    buffer,
    show_line_numbers: true,
    editable: false,
    cursor_visible: false,
    monospace: true,
    wrap_mode: Gtk.WrapMode.CHAR,
    left_margin: 8,
    right_margin: 8,
    top_margin: 8,
  });
  buffer.set_text("Loading configuration…", -1);
  page.append(previewHeader);
  page.append(new Gtk.ScrolledWindow({ child: preview, height_request: 232, vexpand: true }));

  page.connect("map", () => {
    configFile = null;
    edit.sensitive = false;
    loadConfigPreview(buffer, languages, (file) => {
      configFile = file;
      edit.sensitive = true;
      preview.tooltip_text = file.get_path();
    });
  });
  return page;
}
