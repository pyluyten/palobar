import app from "ags/gtk4/app";
import style from "./styles/main.scss";
import { Bar } from "./widgets/Bar";
import { CalendarPopup } from "./widgets/CalendarPopup";
import { QuickSettings } from "./widgets/QuickSettings";
import { NotificationsPopup } from "./widgets/NotificationsPopup";
import { NotificationToasts } from "./widgets/NotificationToasts";

app.start({
  css: style,
  main() {
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
