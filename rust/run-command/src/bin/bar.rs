#[path = "../bar.rs"]
mod bar;
#[path = "../lockscreen.rs"]
mod lockscreen;
#[path = "../quick_settings.rs"]
mod quick_settings;

fn main() -> gtk::glib::ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("lock")) {
        return lockscreen::run();
    }
    bar::run()
}
