use super::label;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::collections::BTreeSet;
use std::future::Future;
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;

fn status<F, Check>(owner: &gtk::Box, check: Check) -> gtk::Label
where
    F: Future<Output = String> + 'static,
    Check: Fn() -> F + 'static,
{
    let status = label("Checking…", "prefs-muted");
    status.set_selectable(true);
    let weak = status.downgrade();
    let running = Rc::new(Cell::new(false));
    owner.connect_map(move |_| {
        if running.replace(true) {
            return;
        }
        if let Some(status) = weak.upgrade() {
            status.set_text("Checking…");
        }
        let (weak, running, result) = (weak.clone(), running.clone(), check());
        glib::spawn_future_local(async move {
            let result = result.await;
            running.set(false);
            if let Some(status) = weak.upgrade() {
                status.set_text(&result);
            }
        });
    });
    status
}

fn page<F, Check>(heading: &str, check: Check) -> gtk::ScrolledWindow
where
    F: Future<Output = String> + 'static,
    Check: Fn() -> F + 'static,
{
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.add_css_class("prefs-page-body");
    body.append(&label(heading, "prefs-group-title"));
    body.append(&status(&body, check));
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&body)
        .build()
}

async fn session_bus_names(method: &str) -> Result<Vec<String>, glib::Error> {
    let bus = gio::bus_get_future(gio::BusType::Session).await?;
    let reply = bus
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            None,
            Some(glib::VariantTy::new("(as)").unwrap()),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
        )
        .await?;
    Ok(reply.child_get(0))
}

pub(super) fn applications() -> gtk::ScrolledWindow {
    page("Desktop Portals", || async {
        let (Ok(running), Ok(activatable)) = (
            session_bus_names("ListNames").await,
            session_bus_names("ListActivatableNames").await,
        ) else {
            return "Could not check desktop portals on the session bus.".into();
        };
        portal_status(&running, &activatable)
    })
}

fn is_portal_name(name: &str) -> bool {
    name.starts_with("org.freedesktop.portal.") || name.starts_with("org.freedesktop.impl.portal.")
}

fn portal_status(running: &[String], activatable: &[String]) -> String {
    let mut running: Vec<_> = running.iter().filter(|name| is_portal_name(name)).collect();
    let mut available: Vec<_> = activatable
        .iter()
        .filter(|name| is_portal_name(name))
        .filter(|name| !running.contains(name))
        .collect();
    running.sort_unstable();
    available.sort_unstable();
    if running.is_empty() && available.is_empty() {
        return "No desktop portal services are running or available to activate.".into();
    }
    let mut sections = Vec::new();
    if !running.is_empty() {
        sections.push(format!(
            "Running\n{}",
            running
                .iter()
                .map(|name| portal_entry(name, "Running"))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
    if !available.is_empty() {
        sections.push(format!(
            "Available to start\nThese services are installed and D-Bus can activate them on demand.\n\n{}",
            available
                .iter()
                .map(|name| portal_entry(name, "Available; not running"))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
    sections.join("\n\n")
}

fn portal_entry(name: &str, state: &str) -> String {
    format!(
        "{} · {state}\n{}\n{}",
        portal_title(name),
        name,
        portal_description(name)
    )
}

fn portal_title(name: &str) -> &'static str {
    match name {
        "org.freedesktop.portal.Desktop" => "Desktop Portal",
        "org.freedesktop.portal.Documents" => "Documents Portal",
        "org.freedesktop.impl.portal.desktop.gnome" => "GNOME Portal Backend",
        "org.freedesktop.impl.portal.desktop.gtk" => "GTK Portal Backend",
        "org.freedesktop.impl.portal.desktop.kwallet" => "KDE Wallet Portal Backend",
        "org.freedesktop.impl.portal.PermissionStore" => "Portal Permission Store",
        "org.freedesktop.portal.IBus" => "IBus Portal",
        "org.freedesktop.portal.Tracker" => "Tracker Portal",
        _ => "Desktop Portal Service",
    }
}

fn portal_description(name: &str) -> &'static str {
    match name {
        "org.freedesktop.portal.Desktop" => {
            "Provides sandboxed applications with desktop integration portals."
        }
        "org.freedesktop.portal.Documents" => {
            "Shares selected files and folders with sandboxed applications."
        }
        "org.freedesktop.impl.portal.desktop.gnome" => {
            "GNOME backend for file chooser, settings, screenshots, and other portals."
        }
        "org.freedesktop.impl.portal.desktop.gtk" => {
            "GTK backend, commonly used for file chooser and application dialogs."
        }
        "org.freedesktop.impl.portal.desktop.kwallet" => {
            "KDE Wallet backend for portal-based secret storage."
        }
        "org.freedesktop.impl.portal.PermissionStore" => {
            "Stores per-application portal permissions."
        }
        "org.freedesktop.portal.IBus" => "Provides input method integration through IBus.",
        "org.freedesktop.portal.Tracker" => {
            "Provides Tracker search integration to sandboxed applications."
        }
        _ => "A desktop portal service currently owns this session bus name.",
    }
}

pub(super) fn notifications() -> gtk::ScrolledWindow {
    eprintln!(
        "[prefs notifications] building page with executable {}",
        std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| format!("<unknown: {error}>"))
    );
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.add_css_class("prefs-page-body");
    body.append(&label("Notification Service", "prefs-group-title"));
    let status = label("Checking notification service…", "prefs-muted");
    status.set_selectable(true);
    body.append(&status);

    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("prefs-row");
    row.add_css_class("last");
    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.append(&label("Palobar Notifications", "prefs-row-title"));
    info.append(&label(
        "Receive notifications in Palobar and show them in Calendar",
        "prefs-muted",
    ));
    row.append(&info);
    let toggle = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    toggle.add_css_class("prefs-notification-switch");
    toggle.update_property(&[gtk::accessible::Property::Label("Palobar Notifications")]);
    row.append(&toggle);
    group.append(&row);
    body.append(&group);

    let controls = Rc::new(NotificationControls {
        status,
        toggle,
        syncing: Cell::new(false),
        busy: Cell::new(false),
    });
    controls.toggle.connect_state_set({
        let controls = Rc::downgrade(&controls);
        move |toggle, enabled| {
            let Some(controls) = controls.upgrade() else {
                return glib::Propagation::Stop;
            };
            if controls.syncing.get() || controls.busy.replace(true) {
                return glib::Propagation::Stop;
            }
            toggle.set_sensitive(false);
            let controls = controls.clone();
            glib::spawn_future_local(async move {
                let result = set_palobar_notifications(enabled).await;
                controls.busy.set(false);
                if let Err(error) = result {
                    controls
                        .status
                        .set_text(&format!("Could not change Palobar notifications. {error}"));
                }
                controls.refresh().await;
            });
            glib::Propagation::Stop
        }
    });
    body.connect_map({
        let controls = controls.clone();
        move |_| {
            eprintln!("[prefs notifications] page mapped");
            let controls = controls.clone();
            glib::spawn_future_local(async move {
                controls.refresh().await;
            });
        }
    });
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&body)
        .build()
}

struct NotificationControls {
    status: gtk::Label,
    toggle: gtk::Switch,
    syncing: Cell<bool>,
    busy: Cell<bool>,
}

impl NotificationControls {
    async fn refresh(&self) {
        eprintln!("[prefs notifications] refresh started");
        let service = notification_service().await;
        eprintln!("[prefs notifications] standard service lookup: {service:?}");
        let palobar = palobar_notification_state().await;
        eprintln!("[prefs notifications] Palobar control lookup: {palobar:?}");
        let (available, enabled) = palobar.as_ref().copied().unwrap_or((false, false));
        let message = if let Err(error) = &palobar {
            format!("Could not read Palobar notification state. {error}")
        } else {
            match (&service, available, enabled) {
                (Ok(Some(owner)), true, true) => {
                    format!("Palobar is the active notification service. {owner}")
                }
                (Ok(Some(owner)), _, _) => format!(
                    "Active service: {owner}. Palobar notifications are inactive because another service owns org.freedesktop.Notifications."
                ),
                (Ok(None), true, false) => {
                    "No notification server is active. Palobar is available and can be enabled here.".into()
                }
                (Ok(None), true, true) => {
                    "Palobar reports notifications enabled, but no server currently owns the notification service name.".into()
                }
                (Ok(None), false, _) => "No notification service is running. Start the Palobar session to enable Palobar notifications.".into(),
                (Err(error), _, _) => format!("Could not identify the notification service. {error}"),
            }
        };
        eprintln!(
            "[prefs notifications] rendering: available={available}, enabled={enabled}, message={message:?}"
        );
        self.status.set_text(&message);
        self.syncing.set(true);
        self.toggle.set_active(enabled);
        self.toggle.set_sensitive(
            !self.busy.get() && available && (enabled || matches!(service, Ok(None))),
        );
        self.syncing.set(false);
    }
}

async fn notification_service() -> Result<Option<String>, String> {
    const SERVICE: &str = "org.freedesktop.Notifications";
    eprintln!("[prefs notifications] connecting to session bus for owner lookup");
    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|error| error.to_string())?;
    let owner = match connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            Some(&(SERVICE,).to_variant()),
            Some(glib::VariantTy::new("(s)").unwrap()),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
        )
        .await
    {
        Ok(reply) => reply
            .get::<(String,)>()
            .map(|(owner,)| owner)
            .ok_or_else(|| "The session bus returned an invalid owner name.".to_owned())?,
        Err(error) if error.matches(gio::DBusError::NameHasNoOwner) => {
            eprintln!("[prefs notifications] no owner for {SERVICE}");
            return Ok(None);
        }
        Err(error) => {
            eprintln!("[prefs notifications] GetNameOwner failed: {error}");
            return Err(error.to_string());
        }
    };
    eprintln!("[prefs notifications] {SERVICE} owner is {owner}");
    let reply = connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            Some(&(owner.as_str(),).to_variant()),
            Some(glib::VariantTy::new("(u)").unwrap()),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
        )
        .await
        .map_err(|error| error.to_string())?;
    let pid = reply
        .get::<(u32,)>()
        .map(|(pid,)| pid)
        .ok_or_else(|| "The session bus returned an invalid process ID.".to_owned())?;
    eprintln!("[prefs notifications] owner PID is {pid}");
    let executable = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    let process_name = executable
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| {
            std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .map(|name| name.trim().to_owned())
        });
    Ok(Some(match process_name {
        Some(name) => format!("{name} (PID {pid}; D-Bus owner {owner})"),
        None => format!("D-Bus owner {owner} (PID {pid})"),
    }))
}

async fn palobar_notification_state() -> Result<(bool, bool), String> {
    eprintln!("[prefs notifications] calling Palobar GetNotificationState");
    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|error| error.to_string())?;
    let reply = connection
        .call_future(
            Some(crate::bar::APP_ID),
            "/io/Astal/Palobar/Control",
            "io.Astal.Palobar.Control",
            "GetNotificationState",
            None,
            Some(glib::VariantTy::new("(bb)").unwrap()),
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
        )
        .await
        .map_err(|error| error.to_string())?;
    reply
        .get::<(bool, bool)>()
        .ok_or_else(|| "Palobar returned an invalid notification state.".to_owned())
}

async fn set_palobar_notifications(enabled: bool) -> Result<(), String> {
    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|error| error.to_string())?;
    let reply = connection
        .call_future(
            Some(crate::bar::APP_ID),
            "/io/Astal/Palobar/Control",
            "io.Astal.Palobar.Control",
            "SetNotificationsEnabled",
            Some(&(enabled,).to_variant()),
            Some(glib::VariantTy::new("(b)").unwrap()),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
        )
        .await
        .map_err(|error| error.to_string())?;
    match reply.get::<(bool,)>() {
        Some((actual,)) if actual == enabled => Ok(()),
        _ => Err("Palobar did not confirm the requested state.".into()),
    }
}

pub(super) fn authentication(owner: &gtk::Box) -> gtk::Label {
    status(owner, || async {
        match gio::spawn_blocking(authentication_agents).await {
            Ok(Ok(agents)) if !agents.is_empty() => {
                format!("Running PolicyKit agents detected\n{}", agents.join("\n"))
            }
            Ok(Ok(_)) => "No known PolicyKit authentication agent detected for this user.".into(),
            _ => "Could not check for running PolicyKit authentication agents.".into(),
        }
    })
}

fn authentication_agents() -> std::io::Result<Vec<String>> {
    // PolicyKit does not expose an API for listing registered authentication agents.
    let user = unsafe { libc::geteuid() };
    let mut agents = BTreeSet::new();
    for entry in std::fs::read_dir("/proc")?.flatten() {
        if !entry
            .file_name()
            .as_encoded_bytes()
            .iter()
            .all(u8::is_ascii_digit)
            || !entry
                .metadata()
                .is_ok_and(|metadata| metadata.uid() == user)
        {
            continue;
        }
        let Ok(executable) = entry.path().join("exe").read_link() else {
            continue;
        };
        let Some(name) = executable.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let name = name
            .strip_prefix('.')
            .and_then(|name| name.strip_suffix("-wrapped"))
            .unwrap_or(name);
        let agent = match name {
            "polkit-gnome-authentication-agent-1" => "GNOME PolicyKit Agent",
            "gnome-shell" => "GNOME Shell",
            "polkit-kde-authentication-agent-1" | "plasma-polkit-agent" => "KDE PolicyKit Agent",
            "lxqt-policykit-agent" => "LXQt PolicyKit Agent",
            "lxpolkit" => "LXDE PolicyKit Agent",
            "polkit-mate-authentication-agent-1" | "mate-polkit" => "MATE PolicyKit Agent",
            "xfce-polkit" => "Xfce PolicyKit Agent",
            "hyprpolkitagent" => "Hyprland PolicyKit Agent",
            "sway-polkit" => "Sway PolicyKit Agent",
            "polkit-dumb-agent" => "PolicyKit Dumb Agent",
            _ => continue,
        };
        agents.insert(agent.to_owned());
    }
    Ok(agents.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::portal_status;

    #[test]
    fn portal_status_separates_running_and_activatable_backends() {
        let running = vec![
            "org.freedesktop.portal.Desktop".to_owned(),
            "org.freedesktop.impl.portal.desktop.gnome".to_owned(),
        ];
        let activatable = vec![
            "org.freedesktop.portal.Desktop".to_owned(),
            "org.freedesktop.impl.portal.desktop.gnome".to_owned(),
            "org.freedesktop.impl.portal.desktop.gtk".to_owned(),
            "org.example.Unrelated".to_owned(),
        ];
        let output = portal_status(&running, &activatable);
        assert!(output.contains("Running\n"));
        assert!(output.contains("Available to start"));
        assert!(output.contains("GTK Portal Backend · Available; not running"));
        assert!(output.contains("D-Bus can activate them on demand"));
        assert!(!output.contains("org.example.Unrelated"));
        assert_eq!(output.matches("org.freedesktop.portal.Desktop").count(), 1);
    }

    #[test]
    fn portal_status_explains_when_no_portal_is_active_or_activatable() {
        assert_eq!(
            portal_status(&[], &[]),
            "No desktop portal services are running or available to activate."
        );
    }
}
