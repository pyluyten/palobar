import { createState } from "ags";
import AstalNotifd from "gi://AstalNotifd";
import { popups } from "./popups";

const notifd = AstalNotifd.get_default();
const unread = new Set(notifd.notifications.map((notification) => notification.id));
const [count, setCount] = createState(unread.size);

export const unreadNotificationCount = count;

notifd.connect("notified", (_source, id) => {
  if (popups.active.get() === "calendar") return;
  unread.add(id);
  setCount(unread.size);
});

notifd.connect("resolved", (_source, id) => {
  if (unread.delete(id)) setCount(unread.size);
});

export function markNotificationsRead() {
  if (unread.size === 0) return;
  unread.clear();
  setCount(0);
}

popups.active.subscribe(() => {
  if (popups.active.get() === "calendar") markNotificationsRead();
});
