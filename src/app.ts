import app from "ags/gtk4/app";
import { Gdk, Gtk } from "ags/gtk4";
import AstalNotifd from "gi://AstalNotifd";
import style from "./styles/main.scss";
import { Bar } from "./widgets/Bar";
import { CalendarPopup } from "./widgets/CalendarPopup";
import { displayWorkspaces, WorkspaceDialog } from "./widgets/WorkspaceDialog";
import { QuickSettings } from "./widgets/QuickSettings";
import { NotificationToasts } from "./widgets/NotificationToasts";
import { ClipboardPopup } from "./widgets/ClipboardPopup";
import { closeLauncher, toggleLauncher } from "./widgets/Launcher";
import { toggleStandaloneDock } from "./widgets/StandaloneDock";
import { toggleRunCommand } from "./widgets/RunCommand";
import { initOverviewCapture } from "./widgets/OverviewCapture";
import { initWallpaperService, stopWallpaper } from "./services/wallpaper";
import { initPowerService, stopPowerService } from "./services/power";
import {
  initClipboardService,
  stopClipboardService,
} from "./services/clipboard";
import { getAccentColor, subscribeAppearance } from "./services/appearance";
import { lockScreen } from "./services/lockscreen";
import { popups } from "./services/popups";
import { initUnreadNotifications, markNotificationsRead, unreadNotificationCount } from "./services/unread-notifications";

app.start({
  css: style,
  requestHandler(request, res) {
    const cmd = request[0];
    if (cmd === "toggle-launcher" || cmd === "launcher") {
      toggleLauncher();
      res("ok");
      return;
    }
    if (cmd === "toggle-dock") {
      toggleStandaloneDock();
      res("ok");
      return;
    }
    if (cmd === "display-workspaces") {
      const monitorIndex = Number(request[1]);
      const monitor = Number.isInteger(monitorIndex)
        ? app.get_monitors()[monitorIndex]
        : undefined;
      displayWorkspaces(monitor);
      res("ok");
      return;
    }
    if (cmd === "toggle-calendar") {
      popups.toggle("calendar");
      res("ok");
      return;
    }
    if (cmd === "toggle-clipboard") {
      popups.toggle("clipboard");
      res("ok");
      return;
    }
    if (cmd === "toggle-quick-settings") {
      popups.toggle("quick-settings");
      res("ok");
      return;
    }
    if (cmd === "close-popup") {
      popups.close();
      res("ok");
      return;
    }
    if (cmd === "get-quick-settings-dnd" || cmd === "toggle-quick-settings-dnd") {
      const notifd = AstalNotifd.get_default();
      if (cmd === "toggle-quick-settings-dnd") {
        notifd.dontDisturb = !notifd.dontDisturb;
      }
      res(String(notifd.dontDisturb));
      return;
    }
    if (cmd === "get-calendar-notifications") {
      const notifd = AstalNotifd.get_default();
      markNotificationsRead();
      res(JSON.stringify({
        dnd: notifd.dontDisturb,
        notifications: notifd.notifications.map((notification) => ({
          id: notification.id,
          time: notification.time,
          app_name: notification.appName ?? "",
          desktop_entry: notification.desktopEntry ?? "",
          app_icon: notification.appIcon ?? "",
          summary: notification.summary ?? "",
          body: notification.body ?? "",
        })),
      }));
      return;
    }
    if (cmd === "dismiss-calendar-notification" || cmd === "clear-calendar-notifications") {
      const notifications = AstalNotifd.get_default().notifications;
      if (cmd === "clear-calendar-notifications") {
        for (const notification of [...notifications]) notification.dismiss();
      } else {
        notifications.find((notification) => String(notification.id) === request[1])?.dismiss();
      }
      res("ok");
      return;
    }
    if (cmd === "get-active-popup") {
      res(popups.active.get() ?? "none");
      return;
    }
    if (cmd === "get-unread-count" || cmd === "get-bar-state") {
      const count = unreadNotificationCount.get();
      res(cmd === "get-unread-count"
        ? String(count)
        : JSON.stringify({ active_popup: popups.active.get(), unread_count: count }));
      return;
    }
    if (cmd === "run-command") {
      toggleRunCommand();
      res("ok");
      return;
    }
    if (cmd === "lock") {
      void lockScreen().then((error) => res(error ?? "ok"));
      return;
    }
    if (cmd === "quit") {
      closeLauncher();
      stopWallpaper();
      stopPowerService();
      stopClipboardService();
      res("ok");
      app.quit();
      return;
    }
    res(`unknown request: ${cmd ?? ""}`);
  },
  main() {
    const accentProvider = new Gtk.CssProvider();
    // Astal installs the bundled stylesheet at USER priority.
    Gtk.StyleContext.add_provider_for_display(
      Gdk.Display.get_default()!,
      accentProvider,
      Gtk.STYLE_PROVIDER_PRIORITY_USER + 1,
    );
    subscribeAppearance("accent-color", () => {
      const accent = getAccentColor().hex;
      accentProvider.load_from_string(`
        .notif-badge,
        .qs-circle-btn.active,
        .qs-chevron-btn.active,
        .qs-toggle-pill.active,
        .qs-mini-pill.active,
        .qs-slider highlight,
        .notif-dnd-btn.active {
          background-color: ${accent};
        }
        .launcher-search:focus {
          border-color: ${accent};
        }
      `);
    });

    initWallpaperService();
    initPowerService();
    initClipboardService();
    initOverviewCapture();

    const monitors = app.get_monitors();
    initUnreadNotifications();
    monitors.forEach((monitor, index) => Bar(monitor, index));

    const primary = monitors[0];
    if (primary) {
      CalendarPopup(primary);
      QuickSettings(primary);
      NotificationToasts(primary);
      ClipboardPopup(primary);
      WorkspaceDialog(primary);
    }
  },
});
