import { Gtk } from "ags/gtk4";
import { createState, For } from "ags";
import AstalNotifd from "gi://AstalNotifd";

function formatNotifTime(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  return new Intl.DateTimeFormat("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function NotificationsSection() {
  const notifd = AstalNotifd.get_default();

  const [notifications, setNotifications] = createState<
    AstalNotifd.Notification[]
  >([...notifd.notifications].reverse());
  const [dnd, setDnd] = createState(notifd.dontDisturb);

  const sync = () => {
    setNotifications([...notifd.notifications].reverse());
  };

  notifd.connect("notified", sync);
  notifd.connect("resolved", sync);
  notifd.connect("notify::notifications", sync);
  notifd.connect("notify::dont-disturb", () => setDnd(notifd.dontDisturb));

  return (
    <box
      class="notif-section"
      vexpand
      orientation={Gtk.Orientation.VERTICAL}
      spacing={12}
    >
      <box class="notif-header" spacing={8}>
        <label
          class="qs-title"
          hexpand
          halign={Gtk.Align.START}
          label="Notifications"
        />

        <button
          class={`notif-dnd-btn ${dnd((d) => (d ? "active" : ""))}`}
          tooltipText="Do Not Disturb"
          onClicked={() => {
            notifd.dontDisturb = !notifd.dontDisturb;
          }}
        >
          <box spacing={6}>
            <image
              iconName={dnd((d) =>
                d
                  ? "notifications-disabled-symbolic"
                  : "preferences-system-notifications-symbolic",
              )}
            />
            <label label="Do Not Disturb" />
          </box>
        </button>

        <button
          class="notif-clear-btn"
          visible={notifications((list) => list.length > 0)}
          onClicked={() => {
            for (const n of [...notifd.notifications]) {
              n.dismiss();
            }
          }}
        >
          <label label="Clear" />
        </button>
      </box>

      <box
        class="notif-empty"
        orientation={Gtk.Orientation.VERTICAL}
        spacing={8}
        vexpand
        valign={Gtk.Align.CENTER}
        visible={notifications((list) => list.length === 0)}
      >
        <image iconName="preferences-system-notifications-symbolic" />
        <label label="No Notifications" />
      </box>

      <Gtk.ScrolledWindow
        class="notif-scroll"
        hscrollbarPolicy={Gtk.PolicyType.NEVER}
        vexpand
        visible={notifications((list) => list.length > 0)}
      >
        <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
          <For each={notifications}>
            {(n) => (
              <box
                class="notif-item"
                orientation={Gtk.Orientation.VERTICAL}
                spacing={4}
              >
                <box spacing={8}>
                  <image
                    class="notif-app-icon"
                    iconName={n.appIcon || "dialog-information-symbolic"}
                  />
                  <label
                    class="notif-app-name"
                    hexpand
                    halign={Gtk.Align.START}
                    label={n.appName || "System"}
                  />
                  <label
                    class="notif-time"
                    label={formatNotifTime(n.time)}
                  />
                  <button
                    class="notif-dismiss-btn"
                    onClicked={() => n.dismiss()}
                  >
                    <image iconName="window-close-symbolic" />
                  </button>
                </box>

                <label
                  class="notif-summary"
                  halign={Gtk.Align.START}
                  wrap
                  label={n.summary}
                />
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
      </Gtk.ScrolledWindow>
    </box>
  );
}
