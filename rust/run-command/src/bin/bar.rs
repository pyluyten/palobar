#[path = "../bar.rs"]
mod bar;

fn main() -> gtk::glib::ExitCode {
    bar::run()
}
