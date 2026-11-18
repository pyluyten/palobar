import app from "ags/gtk4/app";
import style from "./styles/main.scss";
import { Bar } from "./widgets/Bar";
import { CalendarPopup } from "./widgets/CalendarPopup";
import { QuickSettings } from "./widgets/QuickSettings";
import { NotificationsPopup } from "./widgets/NotificationsPopup";
import { NotificationToasts } from "./widgets/NotificationToasts";
import { toggleLauncher } from "./widgets/Launcher";
import { initWallpaperService } from "./services/wallpaper";
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
    if (cmd === "lock") {
      lockScreen();
      res("ok");
      return;
    }
    res(`unknown request: ${cmd ?? ""}`);
  },
  main() {
    initWallpaperService();

    const monitors = app.get_monitors();
    monitors.forEach((monitor, index) => Bar(monitor, index));

    const primary = monitors[0];
    if (primary) {
      CalendarPopup(primary);
      QuickSettings(primary);
      NotificationsPopup(primary);
      NotificationToasts(primary);
    }
  },
});
