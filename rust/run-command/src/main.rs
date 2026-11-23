use gtk::gdk;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Box as GtkBox, CssProvider, Entry, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const APP_ID: &str = "io.Astal.Palobar.RunCommandRust";

fn spawn_command(command: &str, in_terminal: bool) {
    let command_text = command.trim();
    if command_text.is_empty() {
        return;
    }

    let terminal = if in_terminal { find_terminal() } else { None };

    let result = if let Some((binary, flag)) = terminal {
        let mut command = Command::new("niri");
        command.args(["msg", "action", "spawn", "--"]);
        command.arg(binary);
        if let Some(flag) = flag {
            command.arg(flag);
        }
        command.args(["sh", "-c", command_text]);
        command.status()
    } else {
        Command::new("niri")
            .args(["msg", "action", "spawn-sh", "--", command_text])
            .status()
    };

    if let Err(error) = result {
        eprintln!("[run-command-rs] failed to ask niri to spawn command: {error}");
    }
}

fn find_terminal() -> Option<(String, Option<&'static str>)> {
    if executable_in_path("xdg-terminal-exec") {
        return Some(("xdg-terminal-exec".into(), None));
    }

    if let Ok(terminal) = std::env::var("TERMINAL") {
        if executable_in_path(&terminal) {
            let flag = if terminal == "gnome-terminal" {
                "--"
            } else {
                "-e"
            };
            return Some((terminal, Some(flag)));
        }
    }

    [
        ("ghostty", Some("-e")),
        ("kitty", Some("-e")),
        ("alacritty", Some("-e")),
        ("foot", Some("-e")),
        ("wezterm", Some("-e")),
        ("ptyxis", Some("-e")),
        ("kgx", Some("-e")),
        ("gnome-terminal", Some("--")),
        ("xterm", Some("-e")),
    ]
    .into_iter()
    .find(|(binary, _)| executable_in_path(binary))
    .map(|(binary, flag)| (binary.into(), flag))
}

fn executable_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .any(|directory| {
            let path = directory.join(name);
            std::fs::metadata(path)
                .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
}

fn build_window(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Run a command")
        .default_width(440)
        .build();
    window.add_css_class("run-command-overlay");

    window.init_layer_shell();
    window.set_namespace(Some("palobar-run-command"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_exclusive_zone(0);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }

    let entry = Entry::builder()
        .placeholder_text("Run a command... (Ctrl+Enter for terminal)")
        .width_request(440)
        .build();
    entry.add_css_class("run-command-entry");

    let container = GtkBox::new(Orientation::Vertical, 10);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Start);
    container.set_margin_top(120);
    container.set_width_request(440);
    container.append(&entry);
    window.set_child(Some(&container));

    let submit_entry = entry.clone();
    let submit_window = window.clone();
    entry.connect_activate(move |_| {
        spawn_command(&submit_entry.text(), false);
        submit_window.close();
    });

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let key_window = window.clone();
    let key_entry = entry.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gdk::Key::Escape {
            key_window.close();
            return gtk::glib::Propagation::Stop;
        }
        if (key == gdk::Key::Return || key == gdk::Key::KP_Enter)
            && modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        {
            spawn_command(&key_entry.text(), true);
            key_window.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(keys);

    let provider = CssProvider::new();
    provider.load_from_data(
        ".run-command-overlay { background: rgba(0, 0, 0, 0.28); }\
         .run-command-entry { padding: 14px 18px; border-radius: 18px; }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    window.present();
    entry.grab_focus();
}

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_window);
    app.run();
}
