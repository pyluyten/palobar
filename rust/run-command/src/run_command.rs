use crate::history;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, CssProvider, Entry, EntryIconPosition,
    EventControllerKey, GestureClick, Label, ListBox, ListBoxRow, Orientation, SelectionMode,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::rc::Rc;
use std::thread;

const APP_ID: &str = "io.Astal.Palobar.RunCommandRust";

fn spawn_command(command: &str, in_terminal: bool) {
    let command_text = command.trim();
    if command_text.is_empty() {
        return;
    }

    let command_text = command_text.to_string();
    thread::spawn(move || {
        let terminal = if in_terminal { find_terminal() } else { None };
        let terminal_launched = terminal.is_some();
        let result = if let Some((binary, flag)) = terminal {
            let mut niri = Command::new("niri");
            niri.args(["msg", "action", "spawn", "--"]);
            niri.arg(binary);
            if let Some(flag) = flag {
                niri.arg(flag);
            }
            niri.args(["sh", "-c", &command_text]);
            niri.output()
        } else {
            Command::new("niri")
                .args(["msg", "action", "spawn-sh", "--", &command_text])
                .output()
        };

        match result {
            Ok(output) if output.status.success() => {
                history::record_run(&command_text, terminal_launched);
            }
            Ok(output) => eprintln!(
                "[run-command-rs] niri rejected command: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(error) => {
                eprintln!("[run-command-rs] failed to ask niri to spawn command: {error}")
            }
        }
    });
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

fn collect_path_executables() -> Vec<String> {
    let mut seen = HashSet::new();
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if seen.contains(&name) {
                continue;
            }
            if entry
                .metadata()
                .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
            {
                seen.insert(name);
            }
        }
    }
    let mut executables: Vec<_> = seen.into_iter().collect();
    executables.sort_unstable();
    executables
}

#[derive(Default)]
struct RunState {
    draft: String,
    history_matches: Vec<history::RunHistoryEntry>,
    history_index: isize,
    recalling_history: bool,
    matches: Vec<String>,
    selected_index: usize,
}

fn show_terminal_mode(entry: &Entry, in_terminal: bool) {
    if in_terminal {
        entry.add_css_class("run-command-terminal");
        entry.set_icon_tooltip_text(EntryIconPosition::Primary, Some("Run in Terminal"));
    } else {
        entry.remove_css_class("run-command-terminal");
        entry.set_icon_tooltip_text(EntryIconPosition::Primary, None);
    }
}

fn accent_color(settings: &gtk::gio::Settings) -> &'static str {
    match settings.string("accent-color").as_str() {
        "teal" => "#2190a4",
        "green" => "#3a944a",
        "yellow" => "#c88800",
        "orange" => "#ed5b00",
        "red" => "#e62d42",
        "pink" => "#d56199",
        "purple" => "#9141ac",
        "slate" => "#6f8396",
        _ => "#3584e4",
    }
}

fn render_matches(
    raw_text: &str,
    list: &ListBox,
    executables: &[String],
    state: &Rc<RefCell<RunState>>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let token = raw_text.split_whitespace().next().unwrap_or("");
    if token.is_empty() || raw_text.contains(' ') {
        state.borrow_mut().matches.clear();
        list.set_visible(false);
        return;
    }

    let token = token.to_lowercase();
    let mut prefix = Vec::new();
    let mut substring = Vec::new();
    for executable in executables {
        let lower = executable.to_lowercase();
        if lower.starts_with(&token) {
            prefix.push(executable.clone());
            if prefix.len() == 8 {
                break;
            }
        } else if lower.contains(&token) {
            substring.push(executable.clone());
        }
    }
    prefix.extend(substring);
    prefix.truncate(8);

    {
        let mut current = state.borrow_mut();
        current.matches = prefix;
        current.selected_index = 0;
    }
    for command in &state.borrow().matches {
        let label = Label::new(Some(command));
        label.set_halign(gtk::Align::Start);
        label.add_css_class("run-completion-label");
        let row = ListBoxRow::new();
        row.set_child(Some(&label));
        row.add_css_class("run-completion-row");
        list.append(&row);
    }
    if !state.borrow().matches.is_empty() {
        if let Some(row) = list.row_at_index(0) {
            list.select_row(Some(&row));
        }
        list.set_visible(true);
    } else {
        list.set_visible(false);
    }
}

fn navigate_history(
    direction: isize,
    entry: &Entry,
    list: &ListBox,
    executables: &[String],
    state: &Rc<RefCell<RunState>>,
    history: &[history::RunHistoryEntry],
) -> bool {
    {
        let mut current = state.borrow_mut();
        if current.history_index == -1 {
            if direction < 0 {
                return false;
            }
            current.history_matches = history::matching_history(history, &current.draft);
            if current.history_matches.is_empty() {
                return false;
            }
        }
        let last = current.history_matches.len() as isize - 1;
        current.history_index = (current.history_index + direction).clamp(-1, last);
        current.recalling_history = true;
    }

    let (text, terminal_mode) = {
        let current = state.borrow();
        let recalled = (current.history_index >= 0)
            .then(|| &current.history_matches[current.history_index as usize]);
        (
            recalled
                .map(|item| item.command.clone())
                .unwrap_or_else(|| current.draft.clone()),
            recalled.is_some_and(|item| item.in_terminal),
        )
    };
    entry.set_text(&text);
    entry.set_position(-1);
    state.borrow_mut().recalling_history = false;
    show_terminal_mode(entry, terminal_mode);

    if terminal_mode || state.borrow().history_index >= 0 {
        state.borrow_mut().matches.clear();
        list.set_visible(false);
    } else {
        render_matches(&text, list, executables, state);
    }
    true
}

fn submit_command(command: &str, in_terminal: bool, window: &ApplicationWindow) {
    if command.trim().is_empty() {
        return;
    }
    spawn_command(command, in_terminal);
    window.close();
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
    if let Some(display) = gdk::Display::default() {
        let monitors = display.monitors();
        if let Some(monitor) = monitors.item(0).and_downcast::<gdk::Monitor>() {
            window.set_monitor(Some(&monitor));
        }
    }
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }

    let entry = Entry::builder()
        .placeholder_text("Run a command... (Ctrl+Enter for terminal)")
        .primary_icon_name("utilities-terminal-symbolic")
        .width_request(440)
        .build();
    entry.add_css_class("run-command-entry");

    let list = ListBox::builder()
        .selection_mode(SelectionMode::Single)
        .activate_on_single_click(true)
        .visible(false)
        .build();
    list.add_css_class("run-completions");

    let executables = Rc::new(collect_path_executables());
    let history = Rc::new(history::load_history());
    let state = Rc::new(RefCell::new(RunState::default()));

    let container = GtkBox::new(Orientation::Vertical, 10);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Start);
    container.set_margin_top(120);
    container.set_width_request(440);
    container.append(&entry);
    container.append(&list);
    window.set_child(Some(&container));

    let changed_state = state.clone();
    let changed_list = list.clone();
    let changed_executables = executables.clone();
    entry.connect_changed(move |entry| {
        let mut current = changed_state.borrow_mut();
        if current.recalling_history {
            return;
        }
        current.draft = entry.text().to_string();
        current.history_index = -1;
        current.history_matches.clear();
        drop(current);
        show_terminal_mode(entry, false);
        render_matches(
            &entry.text(),
            &changed_list,
            &changed_executables,
            &changed_state,
        );
    });

    let activated_state = state.clone();
    let activated_window = window.clone();
    entry.connect_activate(move |entry| {
        let current = activated_state.borrow();
        let terminal_mode = current
            .history_matches
            .get(current.history_index.max(0) as usize)
            .filter(|_| current.history_index >= 0)
            .is_some_and(|item| item.in_terminal);
        drop(current);
        submit_command(&entry.text(), terminal_mode, &activated_window);
    });

    let activated_state = state.clone();
    let activated_window = window.clone();
    list.connect_row_activated(move |_, row| {
        let command = activated_state
            .borrow()
            .matches
            .get(row.index() as usize)
            .cloned();
        if let Some(command) = command {
            submit_command(&command, false, &activated_window);
        }
    });

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let key_window = window.clone();
    let key_entry = entry.clone();
    let key_list = list.clone();
    let key_state = state.clone();
    let key_history = history.clone();
    let key_executables = executables.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if (key == gdk::Key::Return || key == gdk::Key::KP_Enter)
            && modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        {
            submit_command(&key_entry.text(), true, &key_window);
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::Key::Tab {
            let current = key_state.borrow();
            let chosen = current
                .matches
                .get(current.selected_index)
                .or_else(|| current.matches.first())
                .cloned();
            drop(current);
            if let Some(chosen) = chosen {
                key_entry.set_text(&format!("{chosen} "));
                key_entry.set_position(-1);
                return gtk::glib::Propagation::Stop;
            }
        }
        if key == gdk::Key::Up
            && navigate_history(
                1,
                &key_entry,
                &key_list,
                &key_executables,
                &key_state,
                &key_history,
            )
        {
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::Key::Down
            && navigate_history(
                -1,
                &key_entry,
                &key_list,
                &key_executables,
                &key_state,
                &key_history,
            )
        {
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::Key::Down || key == gdk::Key::Up {
            let mut current = key_state.borrow_mut();
            if current.matches.is_empty() {
                return gtk::glib::Propagation::Proceed;
            }
            let count = current.matches.len();
            current.selected_index = if key == gdk::Key::Down {
                (current.selected_index + 1) % count
            } else {
                (current.selected_index + count - 1) % count
            };
            if let Some(row) = key_list.row_at_index(current.selected_index as i32) {
                key_list.select_row(Some(&row));
            }
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(keys);

    let backdrop = GestureClick::new();
    let backdrop_window = window.clone();
    backdrop.connect_released(move |_, _, x, y| {
        let picked = backdrop_window.pick(x, y, gtk::PickFlags::DEFAULT);
        if picked.is_some_and(|widget| widget == backdrop_window.clone().upcast::<gtk::Widget>()) {
            backdrop_window.close();
        }
    });
    window.add_controller(backdrop);

    let close_keys = EventControllerKey::new();
    let close_window = window.clone();
    close_keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            close_window.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(close_keys);

    let provider = CssProvider::new();
    provider.load_from_data(
        ".run-command-overlay { background: rgba(0, 0, 0, 0.28); }\
         .run-command-entry { padding: 14px 18px; border-radius: 18px; }\
         .run-completions { background: rgba(30, 30, 34, 0.94); border: 1px solid rgba(255,255,255,0.12); border-radius: 14px; padding: 6px; }\
         .run-completion-row { border-radius: 10px; padding: 6px 12px; }\
         .run-completion-row:selected { background: rgba(255,255,255,0.12); }\
         .run-completion-label { font-family: monospace; font-size: 13px; }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        if let Some(settings) = gtk::gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
            .filter(|schema| schema.has_key("accent-color"))
            .map(|schema| {
                gtk::gio::Settings::new_full(&schema, None::<&gtk::gio::SettingsBackend>, None)
            })
        {
            let accent_provider = CssProvider::new();
            let update_accent = |settings: &gtk::gio::Settings, provider: &CssProvider| {
                provider.load_from_data(&format!(
                    "entry.run-command-terminal image {{ color: {}; }}",
                    accent_color(settings)
                ));
            };
            update_accent(&settings, &accent_provider);
            let provider_copy = accent_provider.clone();
            settings.connect_changed(Some("accent-color"), move |settings, _| {
                update_accent(settings, &provider_copy);
            });
            gtk::style_context_add_provider_for_display(
                &display,
                &accent_provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
            );
            app.connect_shutdown(move |_| {
                let _ = &settings;
            });
        }
    }

    window.present();
    entry.grab_focus();
}

pub fn run() -> gtk::glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_window);
    app.run_with_args::<&str>(&[])
}
