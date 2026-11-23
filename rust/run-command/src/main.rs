mod bar;
mod history;
mod run_command;

use gtk::glib::ExitCode;

const HELP: &str = "Usage: palobar-run-command-rs <run-command | run-panel | version>

Commands
  run-command  Open the command launcher
  run-panel    Open the panel
  version      Print the Palobar version";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    if args.next().is_some() {
        eprintln!("{HELP}");
        return ExitCode::FAILURE;
    }

    match command.as_deref().map(|arg| arg.to_str().unwrap_or_default()) {
        Some("run-command") => run_command::run(),
        Some("run-panel") => bar::run(),
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
