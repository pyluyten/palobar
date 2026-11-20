import { Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";
import { makeGroupBox, makeRow } from "./ui";

interface PrinterInfo {
  name: string;
  status: string;
  isDefault: boolean;
}

function detectCupsUrl(): string {
  try {
    const file = Gio.File.new_for_path("/etc/cups/cupsd.conf");
    if (file.query_exists(null)) {
      const [ok, bytes] = file.load_contents(null);
      if (ok) {
        const text = new TextDecoder().decode(bytes);
        const match = text.match(/^\s*(?:Listen|Port)\s+(?:([^:\s]+):)?(\d+)/m);
        if (match) {
          const host = match[1] && match[1] !== "*" ? match[1] : "localhost";
          const port = match[2] || "631";
          return `http://${host}:${port}`;
        }
      }
    }
  } catch {
    // ignore unreadable cupsd.conf
  }
  return "http://localhost:631";
}

function checkCupsWeb(
  url: string,
): Promise<{ reachable: boolean; webEnabled: boolean; detail: string }> {
  return new Promise((resolve) => {
    try {
      const parsed = GLib.Uri.parse(url, GLib.UriFlags.NONE);
      const host = parsed.get_host() || "localhost";
      const port = parsed.get_port() > 0 ? parsed.get_port() : 631;

      const client = new Gio.SocketClient({ timeout: 2 });
      client.connect_to_host_async(host, port, null, (_c, res) => {
        try {
          const conn = client.connect_to_host_finish(res);
          const out = conn.get_output_stream();
          const req = `GET / HTTP/1.0\r\nHost: ${host}:${port}\r\n\r\n`;
          out.write_all(new TextEncoder().encode(req), null);

          const input = new Gio.DataInputStream({
            base_stream: conn.get_input_stream(),
          });
          let body = "";
          const readChunk = () => {
            input.read_line_async(GLib.PRIORITY_DEFAULT, null, (_s, r) => {
              try {
                const [line] = input.read_line_finish_utf8(r);
                if (line === null || body.length > 4096) {
                  conn.close(null);
                  const firstLine = body.split("\n")[0] || "";
                  const disabled =
                    /web interface is disabled/i.test(body) ||
                    /\s403\s|\s404\s/.test(firstLine);
                  resolve({
                    reachable: true,
                    webEnabled: !disabled,
                    detail: disabled
                      ? "CUPS web interface is disabled on this system"
                      : `Available at ${url}`,
                  });
                  return;
                }
                body += line + "\n";
                readChunk();
              } catch {
                resolve({
                  reachable: true,
                  webEnabled: false,
                  detail: "Unable to read CUPS HTTP response",
                });
              }
            });
          };
          readChunk();
        } catch {
          resolve({
            reachable: false,
            webEnabled: false,
            detail: `CUPS is not listening on ${host}:${port}`,
          });
        }
      });
    } catch {
      resolve({
        reachable: false,
        webEnabled: false,
        detail: "Invalid CUPS URL",
      });
    }
  });
}

async function fetchPrinters(): Promise<{
  available: boolean;
  printers: PrinterInfo[];
}> {
  if (!GLib.find_program_in_path("lpstat")) {
    return { available: false, printers: [] };
  }

  let defaultName = "";
  try {
    const defOut = await execAsync(["lpstat", "-d"]);
    const m = defOut.match(/:\s*(\S+)/);
    if (m) defaultName = m[1];
  } catch {
    // no default printer
  }

  try {
    const out = await execAsync(["lpstat", "-p"]);
    const printers: PrinterInfo[] = [];
    for (const rawLine of out.split("\n")) {
      const line = rawLine.trim();
      const match = line.match(/^printer\s+(\S+)\s+(?:is\s+)?(.+?)\.?\s*$/i);
      if (match) {
        const name = match[1];
        const status = match[2];
        printers.push({
          name,
          status,
          isDefault: name === defaultName,
        });
      }
    }
    return { available: true, printers };
  } catch {
    return { available: true, printers: [] };
  }
}

export function buildPrintersPage(): Gtk.Widget {
  const page = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 14,
  });
  page.add_css_class("prefs-page-body");

  // --- Configured Printers ---
  const listHeader = new Gtk.Box({ spacing: 8 });
  const listTitle = new Gtk.Label({
    label: "Printers",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  listTitle.add_css_class("prefs-group-title");

  const refreshBtn = new Gtk.Button({
    tooltip_text: "Refresh",
    child: new Gtk.Image({ icon_name: "view-refresh-symbolic" }),
  });
  refreshBtn.add_css_class("prefs-btn");

  listHeader.append(listTitle);
  listHeader.append(refreshBtn);
  page.append(listHeader);

  const printersGroup = makeGroupBox();
  page.append(printersGroup);

  // --- CUPS Web Interface ---
  const cupsTitle = new Gtk.Label({
    label: "CUPS Administration",
    halign: Gtk.Align.START,
  });
  cupsTitle.add_css_class("prefs-group-title");
  page.append(cupsTitle);

  const cupsGroup = makeGroupBox();
  const cupsRow = makeRow(true);

  const cupsInfo = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 2,
    hexpand: true,
  });
  const cupsLabel = new Gtk.Label({
    label: "CUPS Web Interface",
    halign: Gtk.Align.START,
  });
  cupsLabel.add_css_class("prefs-row-title");
  const cupsSub = new Gtk.Label({
    label: "Checking CUPS status...",
    halign: Gtk.Align.START,
    wrap: true,
  });
  cupsSub.add_css_class("prefs-muted");
  cupsInfo.append(cupsLabel);
  cupsInfo.append(cupsSub);

  let detectedUrl = detectCupsUrl();

  const openCupsBtn = new Gtk.Button({
    label: "Open CUPS",
    sensitive: false,
    valign: Gtk.Align.CENTER,
  });
  openCupsBtn.add_css_class("prefs-btn");
  openCupsBtn.connect("clicked", () => {
    try {
      Gio.AppInfo.launch_default_for_uri(detectedUrl, null);
    } catch (err) {
      console.error("[printers] failed to open CUPS URL:", err);
    }
  });

  cupsRow.append(cupsInfo);
  cupsRow.append(openCupsBtn);
  cupsGroup.append(cupsRow);
  page.append(cupsGroup);

  const refresh = async () => {
    let child = printersGroup.get_first_child();
    while (child) {
      const next = child.get_next_sibling();
      printersGroup.remove(child);
      child = next;
    }

    const { available, printers } = await fetchPrinters();
    if (!available) {
      const row = makeRow(true);
      const lbl = new Gtk.Label({
        label: "CUPS tools (lpstat) not found in PATH",
        halign: Gtk.Align.START,
      });
      lbl.add_css_class("prefs-muted");
      row.append(lbl);
      printersGroup.append(row);
    } else if (printers.length === 0) {
      const row = makeRow(true);
      const lbl = new Gtk.Label({
        label: "No printers configured",
        halign: Gtk.Align.START,
      });
      lbl.add_css_class("prefs-muted");
      row.append(lbl);
      printersGroup.append(row);
    } else {
      printers.forEach((p, idx) => {
        const row = makeRow(idx === printers.length - 1);
        const icon = new Gtk.Image({ icon_name: "printer-symbolic" });
        const info = new Gtk.Box({
          orientation: Gtk.Orientation.VERTICAL,
          spacing: 2,
          hexpand: true,
        });
        const nameLbl = new Gtk.Label({
          label: p.isDefault ? `${p.name} (Default)` : p.name,
          halign: Gtk.Align.START,
        });
        nameLbl.add_css_class("prefs-row-title");
        const statusLbl = new Gtk.Label({
          label: p.status,
          halign: Gtk.Align.START,
          ellipsize: 3,
        });
        statusLbl.add_css_class("prefs-muted");
        info.append(nameLbl);
        info.append(statusLbl);

        row.append(icon);
        row.append(info);
        printersGroup.append(row);
      });
    }

    detectedUrl = detectCupsUrl();
    const web = await checkCupsWeb(detectedUrl);
    openCupsBtn.set_sensitive(web.webEnabled);
    cupsSub.label = web.detail;
  };

  refreshBtn.connect("clicked", () => refresh());
  refresh();

  return page;
}
