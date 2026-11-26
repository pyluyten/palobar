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

async fn session_bus_names() -> Result<Vec<String>, glib::Error> {
    let bus = gio::bus_get_future(gio::BusType::Session).await?;
    let reply = bus
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
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
        let Ok(mut names) = session_bus_names().await else {
            return "Could not check desktop portals on the session bus.".into();
        };
        names.retain(|name| {
            name.starts_with("org.freedesktop.portal.")
                || name.starts_with("org.freedesktop.impl.portal.")
        });
        names.sort_unstable();
        if names.is_empty() {
            "No active desktop portals detected.".into()
        } else {
            names.join("\n")
        }
    })
}

pub(super) fn notifications() -> gtk::ScrolledWindow {
    page("Notification Service", || async {
        match session_bus_names().await {
            Ok(names)
                if names
                    .iter()
                    .any(|name| name == "org.freedesktop.Notifications") =>
            {
                "A notification service is running.".into()
            }
            Ok(_) => "No notification service is running.".into(),
            Err(_) => "Could not check the notification service on the session bus.".into(),
        }
    })
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
