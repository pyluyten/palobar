import { Gtk } from "ags/gtk4";
import AstalApps from "gi://AstalApps";
import Gio from "gi://Gio";
import { niri, type NiriWindow } from "../services/niri";

export function resolveAppInfo(
  appId: string,
  appsService: AstalApps.Apps,
): { name: string; iconName: string } {
  const desktopCandidates = [
    appId,
    `${appId}.desktop`,
    appId.toLowerCase(),
    `${appId.toLowerCase()}.desktop`,
  ];
  for (const candidate of desktopCandidates) {
    if (!candidate.endsWith(".desktop")) continue;
    try {
      const info = Gio.DesktopAppInfo.new(candidate);
      if (info) {
        const icon = info.get_icon()?.to_string() || appId;
        return { name: info.get_display_name() || appId, iconName: icon };
      }
    } catch {
      // ignore
    }
  }

  const lower = appId.toLowerCase();
  const match = appsService.list.find(
    (a) =>
      a.entry?.toLowerCase().replace(/\.desktop$/, "") === lower ||
      a.wmClass?.toLowerCase() === lower ||
      a.name.toLowerCase() === lower,
  );
  if (match) {
    return {
      name: match.name,
      iconName: match.iconName || "application-x-executable",
    };
  }

  const fuzzy = appsService.fuzzy_query(appId)[0];
  if (fuzzy) {
    return {
      name: fuzzy.name,
      iconName: fuzzy.iconName || "application-x-executable",
    };
  }

  return { name: appId, iconName: appId || "application-x-executable" };
}

export interface MiniDockOptions {
  appsService: AstalApps.Apps;
  isAlive: () => boolean;
  onFocusWindow: () => void;
  onShowApps?: () => void;
}

export function createMiniDock(opts: MiniDockOptions): Gtk.Box {
  const dockBox = new Gtk.Box({
    spacing: 8,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.END,
    visible: false,
  });
  dockBox.add_css_class("launcher-dock");

  niri.getWindows().then((windows: NiriWindow[]) => {
    if (!opts.isAlive()) return;

    const seen = new Map<string, NiriWindow>();
    for (const w of windows) {
      const key = (w.app_id || w.title || "").trim().toLowerCase();
      if (!key || seen.has(key)) continue;
      seen.set(key, w);
    }

    for (const [, winItem] of seen) {
      const appId = winItem.app_id || winItem.title || "application";
      const resolved = resolveAppInfo(appId, opts.appsService);

      const dockBtn = new Gtk.Button({
        tooltip_text: resolved.name,
      });
      dockBtn.add_css_class("launcher-dock-item");

      const itemBox = new Gtk.Box({
        orientation: Gtk.Orientation.VERTICAL,
        spacing: 4,
        halign: Gtk.Align.CENTER,
      });

      const img = new Gtk.Image({
        icon_name: resolved.iconName,
        pixel_size: 40,
      });

      const runningDot = new Gtk.Box({
        width_request: 5,
        height_request: 5,
        halign: Gtk.Align.CENTER,
      });
      runningDot.add_css_class("launcher-dock-dot");

      itemBox.append(img);
      itemBox.append(runningDot);
      dockBtn.set_child(itemBox);

      dockBtn.connect("clicked", () => {
        opts.onFocusWindow();
        niri.focusWindow(winItem.id);
      });

      dockBox.append(dockBtn);
    }

    if (opts.onShowApps) {
      const showAppsBtn = new Gtk.Button({
        tooltip_text: "Show Applications",
      });
      showAppsBtn.add_css_class("launcher-dock-item");
      showAppsBtn.set_child(
        new Gtk.Image({
          icon_name: "view-app-grid-symbolic",
          pixel_size: 32,
        }),
      );
      showAppsBtn.connect("clicked", () => opts.onShowApps?.());
      dockBox.append(showAppsBtn);
    }

    if (seen.size > 0 || opts.onShowApps) {
      dockBox.set_visible(true);
    }
  });

  return dockBox;
}
