#[path = "../bar.rs"]
mod bar;
#[path = "../calendar.rs"]
mod calendar;
#[path = "../dock.rs"]
mod dock;
#[path = "../launcher.rs"]
mod launcher;
#[path = "../lockscreen.rs"]
mod lockscreen;
#[path = "../overview.rs"]
mod overview;
#[path = "../panel_popup.rs"]
mod panel_popup;
#[path = "../quick_settings.rs"]
mod quick_settings;

fn main() -> gtk::glib::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        Some("lock") => lockscreen::run(),
        Some("toggle-launcher") => launcher::run(&args),
        Some("toggle-dock") => dock::run(),
        _ => bar::run(),
    }
}
