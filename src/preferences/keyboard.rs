use super::{desktop_services, label};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Deserialize)]
struct Layouts {
    names: Vec<String>,
    current_idx: usize,
}

type Services = &'static [(&'static str, &'static [&'static str])];
const IBUS: Services = &[("org.freedesktop.IBus", &["ibus-setup"])];
const FCITX: Services = &[
    (
        "org.fcitx.Fcitx5",
        &["fcitx5-configtool", "fcitx5-config-qt"],
    ),
    (
        "org.fcitx.Fcitx",
        &["fcitx-configtool", "fcitx-config-gtk3", "fcitx-config"],
    ),
];

async fn niri(args: &[&str]) -> Result<String, String> {
    let argv: Vec<_> = ["niri", "msg"]
        .into_iter()
        .chain(args.iter().copied())
        .map(OsStr::new)
        .collect();
    let process = gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    )
    .map_err(|error| error.to_string())?;
    let (output, error) = process
        .communicate_utf8_future(None)
        .await
        .map_err(|error| error.to_string())?;
    if process.is_successful() {
        Ok(output.map_or_else(String::new, |text| text.to_string()))
    } else {
        Err(error.filter(|text| !text.trim().is_empty()).map_or_else(
            || "Niri rejected the request.".to_owned(),
            |text| text.trim().to_owned(),
        ))
    }
}

fn alert(window: &glib::WeakRef<gtk::ApplicationWindow>, title: &str, detail: &str) {
    let Some(window) = window.upgrade() else {
        return;
    };
    let dialog = gtk::MessageDialog::builder()
        .transient_for(&window)
        .modal(true)
        .destroy_with_parent(true)
        .buttons(gtk::ButtonsType::Close)
        .text(title)
        .secondary_text(detail)
        .build();
    dialog.connect_response(|dialog, _| dialog.destroy());
    dialog.present();
}

mod registry {
    use std::ffi::{c_char, c_uint, CStr};

    #[repr(C)]
    struct Context {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct Layout {
        _private: [u8; 0],
    }

    #[link(name = "xkbregistry")]
    extern "C" {
        fn rxkb_context_new(flags: c_uint) -> *mut Context;
        fn rxkb_context_parse_default_ruleset(context: *mut Context) -> bool;
        fn rxkb_context_unref(context: *mut Context) -> *mut Context;
        fn rxkb_layout_first(context: *mut Context) -> *mut Layout;
        fn rxkb_layout_next(layout: *mut Layout) -> *mut Layout;
        fn rxkb_layout_get_name(layout: *mut Layout) -> *const c_char;
        fn rxkb_layout_get_variant(layout: *mut Layout) -> *const c_char;
        fn rxkb_layout_get_description(layout: *mut Layout) -> *const c_char;
    }

    pub(super) fn layouts() -> Vec<String> {
        // The registry owns the layouts and their strings until the context is released.
        unsafe {
            let context = rxkb_context_new(0);
            if context.is_null() {
                return Vec::new();
            }
            let mut rows = Vec::new();
            if rxkb_context_parse_default_ruleset(context) {
                let read = |text: *const c_char| {
                    (!text.is_null()).then(|| CStr::from_ptr(text).to_string_lossy().into_owned())
                };
                let mut layout = rxkb_layout_first(context);
                while !layout.is_null() {
                    if let Some(mut code) = read(rxkb_layout_get_name(layout)) {
                        if let Some(variant) = read(rxkb_layout_get_variant(layout)) {
                            code.push_str(" / ");
                            code.push_str(&variant);
                        }
                        if let Some(description) = read(rxkb_layout_get_description(layout)) {
                            code.push_str(" — ");
                            code.push_str(&description);
                        }
                        rows.push(code);
                    }
                    layout = rxkb_layout_next(layout);
                }
            }
            rxkb_context_unref(context);
            rows.sort_unstable();
            rows
        }
    }
}

fn show_registry(parent: &gtk::ApplicationWindow, rows: Vec<String>) {
    let dialog = gtk::Dialog::builder()
        .title("XKB Keyboard Layouts")
        .transient_for(parent)
        .modal(true)
        .destroy_with_parent(true)
        .default_width(640)
        .default_height(480)
        .build();
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.connect_response(|dialog, _| dialog.destroy());

    let content = dialog.content_area();
    content.set_spacing(8);
    content.add_css_class("prefs-page-body");
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search by code, variant, or name")
        .build();
    content.append(&search);
    content.append(&label(
        "Layout code / variant code — Description",
        "prefs-muted",
    ));
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let buffer = view.buffer();
    content.append(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&view)
            .build(),
    );
    buffer.set_text(&rows.join("\n"));
    search.connect_search_changed(move |search| {
        let query = search.text().trim().to_lowercase();
        let matches = rows
            .iter()
            .filter(|row| row.to_lowercase().contains(&query))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            buffer.set_text("No matching layouts.");
        } else {
            buffer.set_text(&matches.join("\n"));
        }
    });
    dialog.present();
    search.grab_focus();
}

fn group(page: &gtk::Box, title: &str) -> gtk::Box {
    page.append(&label(title, "prefs-group-title"));
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    page.append(&group);
    group
}

fn row(group: &gtk::Box, last: bool) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("prefs-row");
    if last {
        row.add_css_class("last");
    }
    group.append(&row);
    row
}

fn render_layouts(
    group: &gtk::Box,
    window: &glib::WeakRef<gtk::ApplicationWindow>,
    result: Result<Layouts, String>,
) {
    while let Some(child) = group.first_child() {
        group.remove(&child);
    }
    let layouts = match result {
        Ok(layouts) if !layouts.names.is_empty() => layouts,
        Ok(_) => {
            row(group, true).append(&label("No keyboard layouts found", "prefs-muted"));
            return;
        }
        Err(error) => {
            row(group, true).append(&label(
                &format!("Could not load Niri keyboard layouts. {error}"),
                "prefs-muted",
            ));
            return;
        }
    };
    for (index, name) in layouts.names.iter().enumerate() {
        let title = if layouts.names.iter().filter(|&other| other == name).count() > 1 {
            format!("{name} ({})", index + 1)
        } else {
            name.clone()
        };
        let row = row(group, index + 1 == layouts.names.len());
        let name = label(&title, "prefs-row-title");
        name.set_hexpand(true);
        row.append(&name);
        let active = index == layouts.current_idx;
        let button = gtk::Button::with_label(if active { "Active" } else { "Select" });
        button.add_css_class("prefs-btn");
        button.set_sensitive(!active);
        let tooltip = if active {
            "Current keyboard layout".to_owned()
        } else {
            format!("Switch to {title}")
        };
        button.set_tooltip_text(Some(&tooltip));
        button.connect_clicked({
            let group = group.downgrade();
            let window = window.clone();
            move |_| {
                if let Some(group) = group.upgrade() {
                    refresh(&group, &window, Some(index));
                }
            }
        });
        row.append(&button);
    }
}

fn refresh(
    group: &gtk::Box,
    window: &glib::WeakRef<gtk::ApplicationWindow>,
    switch_to: Option<usize>,
) {
    if !group.is_sensitive() {
        return;
    }
    group.set_sensitive(false);
    let group = group.downgrade();
    let window = window.clone();
    glib::spawn_future_local(async move {
        if let Some(index) = switch_to {
            if let Err(error) = niri(&["action", "switch-layout", &index.to_string()]).await {
                alert(&window, "Could not switch keyboard layout", &error);
            }
        }
        let layouts = niri(&["--json", "keyboard-layouts"])
            .await
            .and_then(|output| serde_json::from_str(&output).map_err(|error| error.to_string()));
        if let Some(group) = group.upgrade() {
            render_layouts(&group, &window, layouts);
            group.set_sensitive(true);
        }
    });
}

#[derive(Default)]
struct InputState {
    running: Cell<u8>,
    command: RefCell<Option<PathBuf>>,
    configuring: Cell<bool>,
}

fn input_source(
    page: &gtk::Box,
    group: &gtk::Box,
    window: &glib::WeakRef<gtk::ApplicationWindow>,
    name: &'static str,
    services: Services,
    last: bool,
) {
    let row = row(group, last);
    let status = label("", "prefs-row-title");
    status.set_hexpand(true);
    row.append(&status);
    let configure = gtk::Button::with_mnemonic(&format!("Configure _{name}"));
    configure.set_sensitive(false);
    configure.add_css_class("prefs-btn");
    row.append(&configure);
    let state = Rc::new(InputState::default());
    let sync = {
        let state = state.clone();
        let status = status.downgrade();
        let configure = configure.downgrade();
        move || {
            let active = services
                .iter()
                .enumerate()
                .find(|(index, _)| state.running.get() & (1 << index) != 0);
            let tools = active.map(|(_, (_, tools))| *tools).unwrap_or_default();
            let command = tools.iter().find_map(glib::find_program_in_path);
            let missing = tools
                .first()
                .filter(|_| command.is_none())
                .map(|tool| format!("Install {tool} to configure {name}"));
            if let Some(status) = status.upgrade() {
                status.set_text(&if active.is_some() {
                    format!("{name} is running")
                } else {
                    format!("{name} is not running")
                });
            }
            if let Some(configure) = configure.upgrade() {
                configure.set_sensitive(command.is_some() && !state.configuring.get());
                configure.set_tooltip_text(missing.as_deref());
            }
            state.command.replace(command);
        }
    };
    let watchers = Rc::new(RefCell::new(Vec::new()));
    let connecting = Rc::new(RefCell::new(None::<glib::JoinHandle<()>>));
    page.connect_map({
        let state = state.clone();
        let watchers = watchers.clone();
        let connecting = connecting.clone();
        let sync = sync.clone();
        let status = status.downgrade();
        let configure = configure.downgrade();
        move |_| {
            if let Some(status) = status.upgrade() {
                status.set_text(&format!("Checking {name}…"));
            }
            if let Some(configure) = configure.upgrade() {
                configure.set_sensitive(false);
            }
            let (state, watchers, sync) = (state.clone(), watchers.clone(), sync.clone());
            connecting.replace(Some(glib::spawn_future_local(async move {
                let Ok(connection) = gio::bus_get_future(gio::BusType::Session).await else {
                    sync();
                    return;
                };
                watchers
                    .borrow_mut()
                    .extend(services.iter().enumerate().map(|(index, (bus, _))| {
                        let bit = 1 << index;
                        gio::bus_watch_name_on_connection(
                            &connection,
                            bus,
                            gio::BusNameWatcherFlags::NONE,
                            {
                                let state = state.clone();
                                let sync = sync.clone();
                                move |_, _, _| {
                                    state.running.set(state.running.get() | bit);
                                    sync();
                                }
                            },
                            {
                                let state = state.clone();
                                let sync = sync.clone();
                                move |_, _| {
                                    state.running.set(state.running.get() & !bit);
                                    sync();
                                }
                            },
                        )
                    }));
            })));
        }
    });
    page.connect_unmap({
        let state = state.clone();
        move |_| {
            if let Some(connection) = connecting.take() {
                connection.abort();
            }
            for watcher in watchers.take() {
                gio::bus_unwatch_name(watcher);
            }
            state.running.set(0);
            state.command.replace(None);
        }
    });
    configure.connect_clicked({
        let window = window.clone();
        move |_| {
            let Some(command) = state.command.borrow().clone() else {
                return;
            };
            let title = format!("Could not configure {name}");
            match gio::Subprocess::newv(&[command.as_os_str()], gio::SubprocessFlags::NONE) {
                Ok(process) => {
                    state.configuring.set(true);
                    sync();
                    let state = state.clone();
                    let sync = sync.clone();
                    let window = window.clone();
                    glib::spawn_future_local(async move {
                        if let Err(error) = process.wait_check_future().await {
                            alert(&window, &title, &error.to_string());
                        }
                        state.configuring.set(false);
                        sync();
                    });
                }
                Err(error) => alert(&window, &title, &error.to_string()),
            }
        }
    });
}

async fn global_shortcuts_status() -> String {
    let Ok(bus) = gio::bus_get_future(gio::BusType::Session).await else {
        return "Could not connect to the session bus to check Global Shortcuts.".into();
    };
    let result = bus
        .call_future(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&("org.freedesktop.portal.GlobalShortcuts", "version").to_variant()),
            Some(glib::VariantTy::new("(v)").unwrap()),
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await;
    match result {
        Ok(reply) if reply.child_get::<glib::Variant>(0).get::<u32>().is_some_and(|version| version > 0) => {
            "The Global Shortcuts portal is available.".into()
        }
        Err(error) if error.matches(gio::DBusError::TimedOut) || error.matches(gio::IOErrorEnum::TimedOut) => {
            "The Global Shortcuts portal did not respond.".into()
        }
        _ => "Global Shortcuts is unavailable. Check that xdg-desktop-portal-gnome is installed and configured for this Niri session.".into(),
    }
}

const SHORTCUT_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts";
type ApplicationShortcuts = Vec<(String, HashMap<String, glib::Variant>)>;

fn shortcut_settings() -> Option<(gio::Settings, gio::SettingsSchema)> {
    let source = gio::SettingsSchemaSource::default()?;
    let registry = source.lookup(SHORTCUT_SCHEMA, true)?;
    let app = source.lookup(&format!("{SHORTCUT_SCHEMA}.application"), true)?;
    if !registry.has_key("applications") || !app.has_key("shortcuts") {
        return None;
    }
    Some((
        gio::Settings::new_full(&registry, gio::SettingsBackend::NONE, None),
        app,
    ))
}

fn accelerator_list(text: &str) -> Result<Vec<String>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut shortcuts = Vec::new();
    for item in text.split(',').map(str::trim) {
        let (key, modifiers) = gtk::accelerator_parse(item)
            .filter(|(key, modifiers)| gtk::accelerator_valid(*key, *modifiers))
            .ok_or_else(|| format!("Invalid shortcut: {item}"))?;
        let accelerator = gtk::accelerator_name(key, modifiers).to_string();
        if !shortcuts.contains(&accelerator) {
            shortcuts.push(accelerator);
        }
    }
    Ok(shortcuts)
}

async fn save_shortcut(
    settings: &gio::Settings,
    app_id: &str,
    shortcut_id: &str,
    shortcuts: Vec<String>,
) -> Result<(), String> {
    let bus = gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|error| error.to_string())?;
    let previous = settings.value("shortcuts");
    let mut bindings = previous
        .get::<ApplicationShortcuts>()
        .ok_or("Unsupported GNOME shortcut format.")?;
    let (_, properties) = bindings
        .iter_mut()
        .find(|(id, _)| id == shortcut_id)
        .ok_or("This shortcut is no longer registered.")?;
    properties.insert("shortcuts".into(), shortcuts.to_variant());
    let updated = bindings.to_variant();
    settings
        .set_value("shortcuts", &updated)
        .map_err(|error| error.to_string())?;
    gio::Settings::sync();
    let result = bus
        .call_future(
            Some("org.freedesktop.impl.portal.desktop.gnome"),
            "/org/gnome/globalshortcuts",
            "org.gnome.GlobalShortcutsRebind",
            "RebindShortcuts",
            Some(&(app_id, bindings).to_variant()),
            Some(glib::VariantTy::UNIT),
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await;
    if let Err(error) = result {
        if settings.value("shortcuts") == updated {
            settings.set_value("shortcuts", &previous)
                .map_err(|rollback| format!("Could not rebind the shortcut. {error} The previous setting could not be restored. {rollback}"))?;
            gio::Settings::sync();
        }
        return Err(format!("Could not rebind the shortcut. {error}"));
    }
    Ok(())
}

fn edit_shortcut(
    parent: &gtk::ApplicationWindow,
    list: &gtk::Box,
    settings: gio::Settings,
    app_id: String,
    id: String,
    name: &str,
    shortcuts: &[String],
) {
    let dialog = gtk::Dialog::builder()
        .title("Edit Global Shortcut")
        .transient_for(parent)
        .destroy_with_parent(true)
        .modal(true)
        .default_width(460)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Save", gtk::ResponseType::Accept);
    let content = dialog.content_area();
    content.set_spacing(8);
    content.add_css_class("prefs-page-body");
    content.append(&label(name, "prefs-row-title"));
    let entry = gtk::Entry::builder()
        .text(shortcuts.join(", "))
        .placeholder_text("<Super>a")
        .build();
    entry.update_property(&[gtk::accessible::Property::Label("Shortcut bindings")]);
    content.append(&entry);
    content.append(&label(
        "Separate multiple shortcuts with commas. Leave blank to disable.",
        "prefs-muted",
    ));
    let record = gtk::ToggleButton::with_label("Record Shortcut");
    record.set_halign(gtk::Align::Start);
    content.append(&record);
    let status = label("", "error");
    status.set_visible(false);
    content.append(&status);
    record.connect_toggled(|button| {
        button.set_label(if button.is_active() {
            button.grab_focus();
            "Press a Shortcut…"
        } else {
            "Record Shortcut"
        });
    });
    record.connect_has_focus_notify(|button| {
        if !button.has_focus() {
            button.set_active(false);
        }
    });
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let entry = entry.downgrade();
        move |controller, key, _, modifiers| {
            let Some(button) = controller.widget().and_downcast::<gtk::ToggleButton>() else {
                return glib::Propagation::Proceed;
            };
            if !button.is_active() {
                return glib::Propagation::Proceed;
            }
            if key == gdk::Key::Escape {
                button.set_active(false);
            } else {
                let modifiers = modifiers & gtk::accelerator_get_default_mod_mask();
                if gtk::accelerator_valid(key, modifiers) {
                    if let Some(entry) = entry.upgrade() {
                        entry.set_text(&gtk::accelerator_name(key.to_lower(), modifiers));
                    }
                    button.set_active(false);
                }
            }
            glib::Propagation::Stop
        }
    });
    record.add_controller(keys);
    let pending = Rc::new(Cell::new(false));
    dialog.connect_close_request({
        let pending = pending.clone();
        move |_| {
            if pending.get() {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    let list = list.downgrade();
    let parent = parent.downgrade();
    dialog.connect_response(move |dialog, response| {
        if pending.get() {
            return;
        }
        if response != gtk::ResponseType::Accept {
            dialog.destroy();
            return;
        }
        let shortcuts = match accelerator_list(&entry.text()) {
            Ok(shortcuts) => shortcuts,
            Err(error) => {
                status.set_text(&error);
                status.set_visible(true);
                return;
            }
        };
        pending.set(true);
        dialog.set_response_sensitive(gtk::ResponseType::Accept, false);
        dialog.set_response_sensitive(gtk::ResponseType::Cancel, false);
        let (dialog, status) = (dialog.downgrade(), status.downgrade());
        let (settings, app_id, id, list, parent, pending) = (
            settings.clone(),
            app_id.clone(),
            id.clone(),
            list.clone(),
            parent.clone(),
            pending.clone(),
        );
        glib::spawn_future_local(async move {
            let result = save_shortcut(&settings, &app_id, &id, shortcuts).await;
            pending.set(false);
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            match result {
                Ok(()) => {
                    dialog.destroy();
                    if let (Some(list), Some(parent)) = (list.upgrade(), parent.upgrade()) {
                        render_global_shortcuts(&list, &parent);
                    }
                }
                Err(error) => {
                    dialog.set_response_sensitive(gtk::ResponseType::Accept, true);
                    dialog.set_response_sensitive(gtk::ResponseType::Cancel, true);
                    if let Some(status) = status.upgrade() {
                        status.set_text(&error);
                        status.set_visible(true);
                    }
                }
            }
        });
    });
    dialog.present();
}

fn render_global_shortcuts(list: &gtk::Box, window: &gtk::ApplicationWindow) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let Some((registry, schema)) = shortcut_settings() else {
        list.append(&label(
            "The GNOME Global Shortcuts settings schemas are unavailable.",
            "prefs-muted",
        ));
        return;
    };
    let mut apps: Vec<_> = registry.strv("applications").into_iter().collect();
    apps.sort_unstable();
    apps.dedup();
    for app_id in apps {
        if app_id.is_empty() || app_id.contains('/') {
            continue;
        }
        let path = format!("/org/gnome/settings-daemon/global-shortcuts/{app_id}/");
        let settings = gio::Settings::new_full(&schema, gio::SettingsBackend::NONE, Some(&path));
        let Some(shortcuts) = settings.value("shortcuts").get::<ApplicationShortcuts>() else {
            continue;
        };
        if shortcuts.is_empty() {
            continue;
        }
        let name = gio::DesktopAppInfo::new(&format!("{app_id}.desktop"))
            .map_or_else(|| app_id.to_string(), |app| app.display_name().into());
        let application = group(list, &name);
        let count = shortcuts.len();
        for (index, (id, properties)) in shortcuts.into_iter().enumerate() {
            let description = properties
                .get("description")
                .and_then(glib::Variant::str)
                .unwrap_or(&id);
            let bindings = properties
                .get("shortcuts")
                .and_then(glib::Variant::get::<Vec<String>>)
                .unwrap_or_default();
            let title = format!("{name} — {description}");
            let details = gtk::Box::new(gtk::Orientation::Vertical, 4);
            details.set_hexpand(true);
            details.append(&label(description, "prefs-row-title"));
            let display = bindings
                .iter()
                .map(|binding| {
                    gtk::accelerator_parse(binding).map_or_else(
                        || binding.clone(),
                        |(key, modifiers)| gtk::accelerator_get_label(key, modifiers).to_string(),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            details.append(&label(
                if bindings.is_empty() {
                    "Disabled"
                } else {
                    &display
                },
                "prefs-muted",
            ));
            let row = row(&application, index + 1 == count);
            row.append(&details);
            let edit = gtk::Button::with_label("Edit");
            edit.add_css_class("prefs-btn");
            edit.set_valign(gtk::Align::Center);
            edit.set_sensitive(settings.is_writable("shortcuts"));
            let (window, list, settings, app_id) = (
                window.downgrade(),
                list.downgrade(),
                settings.clone(),
                app_id.to_string(),
            );
            edit.connect_clicked(move |_| {
                if let (Some(window), Some(list)) = (window.upgrade(), list.upgrade()) {
                    edit_shortcut(
                        &window,
                        &list,
                        settings.clone(),
                        app_id.clone(),
                        id.clone(),
                        &title,
                        &bindings,
                    );
                }
            });
            row.append(&edit);
        }
    }
    if list.first_child().is_none() {
        list.append(&label(
            "No applications have registered global shortcuts.",
            "prefs-muted",
        ));
    }
}

fn global_shortcuts(page: &gtk::Box, window: &glib::WeakRef<gtk::ApplicationWindow>) {
    page.append(&label("Global Shortcuts", "prefs-group-title"));
    page.append(&desktop_services::status(page, global_shortcuts_status));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.append(&list);
    let window = window.clone();
    page.connect_map(move |_| {
        if let Some(window) = window.upgrade() {
            render_global_shortcuts(&list, &window);
        }
    });
}

fn expandable_section(page: &gtk::Box, title: &str) -> gtk::Box {
    let title = label(title, "prefs-row-title");
    title.set_margin_top(12);
    title.set_margin_bottom(12);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_start(16);
    body.set_margin_end(16);
    body.set_margin_bottom(16);
    let section = gtk::Expander::builder()
        .label_widget(&title)
        .child(&body)
        .build();
    section.add_css_class("prefs-group");
    section.add_css_class("prefs-keyboard-section");
    page.append(&section);
    body
}

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("prefs-page-body");
    let window = window.downgrade();
    let layout = expandable_section(&page, "Layout");
    let layouts = group(&layout, "Keyboard Layouts");
    row(&layouts, true).append(&label("Loading keyboard layouts…", "prefs-muted"));
    layout.connect_map({
        let window = window.clone();
        move |_| refresh(&layouts, &window, None)
    });
    let browse = gtk::Button::with_label("Browse XKB Layouts");
    browse.add_css_class("prefs-btn");
    browse.set_halign(gtk::Align::Start);
    browse.connect_clicked({
        let window = window.clone();
        move |button| {
            button.set_sensitive(false);
            let (button, window) = (button.downgrade(), window.clone());
            glib::spawn_future_local(async move {
                let rows = gio::spawn_blocking(registry::layouts)
                    .await
                    .unwrap_or_default();
                if let Some(button) = button.upgrade() {
                    button.set_sensitive(true);
                }
                if rows.is_empty() {
                    alert(
                        &window,
                        "Could not load XKB layouts",
                        "The XKB registry returned no layouts.",
                    );
                } else if let Some(window) = window.upgrade() {
                    show_registry(&window, rows);
                }
            });
        }
    });
    layout.append(&browse);
    let sources = group(&layout, "Input Sources");
    input_source(&layout, &sources, &window, "IBus", IBUS, false);
    input_source(&layout, &sources, &window, "Fcitx", FCITX, true);

    let shortcut_section = expandable_section(&page, "Shortcuts");
    let shortcuts = gtk::Button::with_label("Show Current Shortcuts");
    shortcuts.add_css_class("prefs-btn");
    shortcuts.set_halign(gtk::Align::Start);
    shortcuts.connect_clicked({
        let window = window.clone();
        move |_| {
            let window = window.clone();
            glib::spawn_future_local(async move {
                if let Err(error) = niri(&["action", "show-hotkey-overlay"]).await {
                    alert(&window, "Could not show keyboard shortcuts", &error);
                }
            });
        }
    });
    shortcut_section.append(&shortcuts);
    global_shortcuts(&shortcut_section, &window);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}
