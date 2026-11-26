use super::{append_dependency_warning, label};
use crate::config::{read_config, update_config};
use gtk::prelude::*;
use gtk::{gio, glib};
use serde_json::Value;
use std::cell::Cell;
use std::path::Path;

const BLANK_DELAYS: &[(&str, u32)] = &[
    ("1 minute", 1),
    ("2 minutes", 2),
    ("5 minutes", 5),
    ("10 minutes", 10),
    ("15 minutes", 15),
    ("30 minutes", 30),
    ("1 hour", 60),
];
const SUSPEND_DELAYS: &[(&str, u32)] = &[
    ("15 minutes", 15),
    ("30 minutes", 30),
    ("45 minutes", 45),
    ("1 hour", 60),
    ("2 hours", 120),
];

struct Setting {
    title: &'static str,
    description: Option<&'static str>,
    enabled_key: &'static str,
    minutes_key: &'static str,
    default_enabled: bool,
    default_index: u32,
    delays: &'static [(&'static str, u32)],
}

fn save(path: &Path, status: &gtk::Label, key: &str, value: Value) -> bool {
    let result = (|| {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        update_config(&gio::File::for_path(path), |config| {
            let power = &mut config["power"];
            if power.is_null() {
                *power = serde_json::json!({});
            }
            let power = power
                .as_object_mut()
                .ok_or("Power configuration must be a JSON object")?;
            power.insert(key.to_owned(), value.clone());
            Ok(true)
        })
    })();
    match result {
        Ok(()) => {
            status.set_visible(false);
            true
        }
        Err(error) => {
            status.set_text(&format!("Could not save power settings: {error}"));
            status.set_visible(true);
            false
        }
    }
}

fn group(config: &Value, path: &Path, status: &gtk::Label, setting: Setting) -> gtk::Box {
    let enabled = config[setting.enabled_key]
        .as_bool()
        .unwrap_or(setting.default_enabled);
    let selected = setting
        .delays
        .iter()
        .position(|(_, minutes)| config[setting.minutes_key].as_f64() == Some(f64::from(*minutes)))
        .map_or(setting.default_index, |index| index as u32);

    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    let toggle_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    toggle_row.add_css_class("prefs-row");
    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.append(&label(setting.title, "prefs-row-title"));
    if let Some(description) = setting.description {
        info.append(&label(description, "prefs-muted"));
    }
    let toggle = gtk::Switch::builder()
        .active(enabled)
        .valign(gtk::Align::Center)
        .build();
    toggle.update_property(&[gtk::accessible::Property::Label(setting.title)]);
    toggle_row.append(&info);
    toggle_row.append(&toggle);
    group.append(&toggle_row);

    let delay_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    delay_row.add_css_class("prefs-row");
    delay_row.add_css_class("last");
    delay_row.set_sensitive(enabled);
    let delay_label = label("Delay", "prefs-row-title");
    delay_label.set_hexpand(true);
    let names: Vec<_> = setting.delays.iter().map(|(name, _)| *name).collect();
    let dropdown = gtk::DropDown::from_strings(&names);
    dropdown.update_property(&[gtk::accessible::Property::Label(&format!(
        "{} Delay",
        setting.title
    ))]);
    dropdown.set_selected(selected);
    delay_row.append(&delay_label);
    delay_row.append(&dropdown);
    group.append(&delay_row);

    toggle.connect_active_notify({
        let path = path.to_owned();
        let status = status.clone();
        let saved = Cell::new(enabled);
        move |toggle| {
            let active = toggle.is_active();
            if active == saved.get() {
                return;
            }
            if save(&path, &status, setting.enabled_key, active.into()) {
                saved.set(active);
                delay_row.set_sensitive(active);
            } else {
                toggle.set_active(saved.get());
            }
        }
    });
    dropdown.connect_selected_notify({
        let path = path.to_owned();
        let status = status.clone();
        let saved = Cell::new(selected);
        move |dropdown| {
            let selected = dropdown.selected();
            if selected == saved.get() {
                return;
            }
            if let Some(&(_, minutes)) = setting.delays.get(selected as usize) {
                if save(&path, &status, setting.minutes_key, minutes.into()) {
                    saved.set(selected);
                    return;
                }
            }
            dropdown.set_selected(saved.get());
        }
    });
    group
}

pub(super) fn build() -> gtk::ScrolledWindow {
    let path = glib::user_config_dir().join("palobar/config.json");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.add_css_class("prefs-page-body");
    content.append(&label("Power Saving", "prefs-group-title"));
    append_dependency_warning(
        &content,
        "swayidle",
        "swayidle was not found in PATH. Automatic screen blanking and suspend require swayidle.",
    );
    let status = label("", "error");
    status.set_visible(false);
    content.append(&status);
    let config = match read_config(&gio::File::for_path(&path)) {
        Ok((config, _)) => config,
        Err(error) => {
            status.set_text(&format!("Could not load power settings: {error}"));
            status.set_visible(true);
            Value::Null
        }
    };
    for setting in [
        Setting {
            title: "Automatic Screen Blank",
            description: Some("Turn off the screen after a period of inactivity"),
            enabled_key: "screenBlankEnabled",
            minutes_key: "screenBlankMinutes",
            default_enabled: true,
            default_index: 3,
            delays: BLANK_DELAYS,
        },
        Setting {
            title: "Automatic Suspend",
            description: None,
            enabled_key: "autoSuspendEnabled",
            minutes_key: "autoSuspendMinutes",
            default_enabled: false,
            default_index: 4,
            delays: SUSPEND_DELAYS,
        },
    ] {
        content.append(&group(&config["power"], &path, &status, setting));
    }
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build()
}
