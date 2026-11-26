use super::label;
use gtk::prelude::*;
use gtk::{gio, glib};
use serde::Deserialize;
use std::cell::{Cell, RefCell};
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

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("prefs-page-body");
    let window = window.downgrade();
    let layouts = group(&page, "Keyboard Layouts");
    row(&layouts, true).append(&label("Loading keyboard layouts…", "prefs-muted"));
    page.connect_map({
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
    page.append(&browse);
    let shortcuts = gtk::Button::with_mnemonic("Show keyboard _shortcuts");
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
    page.append(&shortcuts);
    let sources = group(&page, "Input Sources");
    input_source(&page, &sources, &window, "IBus", IBUS, false);
    input_source(&page, &sources, &window, "Fcitx", FCITX, true);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}
