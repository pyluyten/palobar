import { Gdk, Gtk } from "ags/gtk4";
import AstalApps from "gi://AstalApps";
import Gio from "gi://Gio";
import { addFavoriteApp, configService } from "../services/config";
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
  onActivateApp: () => void;
  acceptFavorites?: boolean;
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

  let windows: NiriWindow[] = [];
  let unsubscribe: (() => void) | undefined;

  const appendApp = (
    name: string,
    iconName: string,
    running: boolean,
    activate: () => void,
  ) => {
    const button = new Gtk.Button({ tooltip_text: name });
    button.add_css_class("launcher-dock-item");

    const item = new Gtk.Box({
      orientation: Gtk.Orientation.VERTICAL,
      spacing: 4,
      halign: Gtk.Align.CENTER,
    });
    item.append(new Gtk.Image({ icon_name: iconName, pixel_size: 40 }));

    const dot = new Gtk.Box({
      width_request: 5,
      height_request: 5,
      halign: Gtk.Align.CENTER,
      opacity: running ? 1 : 0,
    });
    dot.add_css_class("launcher-dock-dot");
    item.append(dot);

    button.set_child(item);
    button.connect("clicked", () => {
      opts.onActivateApp();
      activate();
    });
    dockBox.append(button);
  };

  const appKey = (id: string) => id.trim().toLowerCase().replace(/\.desktop$/, "");

  const rebuild = (favoriteIds: string[]) => {
    for (let child = dockBox.get_first_child(); child; child = dockBox.get_first_child()) {
      dockBox.remove(child);
    }

    const runningApps = new Map<string, NiriWindow>();
    for (const window of windows) {
      const key = appKey(window.app_id || window.title || "");
      const previous = runningApps.get(key);
      if (key && (!previous || window.is_focused)) runningApps.set(key, window);
    }

    const appsByEntry = new Map(opts.appsService.list.map((app) => [app.entry, app]));
    for (const id of favoriteIds) {
      const favorite = appsByEntry.get(id);
      if (!favorite) continue;

      const keys = [appKey(id), appKey(favorite.wmClass || "")];
      const matches = keys.flatMap((key) => {
        const window = runningApps.get(key);
        runningApps.delete(key);
        return window ? [window] : [];
      });
      const running = matches.find((window) => window.is_focused) ?? matches[0];
      appendApp(
        favorite.name,
        favorite.iconName || "application-x-executable",
        Boolean(running),
        () => running ? niri.focusWindow(running.id) : favorite.launch(),
      );
    }

    for (const window of runningApps.values()) {
      const appId = window.app_id || window.title || "application";
      const { name, iconName } = resolveAppInfo(appId, opts.appsService);
      appendApp(name, iconName, true, () => niri.focusWindow(window.id));
    }

    if (opts.onShowApps) {
      const showApps = opts.onShowApps;
      const button = new Gtk.Button({
        tooltip_text: "Show Applications",
        child: new Gtk.Image({
          icon_name: "view-app-grid-symbolic",
          pixel_size: 32,
        }),
      });
      button.add_css_class("launcher-dock-item");
      button.connect("clicked", showApps);
      dockBox.append(button);
    }

    if (opts.acceptFavorites && !dockBox.get_first_child()) {
      dockBox.append(new Gtk.Label({
        label: "Drop an app here to add it to favorites",
        height_request: 56,
      }));
    }
  };

  if (opts.acceptFavorites) {
    const drop = Gtk.DropTarget.new(AstalApps.Application.$gtype, Gdk.DragAction.COPY);
    drop.connect("drop", (_target, value) =>
      value instanceof AstalApps.Application && addFavoriteApp(value.entry),
    );
    dockBox.add_controller(drop);
  }

  rebuild(configService.get().favoriteApps);
  dockBox.connect("map", () => {
    unsubscribe = configService.subscribe((cfg) => rebuild(cfg.favoriteApps));
  });
  dockBox.connect("unmap", () => {
    unsubscribe?.();
    unsubscribe = undefined;
  });
  dockBox.set_visible(true);

  niri.getWindows().then((list) => {
    if (!opts.isAlive()) return;
    windows = list;
    rebuild(configService.get().favoriteApps);
  });

  return dockBox;
}
