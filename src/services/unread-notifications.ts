import { createState } from "ags";
import AstalNotifd from "gi://AstalNotifd";
import { popups } from "./popups";

const unread = new Set<number>();
const [count, setCount] = createState(0);

export const unreadNotificationCount = count;

export function initUnreadNotifications() {
  const notifd = AstalNotifd.get_default();
  for (const notification of notifd.notifications) unread.add(notification.id);
  setCount(unread.size);

  notifd.connect("notified", (_source, id) => {
    if (popups.active.get() === "calendar") return;
    unread.add(id);
    setCount(unread.size);
  });

  notifd.connect("resolved", (_source, id) => {
    if (unread.delete(id)) setCount(unread.size);
  });

  popups.active.subscribe(() => {
    if (popups.active.get() === "calendar") markNotificationsRead();
  });
}

export function markNotificationsRead() {
  if (unread.size === 0) return;
  unread.clear();
  setCount(0);
}
