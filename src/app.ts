import app from "ags/gtk4/app";
import { Gdk, Gtk } from "ags/gtk4";
import style from "./styles/main.scss";
import { Bar } from "./widgets/Bar";
import { CalendarPopup } from "./widgets/CalendarPopup";
import { QuickSettings } from "./widgets/QuickSettings";
import { NotificationToasts } from "./widgets/NotificationToasts";
import { ClipboardPopup } from "./widgets/ClipboardPopup";
import { toggleLauncher } from "./widgets/Launcher";
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

app.start({
  css: style,
  requestHandler(request, res) {
    const cmd = request[0];
    if (cmd === "toggle-launcher" || cmd === "launcher") {
      toggleLauncher();
      res("ok");
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
    monitors.forEach((monitor, index) => Bar(monitor, index));

    const primary = monitors[0];
    if (primary) {
      CalendarPopup(primary);
      QuickSettings(primary);
      NotificationToasts(primary);
      ClipboardPopup(primary);
    }
  },
});
