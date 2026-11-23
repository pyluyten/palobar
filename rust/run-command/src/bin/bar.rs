#[path = "../bar.rs"]
mod bar;
#[path = "../quick_settings.rs"]
mod quick_settings;

fn main() -> gtk::glib::ExitCode {
    bar::run()
}
