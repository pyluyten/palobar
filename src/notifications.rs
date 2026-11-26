use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const BUS_NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const PRIVATE: &str = "io.Astal.Palobar.Notifications";
const PRIVATE_PATH: &str = "/io/Astal/Palobar/Notifications";
const HISTORY_LIMIT: usize = 500;
const DISMISSED: u32 = 2;
const CLOSED: u32 = 3;
const EXPIRED: u32 = 1;
type Hints = HashMap<String, glib::Variant>;
type Notify = (String, u32, String, String, String, Vec<String>, Hints, i32);

const INTERFACES: &str = r#"<node>
<interface name="org.freedesktop.Notifications">
 <method name="GetCapabilities"><arg type="as" direction="out"/></method>
 <method name="Notify">
  <arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/>
  <arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/>
  <arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/>
 </method>
 <method name="CloseNotification"><arg type="u" direction="in"/></method>
 <method name="GetServerInformation">
  <arg type="s" direction="out"/><arg type="s" direction="out"/>
  <arg type="s" direction="out"/><arg type="s" direction="out"/>
 </method>
 <signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
</interface>
<interface name="io.Astal.Palobar.Notifications">
 <method name="Snapshot"><arg type="s" direction="out"/></method>
 <method name="GetUnread"><arg type="u" direction="out"/></method>
 <method name="GetDnd"><arg type="b" direction="out"/></method>
 <method name="ToggleDnd"><arg type="b" direction="out"/></method>
 <method name="Dismiss"><arg type="u" direction="in"/></method>
 <method name="Clear"/>
</interface></node>"#;

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub time: i64,
    pub app_name: String,
    pub desktop_entry: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub dnd: bool,
    pub notifications: Vec<Notification>,
}

#[derive(Deserialize, Serialize)]
struct Entry {
    notification: Notification,
    expires: Option<i64>,
    #[serde(skip)]
    transient: bool,
}

#[derive(Default, Deserialize, Serialize)]
struct History {
    entries: Vec<Entry>,
    next_id: u32,
    dnd: bool,
}

impl History {
    fn next_id(&mut self, replacement: u32) -> u32 {
        if replacement != 0
            && self
                .entries
                .iter()
                .any(|entry| entry.notification.id == replacement)
        {
            return replacement;
        }
        loop {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self
                .entries
                .iter()
                .any(|entry| entry.notification.id == self.next_id)
            {
                return self.next_id;
            }
        }
    }
}

#[derive(Default)]
struct State {
    history: History,
    unread: HashSet<u32>,
    toasts: Vec<(u32, i64)>,
    dirty: bool,
}

struct Toasts {
    window: gtk::ApplicationWindow,
    list: gtk::Box,
}

pub struct Service {
    app: glib::WeakRef<gtk::Application>,
    connection: gio::DBusConnection,
    state: RefCell<State>,
    path: PathBuf,
    badges: RefCell<Vec<glib::WeakRef<gtk::Label>>>,
    toasts: RefCell<Option<Toasts>>,
    registration: RefCell<Option<(gio::RegistrationId, gio::RegistrationId)>>,
    timer: RefCell<Option<glib::SourceId>>,
    owns_name: Cell<bool>,
}

fn now() -> i64 {
    glib::real_time() / 1000
}

fn limit(mut text: String, len: usize) -> String {
    if text.len() > len {
        let mut end = len;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

impl Service {
    pub(crate) fn is_enabled(&self) -> bool {
        self.owns_name.get()
    }

    pub(crate) fn set_enabled(self: &Rc<Self>, enabled: bool) -> Result<(), String> {
        if enabled == self.owns_name.get() {
            return Ok(());
        }
        if enabled {
            self.enable()
        } else {
            self.disable();
            Ok(())
        }
    }

    fn enable(self: &Rc<Self>) -> Result<(), String> {
        let connection = &self.connection;
        let node = gio::DBusNodeInfo::for_xml(INTERFACES).map_err(|error| error.to_string())?;
        let public = node
            .lookup_interface(BUS_NAME)
            .ok_or("Missing notification interface")?;
        let private = node
            .lookup_interface(PRIVATE)
            .ok_or("Missing Palobar notification interface")?;
        let weak = Rc::downgrade(self);
        let public_id = connection
            .register_object(PATH, &public)
            .method_call(move |_, _, _, _, method, parameters, call| {
                if let Some(service) = weak.upgrade() {
                    service.public(method, parameters, call);
                }
            })
            .build()
            .map_err(|error| error.to_string())?;
        let weak = Rc::downgrade(self);
        let private_id = match connection
            .register_object(PRIVATE_PATH, &private)
            .method_call(move |_, _, _, _, method, parameters, call| {
                if let Some(service) = weak.upgrade() {
                    service.private(method, parameters, call);
                }
            })
            .build()
        {
            Ok(id) => id,
            Err(error) => {
                let _ = connection.unregister_object(public_id);
                return Err(error.to_string());
            }
        };
        let result = connection.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(BUS_NAME, 4u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        );
        if !result
            .as_ref()
            .ok()
            .and_then(glib::Variant::get::<(u32,)>)
            .is_some_and(|(code,)| matches!(code, 1 | 4))
        {
            let _ = connection.unregister_object(public_id);
            let _ = connection.unregister_object(private_id);
            return Err(result
                .err()
                .map(|error| error.to_string())
                .unwrap_or_else(|| {
                    "Another application already owns org.freedesktop.Notifications".into()
                }));
        }
        self.registration.replace(Some((public_id, private_id)));
        self.owns_name.set(true);
        let ticking = self.clone();
        self.timer.replace(Some(glib::timeout_add_local(
            Duration::from_millis(250),
            move || {
                ticking.tick();
                glib::ControlFlow::Continue
            },
        )));
        Ok(())
    }

    fn disable(self: &Rc<Self>) {
        self.persist();
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        if let Some((public, private)) = self.registration.take() {
            let _ = self.connection.unregister_object(public);
            let _ = self.connection.unregister_object(private);
        }
        if self.owns_name.replace(false) {
            let _ = self.connection.call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "ReleaseName",
                Some(&(BUS_NAME,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            );
        }
        self.state.borrow_mut().toasts.clear();
        self.render_toasts();
    }

    pub(crate) fn unread_count(&self) -> u32 {
        self.state.borrow().unread.len() as u32
    }
    fn update_badges(&self) {
        let count = self.state.borrow().unread.len();
        let text = count.to_string();
        self.badges.borrow_mut().retain(|badge| {
            let Some(badge) = badge.upgrade() else {
                return false;
            };
            badge.set_visible(count != 0);
            badge.set_text(&text);
            true
        });
    }

    pub fn add_badge(&self, badge: &gtk::Label) {
        self.badges.borrow_mut().push(badge.downgrade());
        self.update_badges();
    }

    fn mark_read(&self) {
        if !self.state.borrow().unread.is_empty() {
            self.state.borrow_mut().unread.clear();
            self.update_badges();
        }
    }

    fn notify(
        self: &Rc<Self>,
        (name, replaces, icon, summary, body, _, hints, timeout): Notify,
    ) -> u32 {
        let time = now();
        let mut state = self.state.borrow_mut();
        let id = state.history.next_id(replaces);
        state
            .history
            .entries
            .retain(|entry| entry.notification.id != id);
        let notification = Notification {
            id,
            time: time / 1000,
            app_name: limit(name, 512),
            desktop_entry: limit(
                hints
                    .get("desktop-entry")
                    .and_then(glib::Variant::get)
                    .unwrap_or_default(),
                512,
            ),
            app_icon: limit(icon, 1024),
            summary: limit(summary, 2048),
            body: limit(body, 8192),
        };
        let transient = hints
            .get("transient")
            .and_then(glib::Variant::get::<bool>)
            .unwrap_or(false);
        state.history.entries.push(Entry {
            notification,
            expires: (timeout > 0).then_some(time + i64::from(timeout)),
            transient,
        });
        state.dirty = true;
        let overflow = (state.history.entries.len() > HISTORY_LIMIT)
            .then(|| state.history.entries[0].notification.id);
        if !self
            .app
            .upgrade()
            .is_some_and(|app| crate::calendar::is_open(&app))
        {
            state.unread.insert(id);
        }
        let toast = !state.history.dnd;
        if toast {
            state.toasts.retain(|(key, _)| *key != id);
            state
                .toasts
                .insert(0, (id, glib::monotonic_time() + 5_000_000));
            state.toasts.truncate(3);
        }
        drop(state);
        if let Some(old) = overflow {
            self.close(&[old], EXPIRED);
        } else {
            self.update_badges();
            if toast {
                self.render_toasts();
            }
        }
        id
    }

    fn close(self: &Rc<Self>, ids: &[u32], reason: u32) -> bool {
        let mut state = self.state.borrow_mut();
        let mut removed = Vec::new();
        state.history.entries.retain(|entry| {
            if ids.contains(&entry.notification.id) {
                removed.push(entry.notification.id);
                false
            } else {
                true
            }
        });
        if removed.is_empty() {
            return false;
        }
        state.dirty = true;
        state.unread.retain(|id| !ids.contains(id));
        state.toasts.retain(|(id, _)| !ids.contains(id));
        drop(state);
        for id in removed {
            let _ = self.connection.emit_signal(
                None,
                PATH,
                BUS_NAME,
                "NotificationClosed",
                Some(&(id, reason).to_variant()),
            );
        }
        self.update_badges();
        self.render_toasts();
        true
    }

    fn hide_toast(self: &Rc<Self>, id: u32) {
        self.state.borrow_mut().toasts.retain(|(key, _)| *key != id);
        self.render_toasts();
    }

    fn render_toasts(self: &Rc<Self>) {
        let visible: Vec<_> = {
            let state = self.state.borrow();
            state
                .toasts
                .iter()
                .filter_map(|(id, _)| {
                    state
                        .history
                        .entries
                        .iter()
                        .find(|entry| entry.notification.id == *id)
                        .map(|entry| entry.notification.clone())
                })
                .collect()
        };
        if visible.is_empty() {
            if let Some(toasts) = self.toasts.borrow_mut().take() {
                toasts.window.close();
            }
            return;
        }
        let mut toasts = self.toasts.borrow_mut();
        if toasts.is_none() {
            let Some(app) = self.app.upgrade() else {
                return;
            };
            let Some(monitor) = gdk::Display::default()
                .and_then(|display| display.monitors().item(0))
                .and_downcast::<gdk::Monitor>()
            else {
                return;
            };
            let window = gtk::ApplicationWindow::builder()
                .application(&app)
                .title("Notifications")
                .build();
            window.add_css_class("notification-toast-window");
            window.init_layer_shell();
            window.set_namespace(Some("notification-toasts"));
            window.set_monitor(Some(&monitor));
            window.set_anchor(Edge::Top, true);
            window.set_margin(Edge::Top, 8);
            window.set_layer(Layer::Top);
            window.set_keyboard_mode(KeyboardMode::None);
            let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
            window.set_child(Some(&list));
            *toasts = Some(Toasts { window, list });
        }
        let Some(toasts) = toasts.as_ref() else {
            return;
        };
        while let Some(child) = toasts.list.first_child() {
            toasts.list.remove(&child);
        }
        for item in visible {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
            card.add_css_class("notification-toast-card");
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            header.append(&app_icon(&item));
            let title = gtk::Label::builder()
                .label(&item.summary)
                .hexpand(true)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(38)
                .build();
            title.add_css_class("notification-toast-title");
            header.append(&title);
            let dismiss = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text("Hide Notification")
                .build();
            dismiss.add_css_class("notification-toast-dismiss");
            let weak = Rc::downgrade(self);
            dismiss.connect_clicked(move |_| {
                if let Some(service) = weak.upgrade() {
                    service.hide_toast(item.id);
                }
            });
            header.append(&dismiss);
            card.append(&header);
            if !item.body.is_empty() {
                card.append(
                    &gtk::Label::builder()
                        .label(&item.body)
                        .xalign(0.0)
                        .wrap(true)
                        .wrap_mode(gtk::pango::WrapMode::WordChar)
                        .max_width_chars(42)
                        .build(),
                );
            }
            toasts.list.append(&card);
        }
        toasts.window.present();
    }

    fn persist(&self) {
        let mut state = self.state.borrow_mut();
        if !std::mem::take(&mut state.dirty) {
            return;
        }
        #[derive(Serialize)]
        struct Saved<'a> {
            entries: Vec<&'a Entry>,
            next_id: u32,
            dnd: bool,
        }
        let saved = Saved {
            entries: state
                .history
                .entries
                .iter()
                .filter(|entry| !entry.transient)
                .collect(),
            next_id: state.history.next_id,
            dnd: state.history.dnd,
        };
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            if let Some(directory) = self.path.parent() {
                std::fs::create_dir_all(directory)?;
            }
            gio::File::for_path(&self.path).replace_contents(
                &serde_json::to_vec(&saved)?,
                None,
                false,
                gio::FileCreateFlags::PRIVATE,
                gio::Cancellable::NONE,
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("[notifications] Could not save notification history: {error}");
        }
    }

    fn tick(self: &Rc<Self>) {
        let time = now();
        let expired: Vec<_> = self
            .state
            .borrow()
            .history
            .entries
            .iter()
            .filter(|entry| entry.expires.is_some_and(|deadline| time >= deadline))
            .map(|entry| entry.notification.id)
            .collect();
        if !expired.is_empty() {
            self.close(&expired, EXPIRED);
        }
        let changed = {
            let mut state = self.state.borrow_mut();
            let length = state.toasts.len();
            let time = glib::monotonic_time();
            state.toasts.retain(|(_, deadline)| time < *deadline);
            state.toasts.len() != length
        };
        if changed {
            self.render_toasts();
        }
        self.persist();
    }

    fn public(
        self: &Rc<Self>,
        method: &str,
        parameters: glib::Variant,
        call: gio::DBusMethodInvocation,
    ) {
        match method {
            "GetCapabilities" => {
                call.return_value(Some(&(vec!["body", "persistence"],).to_variant()))
            }
            "GetServerInformation" => call.return_value(Some(
                &(
                    "Palobar",
                    "Palobar",
                    include_str!("../VERSION").trim(),
                    "1.3",
                )
                    .to_variant(),
            )),
            "Notify" => {
                let Some(data) = parameters.get::<Notify>() else {
                    call.return_dbus_error(
                        "org.freedesktop.DBus.Error.InvalidArgs",
                        "Invalid notification",
                    );
                    return;
                };
                call.return_value(Some(&(self.notify(data),).to_variant()));
            }
            "CloseNotification" => {
                if parameters
                    .get::<(u32,)>()
                    .is_some_and(|(id,)| self.close(&[id], CLOSED))
                {
                    call.return_value(None);
                } else {
                    call.return_dbus_error(
                        "org.freedesktop.Notifications.Error.InvalidNotification",
                        "Notification does not exist",
                    );
                }
            }
            _ => call.return_dbus_error(
                "org.freedesktop.DBus.Error.UnknownMethod",
                "Unknown notification method",
            ),
        }
    }

    fn private(
        self: &Rc<Self>,
        method: &str,
        parameters: glib::Variant,
        call: gio::DBusMethodInvocation,
    ) {
        match method {
            "Snapshot" => {
                self.mark_read();
                let state = self.state.borrow();
                let snapshot = Snapshot {
                    dnd: state.history.dnd,
                    notifications: state
                        .history
                        .entries
                        .iter()
                        .map(|entry| entry.notification.clone())
                        .collect(),
                };
                match serde_json::to_string(&snapshot) {
                    Ok(data) => call.return_value(Some(&(data,).to_variant())),
                    Err(error) => call
                        .return_dbus_error("org.freedesktop.DBus.Error.Failed", &error.to_string()),
                }
            }
            "GetUnread" => call.return_value(Some(
                &(self.state.borrow().unread.len() as u32,).to_variant(),
            )),
            "GetDnd" => call.return_value(Some(&(self.state.borrow().history.dnd,).to_variant())),
            "ToggleDnd" => {
                let mut state = self.state.borrow_mut();
                state.history.dnd = !state.history.dnd;
                state.dirty = true;
                let enabled = state.history.dnd;
                if enabled {
                    state.toasts.clear();
                }
                drop(state);
                self.render_toasts();
                call.return_value(Some(&(enabled,).to_variant()));
            }
            "Dismiss" => {
                if let Some((id,)) = parameters.get::<(u32,)>() {
                    self.close(&[id], DISMISSED);
                }
                call.return_value(None);
            }
            "Clear" => {
                let ids: Vec<_> = self
                    .state
                    .borrow()
                    .history
                    .entries
                    .iter()
                    .map(|entry| entry.notification.id)
                    .collect();
                self.close(&ids, DISMISSED);
                call.return_value(None);
            }
            _ => call.return_dbus_error(
                "org.freedesktop.DBus.Error.UnknownMethod",
                "Unknown Palobar notification method",
            ),
        }
    }
}

pub fn app_icon(notification: &Notification) -> gtk::Image {
    let id = notification.desktop_entry.trim();
    let desktop = id.strip_suffix(".desktop").unwrap_or(id);
    let desktop_icon = (!desktop.is_empty())
        .then(|| gio::DesktopAppInfo::new(&format!("{desktop}.desktop")))
        .flatten()
        .and_then(|info| info.icon());
    let image = if let Some(icon) = desktop_icon {
        gtk::Image::from_gicon(&icon)
    } else if notification.app_icon.starts_with('/') || notification.app_icon.starts_with("file://")
    {
        let icon = gio::FileIcon::new(&gio::File::for_commandline_arg(&notification.app_icon));
        gtk::Image::from_gicon(&icon)
    } else {
        let found = gdk::Display::default().is_some_and(|display| {
            gtk::IconTheme::for_display(&display).has_icon(&notification.app_icon)
        });
        gtk::Image::from_icon_name(if found {
            &notification.app_icon
        } else {
            "dialog-information-symbolic"
        })
    };
    image.set_pixel_size(24);
    image
}

pub fn start(app: &gtk::Application, display: &gdk::Display) -> Result<Rc<Service>, String> {
    let connection = app.dbus_connection().ok_or("No session D-Bus connection")?;
    let path = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| glib::home_dir().join(".local/state"))
        .join("palobar/notifications.json");
    let mut history: History = std::fs::read(&path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default();
    history
        .entries
        .retain(|entry| entry.expires.is_none_or(|expiry| expiry > now()));
    if history.entries.len() > HISTORY_LIMIT {
        history
            .entries
            .drain(..history.entries.len() - HISTORY_LIMIT);
    }
    let unread = history
        .entries
        .iter()
        .map(|entry| entry.notification.id)
        .collect();
    let service = Rc::new(Service {
        app: app.downgrade(),
        connection: connection.clone(),
        state: RefCell::new(State {
            history,
            unread,
            ..State::default()
        }),
        path,
        badges: RefCell::default(),
        toasts: RefCell::default(),
        registration: RefCell::default(),
        timer: RefCell::default(),
        owns_name: Cell::default(),
    });
    let css = gtk::CssProvider::new();
    css.load_from_data(
        "window.notification-toast-window { background: transparent; }
        .notification-toast-card { background: #333; color: #f6f5f4; min-width: 330px;
        border-radius: 18px; border: 1px solid rgba(255,255,255,0.12); padding: 12px; }
        .notification-toast-title { font-weight: 700; }
        .notification-toast-card button.notification-toast-dismiss {
            background: transparent; border: none; border-radius: 999px;
            box-shadow: none; padding: 2px 4px; color: rgba(246, 245, 244, 0.65);
        }
        .notification-toast-card button.notification-toast-dismiss:hover,
        .notification-toast-card button.notification-toast-dismiss:active {
            background-color: rgba(255, 255, 255, 0.12); color: #f6f5f4;
        }",
    );
    gtk::style_context_add_provider_for_display(
        display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    if let Err(error) = service.set_enabled(true) {
        eprintln!("[notifications] {error}");
    }
    let shutdown = service.clone();
    app.connect_shutdown(move |_| {
        shutdown.disable();
    });
    Ok(service)
}

pub async fn request(
    method: &str,
    parameters: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    let connection = gio::bus_get_future(gio::BusType::Session).await?;
    connection
        .call_future(
            Some(crate::bar::APP_ID),
            PRIVATE_PATH,
            PRIVATE,
            method,
            parameters,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
        )
        .await
}

pub async fn dnd(toggle: bool) -> Option<bool> {
    request(if toggle { "ToggleDnd" } else { "GetDnd" }, None)
        .await
        .ok()?
        .get::<(bool,)>()
        .map(|(dnd,)| dnd)
}

#[cfg(test)]
mod tests {
    use super::{limit, History};

    #[test]
    fn notification_ids_never_use_zero_even_after_wrapping() {
        let mut history = History {
            next_id: u32::MAX,
            ..History::default()
        };
        assert_eq!(history.next_id(0), 1);
        assert_eq!(history.next_id(0), 2);
    }

    #[test]
    fn truncation_preserves_utf8() {
        assert_eq!(limit("123é".into(), 4), "123");
    }
}
