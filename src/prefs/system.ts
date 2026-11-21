import { Gdk, Gtk } from "ags/gtk4";
import { execAsync } from "ags/process";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { makeGroupBox, makeRow } from "./ui";

type Field = [label: string, value: string | null];

function readText(path: string): string | null {
  try {
    const [, bytes] = GLib.file_get_contents(path);
    return new TextDecoder().decode(bytes).trim() || null;
  } catch {
    return null;
  }
}

function dmi(name: string): string | null {
  const value = readText(`/sys/class/dmi/id/${name}`);
  return value && !/^(to be filled by o\.e\.m\.|default string|system (manufacturer|product name)|not specified)$/i.test(value)
    ? value
    : null;
}

function hardwareModel(): string | null {
  const model = dmi("product_name");
  return [...new Set(model
    ? [dmi("sys_vendor"), model].filter(Boolean)
    : [dmi("board_vendor"), dmi("board_name")].filter(Boolean))].join(" ") || null;
}

function processor(): string | null {
  const cpuInfo = readText("/proc/cpuinfo");
  const model = cpuInfo?.match(/^model name\s*:\s*(.+)$/m)?.[1]
    ?? cpuInfo?.match(/^Hardware\s*:\s*(.+)$/m)?.[1];
  const count = GLib.get_num_processors();
  return model ? `${model.trim()} × ${count}` : `${count} logical processors`;
}

function systemStorage(): string | null {
  try {
    const info = Gio.File.new_for_path("/").query_filesystem_info("filesystem::size", null);
    const bytes = info.get_attribute_uint64("filesystem::size");
    return bytes > 0 ? GLib.format_size(bytes) : null;
  } catch {
    return null;
  }
}

async function commandOutput(args: string[]): Promise<string | null> {
  try {
    return (await execAsync(args)).trim() || null;
  } catch {
    return null;
  }
}

async function niriVersion(): Promise<string | null> {
  const response = await commandOutput(["niri", "msg", "-j", "version"]);
  if (response) {
    try {
      const { compositor } = JSON.parse(response);
      if (typeof compositor === "string" && compositor) return compositor;
    } catch {
      // Older releases may return plain text.
    }
  }
  const version = await commandOutput(["niri", "--version"]);
  return version?.replace(/^niri\s+/i, "") ?? null;
}

async function graphics(): Promise<string | null> {
  const listing = await commandOutput(["lspci", "-mm"]);
  const names = listing?.split("\n").flatMap((line) => {
    const match = line.match(/^\S+ "(?:VGA compatible controller|3D controller|Display controller)" "([^"]+)" "([^"]+)"/);
    return match ? [`${match[1]} ${match[2]}`] : [];
  });
  if (names?.length) return [...new Set(names)].join("\n");

  // PCI IDs remain available on systems where pciutils is absent.
  const cards = Gio.File.new_for_path("/sys/class/drm");
  try {
    const entries = cards.enumerate_children("standard::name", Gio.FileQueryInfoFlags.NONE, null);
    const ids = new Set<string>();
    try {
      for (let entry = entries.next_file(null); entry; entry = entries.next_file(null)) {
        const card = entry.get_name();
        if (!/^card\d+$/.test(card)) continue;
        const device = `/sys/class/drm/${card}/device`;
        const vendor = readText(`${device}/vendor`);
        const id = readText(`${device}/device`);
        if (vendor && id) ids.add(`PCI ${vendor.slice(2)}:${id.slice(2)}`);
      }
    } finally {
      entries.close(null);
    }
    return [...ids].join("\n") || null;
  } catch {
    return null;
  }
}

function fieldRow([name, value]: Field, last: boolean): Gtk.Box {
  const row = makeRow(last);
  const content = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 2, hexpand: true });
  const label = new Gtk.Label({ label: name, xalign: 0 });
  label.add_css_class("prefs-muted");
  const text = new Gtk.Label({ label: value ?? "Unavailable", xalign: 0, wrap: true, selectable: true });
  text.add_css_class("prefs-row-title");
  content.append(label);
  content.append(text);
  row.append(content);
  return row;
}

function detailsColumn(title: string, fields: Field[]): Gtk.Box {
  const column = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 6 });
  const heading = new Gtk.Label({ label: title, xalign: 0 });
  heading.add_css_class("prefs-group-title");
  column.append(heading);
  const group = makeGroupBox();
  fields.forEach((field, index) => group.append(fieldRow(field, index === fields.length - 1)));
  column.append(group);
  return column;
}

function presentDetails(parent: Gtk.Window, hardware: Field[], software: Field[]) {
  const dialog = new Gtk.Window({
    title: "System Information",
    transient_for: parent,
    destroy_with_parent: true,
    modal: true,
    default_width: 660,
    default_height: 460,
  });
  dialog.add_css_class("prefs-window");
  if (parent.has_css_class("dark")) dialog.add_css_class("dark");

  const header = new Gtk.HeaderBar();
  const copy = new Gtk.Button({ label: "Copy" });
  copy.add_css_class("prefs-btn");
  copy.connect("clicked", () => {
    const lines = (title: string, fields: Field[]) =>
      [title, ...fields.map(([label, value]) => `${label}: ${value ?? "Unavailable"}`)].join("\n");
    dialog.get_clipboard().set([
      lines("Hardware", hardware),
      lines("Software", software),
    ].join("\n\n"));
  });
  header.pack_start(copy);
  dialog.set_titlebar(header);

  const columns = new Gtk.Box({ spacing: 16, homogeneous: true });
  columns.add_css_class("prefs-page-body");
  columns.append(detailsColumn("Hardware", hardware));
  columns.append(detailsColumn("Software", software));
  dialog.set_child(new Gtk.ScrolledWindow({ child: columns, hscrollbar_policy: Gtk.PolicyType.NEVER }));
  dialog.present();
}

export function buildSystemPage(win: Gtk.Window): Gtk.Widget {
  const page = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 16 });
  page.add_css_class("prefs-page-body");

  page.connect("map", () => {
    if (page.get_first_child()) return;

    const os = GLib.get_os_info("PRETTY_NAME") ?? GLib.get_os_info("NAME");
    const logo = GLib.get_os_info("LOGO");
    const theme = Gtk.IconTheme.get_for_display(win.get_display());
    page.append(new Gtk.Image({
      icon_name: logo && theme.has_icon(logo) ? logo : "computer-symbolic",
      pixel_size: 76,
      halign: Gtk.Align.CENTER,
      margin_top: 4,
      margin_bottom: 4,
    }));

    const memoryKiB = Number(readText("/proc/meminfo")?.match(/^MemTotal:\s+(\d+) kB$/m)?.[1]);
    const hardware: Field[] = [
      ["Hardware Model", hardwareModel()],
      ["Processor", processor()],
      ["Memory", memoryKiB > 0 ? GLib.format_size_full(memoryKiB * 1024, GLib.FormatSizeFlags.IEC_UNITS) : null],
      ["System Storage", systemStorage()],
    ];
    const hostname = GLib.get_host_name();
    const device = makeGroupBox();
    device.append(fieldRow(["Device Name", hostname], true));
    page.append(device);

    const summary = makeGroupBox();
    const headline: Field[] = [["Operating System", os], ...hardware];
    headline.forEach((field) => summary.append(fieldRow(field, false)));

    const details = new Gtk.Button({ label: "System Information" });
    details.add_css_class("prefs-btn");
    details.connect("clicked", async () => {
      details.set_sensitive(false);
      try {
        const [displayAdapter, niri, architecture] = await Promise.all([
          graphics(), niriVersion(), commandOutput(["uname", "-m"]),
        ]);
        if (!win.get_visible()) return;
        const displayName = win.get_display().get_name();
        const windowing = displayName.startsWith("wayland") ? "Wayland"
          : displayName.startsWith(":") ? "X11" : GLib.getenv("XDG_SESSION_TYPE");
        const software: Field[] = [
          ["Operating System", os],
          ["OS Build", GLib.get_os_info("BUILD_ID")],
          ["Architecture", architecture],
          ["Niri Version", niri],
          ["Palobar Version", readText(GLib.getenv("PALOBAR_VERSION_FILE") ?? "VERSION")],
          ["Windowing System", windowing],
          ["Kernel Version", readText("/proc/sys/kernel/osrelease")],
        ];
        presentDetails(win, [
          ["Device Name", hostname], ...hardware, ["Graphics", displayAdapter],
          ["Firmware Version", dmi("bios_version")],
        ], software);
      } finally {
        details.set_sensitive(true);
      }
    });
    const actionRow = makeRow(true);
    actionRow.append(details);
    summary.append(actionRow);
    page.append(summary);
  });

  return new Gtk.ScrolledWindow({ child: page, hscrollbar_policy: Gtk.PolicyType.NEVER });
}
