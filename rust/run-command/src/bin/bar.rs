#[path = "../background.rs"]
mod background;
#[path = "../bar.rs"]
mod bar;
#[path = "../calendar.rs"]
mod calendar;
#[path = "../clipboard.rs"]
mod clipboard;
#[path = "../config.rs"]
mod config;
#[path = "../dock.rs"]
mod dock;
#[path = "../launcher.rs"]
mod launcher;
#[path = "../lockscreen.rs"]
mod lockscreen;
#[path = "../notifications.rs"]
mod notifications;
#[path = "../overview.rs"]
mod overview;
#[path = "../panel_popup.rs"]
mod panel_popup;
#[path = "../preferences.rs"]
mod preferences;
#[path = "../quick_settings.rs"]
mod quick_settings;
#[path = "../workspace_dialog.rs"]
mod workspace_dialog;

fn main() -> gtk::glib::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        Some("prefs") => preferences::run(),
        Some(preferences::PREVIEW_COMMAND) => preferences::run_preview(&args[1..]),
        Some("lock") => lockscreen::run(),
        Some("toggle-launcher") => launcher::run(&args),
        Some("toggle-dock") => dock::run(),
        Some(workspace_dialog::ACTION) => workspace_dialog::run(&args[1..]),
        _ => bar::run(),
    }
}
