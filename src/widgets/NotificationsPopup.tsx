import { Gdk, Gtk } from "ags/gtk4";
import { createState, For } from "ags";
import AstalNotifd from "gi://AstalNotifd";
import Gio from "gi://Gio";
import Pango from "gi://Pango";

interface NotificationGroup {
  key: string;
  notifications: AstalNotifd.Notification[];
}

function groupNotifications(notifications: AstalNotifd.Notification[]) {
  const groups = new Map<string, NotificationGroup>();
  const newestFirst = [...notifications].reverse().sort((a, b) => b.time - a.time);
  for (const notification of newestFirst) {
    const desktopId = notification.desktopEntry?.trim().replace(/\.desktop$/, "");
    const appName = notification.appName?.trim().toLowerCase();
    const key = desktopId ? `desktop:${desktopId}` : `name:${appName || ""}`;
    let group = groups.get(key);
    if (!group) {
      group = { key, notifications: [] };
      groups.set(key, group);
    }
    group.notifications.push(notification);
  }
  return [...groups.values()];
}

function NotificationAppIcon({ notification }: { notification: AstalNotifd.Notification }) {
  const desktopId = notification.desktopEntry?.trim().replace(/\.desktop$/, "");
  const desktopIcon = desktopId ? Gio.DesktopAppInfo.new(`${desktopId}.desktop`)?.get_icon() : null;
  if (desktopIcon) return <image gicon={desktopIcon} pixelSize={24} />;

  const icon = notification.appIcon;
  if (icon?.startsWith("/") || icon?.startsWith("file://")) {
    const file = Gio.File.new_for_commandline_arg(icon);
    return <image gicon={new Gio.FileIcon({ file })} pixelSize={24} />;
  }

  const display = Gdk.Display.get_default();
  const theme = display && Gtk.IconTheme.get_for_display(display);
  const iconName = icon && theme?.has_icon(icon) ? icon : "dialog-information-symbolic";
  return <image iconName={iconName} pixelSize={24} />;
}

function formatNotifTime(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  return new Intl.DateTimeFormat("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function NotificationRow({ notification }: { notification: AstalNotifd.Notification }) {
  return (
    <box class="notif-item" orientation={Gtk.Orientation.VERTICAL} spacing={4}>
      <box spacing={6}>
        <label
          class="notif-summary"
          hexpand
          xalign={0}
          ellipsize={Pango.EllipsizeMode.END}
          label={notification.summary}
        />
        <label class="notif-time" label={formatNotifTime(notification.time)} />
        <button
          class="notif-dismiss-btn"
          tooltipText="Dismiss Notification"
          onClicked={() => notification.dismiss()}
        >
          <image iconName="window-close-symbolic" />
        </button>
      </box>
      {notification.body ? (
        <label class="notif-body" xalign={0} wrap label={notification.body} />
      ) : null}
    </box>
  );
}

export function NotificationsSection() {
  const notifd = AstalNotifd.get_default();

  const [groups, setGroups] = createState<NotificationGroup[]>([]);
  const [expanded, setExpanded] = createState<ReadonlySet<string>>(new Set());
  const [dnd, setDnd] = createState(notifd.dontDisturb);
  const hasNotifications = groups((list) => list.length > 0);

  const sync = () => {
    const next = groupNotifications(notifd.notifications);
    setExpanded((current) => {
      const retained = new Set(
        next.filter((group) => group.notifications.length > 1 && current.has(group.key))
          .map((group) => group.key),
      );
      return retained.size === current.size ? current : retained;
    });
    setGroups(next);
  };

  return (
    <box
      class="notif-section"
      $={(self) => {
        let signals: number[] = [];
        self.connect("map", () => {
          sync();
          setDnd(notifd.dontDisturb);
          signals = [
            notifd.connect("notified", sync),
            notifd.connect("resolved", sync),
            notifd.connect("notify::dont-disturb", () => setDnd(notifd.dontDisturb)),
          ];
        });
        self.connect("unmap", () => {
          for (const id of signals) notifd.disconnect(id);
          signals = [];
          setGroups([]);
          setExpanded(new Set());
        });
      }}
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
          visible={hasNotifications}
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
        visible={hasNotifications((value) => !value)}
      >
        <image iconName="preferences-system-notifications-symbolic" />
        <label label="No Notifications" />
      </box>

      <Gtk.ScrolledWindow
        class="notif-scroll"
        hscrollbarPolicy={Gtk.PolicyType.NEVER}
        minContentWidth={340}
        maxContentHeight={440}
        propagateNaturalHeight
        vexpand
        visible={hasNotifications}
      >
        <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
          <For each={groups}>
            {(group) => {
              const [latest, ...older] = group.notifications;
              const isExpanded = expanded((keys) => keys.has(group.key));
              return (
                <box class="notif-group" orientation={Gtk.Orientation.VERTICAL} spacing={6}>
                  <box class="notif-group-header" spacing={8}>
                    <NotificationAppIcon notification={latest} />
                    <label
                      class="notif-group-name"
                      hexpand
                      xalign={0}
                      ellipsize={Pango.EllipsizeMode.END}
                      label={latest.appName || "System"}
                    />
                    {older.length > 0 && (
                      <button
                        class="notif-expand-btn"
                        tooltipText={isExpanded((open) =>
                          open ? "Collapse Notifications" : "Expand Notifications"
                        )}
                        onClicked={() => setExpanded((current) => {
                          const next = new Set(current);
                          if (!next.delete(group.key)) next.add(group.key);
                          return next;
                        })}
                      >
                        <box spacing={6}>
                          <label label={String(group.notifications.length)} />
                          <image iconName={isExpanded((open) =>
                            open ? "pan-up-symbolic" : "pan-down-symbolic"
                          )} />
                        </box>
                      </button>
                    )}
                  </box>
                  <NotificationRow notification={latest} />
                  <For each={isExpanded((open) => open ? older : [])}>
                    {(notification) => <NotificationRow notification={notification} />}
                  </For>
                </box>
              );
            }}
          </For>
        </box>
      </Gtk.ScrolledWindow>
    </box>
  );
}
