mod bar;
mod dock;
mod history;
mod lockscreen;
mod quick_settings;
mod run_command;

use gtk::glib::ExitCode;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::collections::HashMap;

const HELP: &str = "Usage: palobar-run-command-rs <run-command | run-panel | toggle-dock | toggle-quick-settings | lock | version>

Commands
  run-command            Open the command launcher
  run-panel              Open the panel
  toggle-dock            Toggle the standalone dock
  toggle-quick-settings  Toggle Quick Settings
  lock                   Lock the Wayland session
  version                Print the Palobar version";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    if args.next().is_some() {
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
        Some(quick_settings::ACTION) => toggle_quick_settings(),
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

fn toggle_quick_settings() -> glib::ExitCode {
    let panel_id = bar::APP_ID;
    if let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        let path = format!("/{}", panel_id.replace('.', "/"));
        let parameters = (
            quick_settings::ACTION,
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
        .application_id("io.Astal.Palobar.RustQuickSettings")
        .build();
    app.connect_startup(|_| {
        if let Some(display) = gdk::Display::default() {
            quick_settings::install_style(&display);
        }
    });
    app.connect_activate(quick_settings::toggle_first_monitor);
    app.run_with_args::<&str>(&[])
}
