import app from "ags/gtk4/app";
import { Gdk, Gtk } from "ags/gtk4";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import AstalNotifd from "gi://AstalNotifd";
import style from "./styles/main.scss";
import { Bar } from "./widgets/Bar";
import { CalendarPopup } from "./widgets/CalendarPopup";
import { displayWorkspaces, WorkspaceDialog } from "./widgets/WorkspaceDialog";
import { QuickSettings } from "./widgets/QuickSettings";
import { NotificationToasts } from "./widgets/NotificationToasts";
import { ClipboardPopup } from "./widgets/ClipboardPopup";
import { toggleLauncher } from "./widgets/Launcher";
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
import { unreadNotificationCount } from "./services/unread-notifications";

const rustCommand = GLib.getenv("PALOBAR_RUST_COMMAND");
const rustRequests = new Set(["run-command", "toggle-dock", "toggle-quick-settings", "lock"]);

app.start({
  css: style,
  requestHandler(request, res) {
    const cmd = request[0];
    if (rustCommand && rustRequests.has(cmd)) {
      try {
        Gio.Subprocess.new([rustCommand, cmd], Gio.SubprocessFlags.NONE);
        res("ok");
      } catch (error) {
        console.error(`[palobar] Could not start Rust command ${cmd}:`, error);
        res(`Could not start ${cmd}`);
      }
      return;
    }
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
    if (cmd === "get-active-popup") {
      res(popups.active((active) => active) ?? "none");
      return;
    }
    if (cmd === "get-unread-count") {
      res(unreadNotificationCount((count) => String(count)));
      return;
    }
    if (cmd === "get-bar-state") {
      res(JSON.stringify({
        active_popup: popups.active((active) => active),
        unread_count: unreadNotificationCount((count) => count),
      }));
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
    if (!rustCommand) {
      monitors.forEach((monitor, index) => Bar(monitor, index));
    }

    const primary = monitors[0];
    if (primary) {
      CalendarPopup(primary);
      if (!rustCommand) QuickSettings(primary);
      WorkspaceDialog(primary);
      NotificationToasts(primary);
      ClipboardPopup(primary);
    }
  },
});
