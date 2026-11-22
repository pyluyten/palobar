import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { createPoll } from "ags/time";
import { popups } from "../services/popups";
import { MediaControl } from "./MediaControl";
import { NotificationsSection } from "./NotificationsPopup";
import { PopupDismissArea } from "./PopupDismissArea";

function getFullDate(): { weekday: string; fullDate: string } {
  const now = new Date();
  return {
    weekday: new Intl.DateTimeFormat("en-US", { weekday: "long" }).format(now),
    fullDate: new Intl.DateTimeFormat("en-US", {
      month: "long",
      day: "numeric",
      year: "numeric",
    }).format(now),
  };
}

export function CalendarPopup(gdkmonitor: Gdk.Monitor) {
  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;
  const header = createPoll(getFullDate(), 30000, getFullDate);

  return (
    <window
      name="calendar-popup"
      class="popup-window"
      gdkmonitor={gdkmonitor}
      anchor={TOP | BOTTOM | LEFT | RIGHT}
      exclusivity={Astal.Exclusivity.NORMAL}
      keymode={Astal.Keymode.ON_DEMAND}
      visible={popups.active((p) => p === "calendar")}
      application={app}
      $={(self) => {
        const keyCtrl = new Gtk.EventControllerKey();
        keyCtrl.connect("key-pressed", (_ctrl, keyval) => {
          if (keyval === Gdk.KEY_Escape) {
            popups.close();
            return true;
          }
          return false;
        });
        self.add_controller(keyCtrl);
      }}
    >
      <PopupDismissArea>
        <box
          class="popup-card calendar-card"
          halign={Gtk.Align.CENTER}
          valign={Gtk.Align.START}
          marginTop={6}
          spacing={18}
        >
          <box class="date-menu-feed" orientation={Gtk.Orientation.VERTICAL} spacing={14}>
            <MediaControl />
            <Gtk.Separator />
            <NotificationsSection />
          </box>
          <Gtk.Separator orientation={Gtk.Orientation.VERTICAL} />
          <box class="date-menu-calendar" orientation={Gtk.Orientation.VERTICAL} spacing={12}>
            <box class="calendar-header" orientation={Gtk.Orientation.VERTICAL} spacing={2}>
              <label
                class="calendar-weekday"
                halign={Gtk.Align.START}
                label={header((h) => h.weekday)}
              />
              <label
                class="calendar-date"
                halign={Gtk.Align.START}
                label={header((h) => h.fullDate)}
              />
            </box>
            <Gtk.Calendar class="gnome-calendar" />
          </box>
        </box>
      </PopupDismissArea>
    </window>
  );
}
