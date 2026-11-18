import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { createState, For } from "ags";
import GLib from "gi://GLib";
import AstalNotifd from "gi://AstalNotifd";

export function NotificationToasts(gdkmonitor: Gdk.Monitor) {
  const { TOP } = Astal.WindowAnchor;
  const notifd = AstalNotifd.get_default();
  const [toasts, setToasts] = createState<AstalNotifd.Notification[]>([]);

  const removeToast = (id: number) => {
    setToasts((list) => list.filter((n) => n.id !== id));
  };

  notifd.connect("notified", (_src, id) => {
    if (notifd.dontDisturb) return;
    const notif = notifd.get_notification(id);
    if (!notif) return;

    setToasts((list) => [notif, ...list.filter((n) => n.id !== id)].slice(0, 3));

    GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
      removeToast(id);
      return GLib.SOURCE_REMOVE;
    });
  });

  notifd.connect("resolved", (_src, id) => {
    removeToast(id);
  });

  return (
    <window
      name="notification-toasts"
      class="toast-window"
      gdkmonitor={gdkmonitor}
      anchor={TOP}
      marginTop={8}
      visible={toasts((list) => list.length > 0)}
      application={app}
    >
      <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
        <For each={toasts}>
          {(n) => (
            <box
              class="popup-card notif-toast"
              orientation={Gtk.Orientation.VERTICAL}
              spacing={4}
            >
              <box spacing={8}>
                <image
                  class="notif-app-icon"
                  iconName={n.appIcon || "dialog-information-symbolic"}
                />
                <label
                  class="notif-summary"
                  hexpand
                  halign={Gtk.Align.START}
                  ellipsize={3}
                  label={n.summary}
                />
                <button
                  class="notif-dismiss-btn"
                  onClicked={() => removeToast(n.id)}
                >
                  <image iconName="window-close-symbolic" />
                </button>
              </box>
              {n.body ? (
                <label
                  class="notif-body"
                  halign={Gtk.Align.START}
                  wrap
                  label={n.body}
                />
              ) : (
                <box />
              )}
            </box>
          )}
        </For>
      </box>
    </window>
  );
}
