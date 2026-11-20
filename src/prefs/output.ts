import { Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";
import { makeGroupBox, makeRow } from "./ui";

interface Mode {
  width: number;
  height: number;
  refresh_rate: number;
}

interface Output {
  name: string;
  model: string;
  modes: Mode[];
  current_mode: number | null;
  is_custom_mode: boolean;
  logical: { width: number; height: number; scale: number } | null;
}

type Setting = "mode" | "scale";

const CONFIRM_SECONDS = 15;
const modeArgument = (mode: Mode) =>
  `${mode.width}x${mode.height}@${(mode.refresh_rate / 1000).toFixed(3)}`;
const currentMode = (output: Output) =>
  output.current_mode === null ? undefined : output.modes[output.current_mode];
const percent = (scale: number) => `${Number((scale * 100).toFixed(2))}%`;

async function getOutputs(): Promise<Record<string, Output>> {
  return JSON.parse(await execAsync(["niri", "msg", "-j", "outputs"]));
}

function monitorDrawing(output: Output): Gtk.DrawingArea {
  const mode = currentMode(output);
  const ratio = output.logical
    ? output.logical.width / output.logical.height
    : mode ? mode.width / mode.height : 16 / 9;
  const drawing = new Gtk.DrawingArea({ content_width: 112, content_height: 76 });
  drawing.set_draw_func((_widget, cr, width, height) => {
    const screenWidth = Math.min(width - 8, (height - 20) * ratio);
    const screenHeight = screenWidth / ratio;
    const x = (width - screenWidth) / 2;
    const y = (height - screenHeight - 14) / 2;

    cr.setSourceRGBA(0.31, 0.56, 0.86, 0.20);
    cr.rectangle(x, y, screenWidth, screenHeight);
    cr.fillPreserve();
    cr.setSourceRGBA(0.31, 0.56, 0.86, 0.95);
    cr.setLineWidth(2);
    cr.stroke();

    cr.moveTo(width / 2, y + screenHeight);
    cr.lineTo(width / 2, y + screenHeight + 10);
    cr.moveTo(width / 2 - 13, y + screenHeight + 10);
    cr.lineTo(width / 2 + 13, y + screenHeight + 10);
    cr.stroke();
  });
  return drawing;
}

// The subprocess owns the deadline and reverts on EOF as well as timeout.
// A closed or stalled preferences window cannot leave the preview applied.
const previewCommand = `
  change() { niri msg output "$1" "$2" "$3"; }

  if ! change "$1" "$2" "$3" >&2; then
    change "$1" "$2" "$4" >&2
    exit 3
  fi

  printf 'ready\\n'
  if IFS= read -r -t "$5" answer && [ "$answer" = keep ]; then
    exit 0
  fi

  change "$1" "$2" "$4" >&2 || exit 4
  exit 2
`;

export function buildOutputPage(win: Gtk.Window): Gtk.Widget {
  const page = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 12 });
  page.add_css_class("prefs-page-body");

  const title = new Gtk.Label({ label: "Temporary settings", xalign: 0 });
  title.add_css_class("prefs-group-title");
  page.append(title);

  const displays = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 12 });
  page.append(displays);
  let busy = false;
  let refreshId = 0;

  const showError = (detail: string) => {
    new Gtk.AlertDialog({ message: "Could not change display settings", detail }).show(win);
  };

  const refresh = async () => {
    const request = ++refreshId;
    try {
      const outputs = Object.values(await getOutputs());
      if (request !== refreshId || busy) return;
      for (let child = displays.get_first_child(); child; child = displays.get_first_child()) {
        displays.remove(child);
      }
      if (outputs.length === 0) {
        displays.append(new Gtk.Label({ label: "No connected displays", xalign: 0 }));
      }
      for (const output of outputs) displays.append(buildDisplay(output));
    } catch (err) {
      console.error("[output] could not list displays:", err);
      if (request === refreshId && !displays.get_first_child()) {
        displays.append(new Gtk.Label({ label: "Could not list niri displays", xalign: 0 }));
      }
    }
  };

  const apply = async (name: string, setting: Setting, value: string) => {
    if (busy) return;
    busy = true;
    displays.sensitive = false;
    const finish = () => {
      busy = false;
      displays.sensitive = true;
      void refresh();
    };

    try {
      const output = (await getOutputs())[name];
      const mode = output && currentMode(output);
      if (!output?.logical || !mode) throw new Error("The display is no longer active.");
      if (setting === "mode" && (output.is_custom_mode ||
        !output.modes.some((candidate) => modeArgument(candidate) === value))) {
        throw new Error("This display mode is no longer available for testing.");
      }

      const previous = setting === "mode" ? modeArgument(mode) : String(output.logical.scale);
      if (value === previous) {
        finish();
        return;
      }

      const proc = Gio.Subprocess.new(
        ["bash", "-c", previewCommand, "palobar-output-preview",
          name, setting, value, previous, String(CONFIRM_SECONDS)],
        Gio.SubprocessFlags.STDIN_PIPE | Gio.SubprocessFlags.STDOUT_PIPE,
      );
      const stdin = proc.get_stdin_pipe()!;
      const stdout = new Gio.DataInputStream({ base_stream: proc.get_stdout_pipe()! });
      let dialog: Gtk.MessageDialog | null = null;
      let countdown = 0;
      let settled = false;

      const dismiss = () => {
        if (countdown) GLib.source_remove(countdown);
        countdown = 0;
        dialog?.destroy();
        dialog = null;
      };

      stdout.read_line_async(GLib.PRIORITY_DEFAULT, null, (_stream, result) => {
        try {
          const [line] = stdout.read_line_finish_utf8(result);
          if (line !== "ready" || settled) return;
          const deadline = GLib.get_monotonic_time() + CONFIRM_SECONDS * GLib.USEC_PER_SEC;
          const message = (seconds: number) =>
            `Previous settings will be restored in ${seconds} seconds.`;
          dialog = new Gtk.MessageDialog({
            transient_for: win,
            modal: true,
            destroy_with_parent: true,
            text: "Keep these display settings?",
            secondary_text: message(CONFIRM_SECONDS),
          });
          dialog.add_button("Revert", Gtk.ResponseType.CANCEL);
          dialog.add_button("Keep Changes", Gtk.ResponseType.ACCEPT);
          dialog.set_default_response(Gtk.ResponseType.CANCEL);
          dialog.connect("response", (_dialog, response) => {
            try {
              if (response === Gtk.ResponseType.ACCEPT) {
                stdin.write_all(new TextEncoder().encode("keep\n"), null);
              }
              stdin.close(null);
            } catch (err) {
              console.error("[output] confirmation expired:", err);
            }
            dismiss();
          });
          countdown = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, () => {
            const remaining = Math.max(0,
              Math.ceil((deadline - GLib.get_monotonic_time()) / GLib.USEC_PER_SEC));
            dialog?.set_property("secondary-text", message(remaining));
            return GLib.SOURCE_CONTINUE;
          });
          dialog.present();
        } catch (err) {
          console.error("[output] could not show display confirmation:", err);
          stdin.close(null);
        }
      });

      proc.wait_async(null, (_proc, result) => {
        settled = true;
        dismiss();
        try {
          proc.wait_finish(result);
          const status = proc.get_if_exited() ? proc.get_exit_status() : -1;
          if (status === 4) showError("Niri could not restore the previous display setting.");
          else if (status !== 0 && status !== 2) showError("Niri rejected the requested display setting.");
        } catch (err) {
          showError(String(err));
        } finally {
          finish();
        }
      });
    } catch (err) {
      showError(String(err));
      finish();
    }
  };

  function buildDisplay(output: Output) {
    const box = makeGroupBox();
    const summary = makeRow(false);
    const mode = currentMode(output);
    const scale = output.logical?.scale;
    const details = new Gtk.Box({
      orientation: Gtk.Orientation.VERTICAL,
      spacing: 4,
      hexpand: true,
    });
    const name = new Gtk.Label({
      label: output.model ? `${output.model} (${output.name})` : output.name,
      xalign: 0,
      wrap: true,
    });
    name.add_css_class("prefs-row-title");
    details.append(name);
    const status = new Gtk.Label({
      label: mode && scale
        ? `${mode.width} × ${mode.height} · ${percent(scale)}`
        : "Display off",
      xalign: 0,
    });
    status.add_css_class("prefs-muted");
    details.append(status);
    summary.append(monitorDrawing(output));
    summary.append(details);
    box.append(summary);
    if (!mode || !scale) return box;

    const appendSetting = (
      label: string,
      setting: Setting,
      choices: Array<{ label: string; value: string }>,
      selected: string,
      last: boolean,
    ) => {
      const row = makeRow(last);
      const title = new Gtk.Label({ label, xalign: 0, hexpand: true });
      title.add_css_class("prefs-row-title");
      const dropdown = Gtk.DropDown.new_from_strings(choices.map((choice) => choice.label));
      dropdown.selected = choices.findIndex((choice) => choice.value === selected);
      const applyButton = new Gtk.Button({ label: "Apply", sensitive: false });
      applyButton.add_css_class("prefs-btn");
      dropdown.connect("notify::selected", () => {
        applyButton.sensitive = choices[dropdown.selected]?.value !== selected;
      });
      applyButton.connect("clicked", () => {
        const value = choices[dropdown.selected]?.value;
        if (value) void apply(output.name, setting, value);
      });
      row.append(title);
      row.append(dropdown);
      row.append(applyButton);
      box.append(row);
    };

    if (!output.is_custom_mode) {
      appendSetting("Resolution", "mode", output.modes.map((option) => ({
        label: `${option.width} × ${option.height} · ${(option.refresh_rate / 1000).toFixed(3)} Hz`,
        value: modeArgument(option),
      })), modeArgument(mode), false);
    }
    const scales = [...new Set([1, 1.25, 1.5, 1.75, 2, 2.5, 3, scale])].sort((a, b) => a - b);
    appendSetting("Scale", "scale", scales.map((option) => ({
      label: percent(option),
      value: String(option),
    })), String(scale), true);
    return box;
  }

  page.connect("map", () => {
    void refresh();
  });

  return new Gtk.ScrolledWindow({ hscrollbar_policy: Gtk.PolicyType.NEVER, child: page });
}
