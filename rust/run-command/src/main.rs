mod background;
mod bar;
mod calendar;
mod clipboard;
mod config;
mod dock;
mod history;
mod launcher;
mod lockscreen;
mod notifications;
mod overview;
mod panel_popup;
mod preferences;
mod quick_settings;
mod run_command;

use gtk::glib::ExitCode;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::collections::HashMap;

const HELP: &str = "Usage: palobar-run-command-rs <run-command | run-panel | toggle-dock | toggle-launcher | toggle-quick-settings | toggle-calendar | toggle-clipboard | prefs | lock | version>

Commands
  run-command            Open the command launcher
  run-panel              Open the panel
  toggle-dock            Toggle the standalone dock
  toggle-launcher        Toggle the applications launcher
  toggle-quick-settings  Toggle Quick Settings
  toggle-calendar        Toggle Calendar and Notifications
  toggle-clipboard       Toggle Clipboard History
  prefs                  Open Palobar Settings
  lock                   Lock the Wayland session
  version                Print the Palobar version";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    let mut args: Vec<_> = args.collect();
    let mode = command.as_deref().and_then(std::ffi::OsStr::to_str);
    if matches!(mode, Some("toggle-launcher" | "close-launcher")) && args.is_empty()
        || mode == Some("open-launcher")
            && (1..=2).contains(&args.len())
            && args[0]
                .to_str()
                .is_some_and(|index| index.parse::<u32>().is_ok())
    {
        if mode == Some("open-launcher") && args.len() == 2 {
            let mut query = std::ffi::OsString::from("query=");
            query.push(&args[1]);
            args[1] = query;
        }
        args.insert(0, command.expect("launcher command"));
        return launcher::run(&args);
    }
    if mode == Some(preferences::PREVIEW_COMMAND) {
        return preferences::run_preview(&args);
    }
    if !args.is_empty() {
        eprintln!("{HELP}");
        return ExitCode::FAILURE;
    }

    match command
        .as_deref()
        .map(|arg| arg.to_str().unwrap_or_default())
    {
        Some("run-command") => run_command::run(),
        Some("run-panel") => bar::run(),
        Some("toggle-dock") => dock::run(),
        Some(quick_settings::ACTION) => toggle_popup(
            quick_settings::ACTION,
            "io.Astal.Palobar.RustQuickSettings",
            quick_settings::install_style,
            quick_settings::toggle_first_monitor,
        ),
        Some(calendar::ACTION) => toggle_popup(
            calendar::ACTION,
            "io.Astal.Palobar.RustCalendar",
            calendar::install_style,
            calendar::toggle_first_monitor,
        ),
        Some(clipboard::ACTION) => toggle_popup(
            clipboard::ACTION,
            "io.Astal.Palobar.RustClipboard",
            clipboard::install_style,
            clipboard::toggle_first_monitor,
        ),
        Some("prefs") => preferences::run(),
        Some("lock") => lockscreen::run(),
        Some("version") => {
            println!("{}", include_str!("../../../VERSION").trim());
            ExitCode::SUCCESS
        }
        None | Some("--help" | "-h") => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{HELP}");
            ExitCode::FAILURE
        }
    }
}

fn toggle_popup(
    action: &str,
    application_id: &str,
    install_style: fn(&gdk::Display),
    toggle: fn(&gtk::Application),
) -> glib::ExitCode {
    let panel_id = bar::APP_ID;
    if let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        let path = format!("/{}", panel_id.replace('.', "/"));
        let parameters = (
            action,
            Vec::<glib::Variant>::new(),
            HashMap::<String, glib::Variant>::new(),
        );
        if bus
            .call_sync(
                Some(panel_id),
                &path,
                "org.gtk.Actions",
                "Activate",
                Some(&parameters.to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1000,
                gio::Cancellable::NONE,
            )
            .is_ok()
        {
            return glib::ExitCode::SUCCESS;
        }
    }
    let app = gtk::Application::builder()
        .application_id(application_id)
        .build();
    app.connect_startup(move |_| {
        if let Some(display) = gdk::Display::default() {
            install_style(&display);
        }
    });
    app.connect_activate(toggle);
    app.run_with_args::<&str>(&[])
}
