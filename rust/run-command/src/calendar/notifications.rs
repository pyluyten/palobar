use crate::notifications::{app_icon, request, Notification, Snapshot};
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

impl Notification {
    fn desktop_id(&self) -> &str {
        let id = self.desktop_entry.trim();
        id.strip_suffix(".desktop").unwrap_or(id)
    }

    fn key(&self) -> String {
        let desktop = self.desktop_id();
        if desktop.is_empty() {
            format!("name:{}", self.app_name.trim().to_lowercase())
        } else {
            format!("desktop:{desktop}")
        }
    }
}

fn sections(notifications: &[Notification]) -> Vec<(String, Vec<&Notification>)> {
    let mut ordered: Vec<_> = notifications.iter().rev().collect();
    ordered.sort_by_key(|notification| std::cmp::Reverse(notification.time));
    let mut groups: Vec<(String, Vec<_>)> = Vec::new();
    let mut indices = HashMap::new();
    for notification in ordered {
        let key = notification.key();
        let index = *indices.entry(key.clone()).or_insert_with(|| {
            groups.push((key, Vec::new()));
            groups.len() - 1
        });
        groups[index].1.push(notification);
    }
    groups
}

struct Feed {
    list: gtk::Box,
    empty: gtk::Box,
    message: gtk::Label,
    scroll: gtk::ScrolledWindow,
    dnd: gtk::Button,
    dnd_image: gtk::Image,
    clear: gtk::Button,
    snapshot: RefCell<Option<Snapshot>>,
    expanded: RefCell<HashSet<String>>,
    busy: Cell<bool>,
    revision: Cell<u64>,
    pending: RefCell<Option<glib::JoinHandle<()>>>,
}

impl Feed {
    fn row(self: &Rc<Self>, notification: &Notification) -> gtk::Box {
        let item = gtk::Box::new(gtk::Orientation::Vertical, 4);
        item.add_css_class("date-notification-item");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let summary = super::label(&notification.summary, "date-notification-summary");
        summary.set_hexpand(true);
        header.append(&summary);
        let time = chrono::DateTime::from_timestamp(notification.time, 0)
            .map(|value| {
                value
                    .with_timezone(&chrono::Local)
                    .format("%I:%M %p")
                    .to_string()
            })
            .unwrap_or_default();
        header.append(&super::label(&time, "date-muted"));
        let dismiss = super::icon_button("window-close-symbolic", "Dismiss Notification");
        let id = notification.id;
        let weak = Rc::downgrade(self);
        dismiss.connect_clicked(move |_| {
            if let Some(feed) = weak.upgrade() {
                feed.act("Dismiss", Some(id));
            }
        });
        header.append(&dismiss);
        item.append(&header);
        if !notification.body.is_empty() {
            let body = gtk::Label::builder()
                .label(&notification.body)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .max_width_chars(38)
                .build();
            body.add_css_class("date-notification-body");
            item.append(&body);
        }
        item
    }

    fn act(self: &Rc<Self>, action: &'static str, id: Option<u32>) {
        if self.busy.replace(true) {
            return;
        }
        self.revision.set(self.revision.get().wrapping_add(1));
        self.dnd.set_sensitive(false);
        self.clear.set_sensitive(false);
        self.list.set_sensitive(false);
        let weak = Rc::downgrade(self);
        let handle = glib::spawn_future_local(async move {
            let params = id.map(|id| (id,).to_variant());
            if let Err(error) = request(action, params.as_ref()).await {
                eprintln!("[calendar] Notification action failed: {error}");
            }
            if let Some(feed) = weak.upgrade() {
                match read().await {
                    Some(snapshot) => feed.update(snapshot),
                    None => feed.unavailable(),
                }
                feed.busy.set(false);
                feed.dnd.set_sensitive(feed.snapshot.borrow().is_some());
                feed.clear.set_sensitive(true);
                feed.list.set_sensitive(true);
            }
        });
        self.pending.replace(Some(handle));
    }

    fn unavailable(&self) {
        if self.snapshot.borrow_mut().take().is_some() {
            super::clear(&self.list);
            self.expanded.borrow_mut().clear();
        }
        self.message.set_text("Notifications Unavailable");
        self.empty.set_visible(true);
        self.scroll.set_visible(false);
        self.clear.set_visible(false);
        self.dnd.set_sensitive(false);
    }

    fn update(self: &Rc<Self>, snapshot: Snapshot) {
        let current = self.snapshot.borrow();
        if current.as_ref() == Some(&snapshot) {
            return;
        }
        let changed = current
            .as_ref()
            .is_none_or(|previous| previous.notifications != snapshot.notifications);
        drop(current);
        if snapshot.dnd {
            self.dnd.add_css_class("active");
        } else {
            self.dnd.remove_css_class("active");
        }
        self.dnd_image.set_icon_name(Some(if snapshot.dnd {
            "notifications-disabled-symbolic"
        } else {
            "preferences-system-notifications-symbolic"
        }));
        self.dnd.set_sensitive(!self.busy.get());
        self.snapshot.replace(Some(snapshot));
        if changed {
            self.render();
        }
    }

    fn render(self: &Rc<Self>) {
        super::clear(&self.list);
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            return;
        };
        let groups = sections(&snapshot.notifications);
        self.expanded.borrow_mut().retain(|key| {
            groups
                .iter()
                .any(|(group, items)| group == key && items.len() > 1)
        });
        let empty = groups.is_empty();
        self.message.set_text("No Notifications");
        self.clear.set_visible(!empty);
        self.empty.set_visible(empty);
        self.scroll.set_visible(!empty);
        for (key, notifications) in groups {
            let latest = notifications[0];
            let group = gtk::Box::new(gtk::Orientation::Vertical, 6);
            group.add_css_class("date-notification-group");
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            header.append(&app_icon(latest));
            let name = if latest.app_name.is_empty() {
                "System"
            } else {
                &latest.app_name
            };
            let name = super::label(name, "date-notification-name");
            name.set_hexpand(true);
            header.append(&name);
            let expanded = self.expanded.borrow().contains(&key);
            if notifications.len() > 1 {
                let icon = if expanded {
                    "pan-up-symbolic"
                } else {
                    "pan-down-symbolic"
                };
                let tooltip = if expanded {
                    "Collapse Notifications"
                } else {
                    "Expand Notifications"
                };
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                content.append(&gtk::Label::new(Some(&notifications.len().to_string())));
                content.append(&gtk::Image::from_icon_name(icon));
                let toggle = gtk::Button::builder()
                    .child(&content)
                    .tooltip_text(tooltip)
                    .build();
                toggle.add_css_class("date-button");
                let weak = Rc::downgrade(self);
                toggle.connect_clicked(move |_| {
                    if let Some(feed) = weak.upgrade() {
                        let mut expanded = feed.expanded.borrow_mut();
                        if !expanded.remove(&key) {
                            expanded.insert(key.clone());
                        }
                        drop(expanded);
                        feed.render();
                    }
                });
                header.append(&toggle);
            }
            group.append(&header);
            group.append(&self.row(latest));
            if expanded {
                for notification in &notifications[1..] {
                    group.append(&self.row(notification));
                }
            }
            self.list.append(&group);
        }
    }
}

async fn read() -> Option<Snapshot> {
    let (output,) = request("Snapshot", None).await.ok()?.get::<(String,)>()?;
    serde_json::from_str(&output).ok()
}

pub(super) fn section(parent: &gtk::Box, window: &gtk::ApplicationWindow) -> glib::JoinHandle<()> {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 12);
    section.set_vexpand(true);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = super::label("Notifications", "date-section-title");
    title.set_hexpand(true);
    header.append(&title);
    let dnd_image = gtk::Image::from_icon_name("preferences-system-notifications-symbolic");
    let dnd_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    dnd_content.append(&dnd_image);
    dnd_content.append(&gtk::Label::new(Some("Do Not Disturb")));
    let dnd = gtk::Button::builder()
        .child(&dnd_content)
        .sensitive(false)
        .build();
    dnd.add_css_class("date-button");
    header.append(&dnd);
    let clear = gtk::Button::builder().label("Clear").visible(false).build();
    clear.add_css_class("date-button");
    header.append(&clear);
    section.append(&header);

    let empty = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .vexpand(true)
        .valign(gtk::Align::Center)
        .build();
    empty.add_css_class("date-muted");
    empty.append(&gtk::Image::from_icon_name(
        "preferences-system-notifications-symbolic",
    ));
    let message = gtk::Label::new(Some("Loading Notifications…"));
    empty.append(&message);
    section.append(&empty);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let scroll = super::scroll(&list, 440);
    scroll.set_vexpand(true);
    scroll.set_visible(false);
    section.append(&scroll);
    parent.append(&section);

    let feed = Rc::new(Feed {
        list,
        empty,
        message,
        scroll,
        dnd,
        dnd_image,
        clear,
        snapshot: RefCell::default(),
        expanded: RefCell::default(),
        busy: Cell::default(),
        revision: Cell::default(),
        pending: RefCell::default(),
    });
    for (button, action) in [(&feed.dnd, "ToggleDnd"), (&feed.clear, "Clear")] {
        let weak = Rc::downgrade(&feed);
        button.connect_clicked(move |_| {
            if let Some(feed) = weak.upgrade() {
                feed.act(action, None);
            }
        });
    }
    let weak = Rc::downgrade(&feed);
    window.connect_unrealize(move |_| {
        if let Some(feed) = weak.upgrade() {
            if let Some(handle) = feed.pending.take() {
                handle.abort();
            }
        }
    });
    glib::spawn_future_local(async move {
        let _ = crate::quick_settings::command(&["palobar-ctl", "close-popup"]).await;
        loop {
            if !feed.busy.get() {
                let revision = feed.revision.get();
                let snapshot = read().await;
                if !feed.busy.get() && revision == feed.revision.get() {
                    match snapshot {
                        Some(snapshot) => feed.update(snapshot),
                        None => feed.unavailable(),
                    }
                }
            }
            glib::timeout_future(Duration::from_secs(1)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{sections, Snapshot};

    #[test]
    fn groups_by_desktop_id_and_orders_latest_notifications_first() {
        let snapshot: Snapshot = serde_json::from_str(r#"{
            "dnd": false,
            "notifications": [
                {"id":1,"time":10,"app_name":"First","desktop_entry":"app.desktop","app_icon":"","summary":"","body":""},
                {"id":2,"time":11,"app_name":"Other","desktop_entry":"","app_icon":"","summary":"","body":""},
                {"id":3,"time":12,"app_name":"Renamed","desktop_entry":"app","app_icon":"","summary":"","body":""}
            ]
        }"#).unwrap();
        let grouped = sections(&snapshot.notifications);
        assert_eq!(grouped[0].0, "desktop:app");
        assert_eq!(
            grouped[0].1.iter().map(|item| item.id).collect::<Vec<_>>(),
            [3, 1]
        );
        assert_eq!(grouped[1].0, "name:other");
    }
}
