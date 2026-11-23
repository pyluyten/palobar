use crate::quick_settings;
use chrono::Local;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CenterBox, CssProvider,
    EventControllerKey, EventControllerScroll, GestureClick, Image, Label, Orientation,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const APP_ID: &str = "io.Astal.Palobar.RustBar";
const PANEL_HEIGHT: i32 = 34;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Workspace {
    id: i64,
    idx: i64,
    name: Option<String>,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_urgent: bool,
}

#[derive(Deserialize)]
struct BarState {
    active_popup: Option<String>,
    unread_count: usize,
}

fn run_ctl(request: &str) -> Option<String> {
    let output = Command::new("palobar-ctl").arg(request).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn open_workspace_dialog(monitor_index: u32) {
    let index = monitor_index.to_string();
    thread::spawn(move || {
        if let Err(error) = Command::new("palobar-ctl")
            .args(["display-workspaces", &index])
            .status()
        {
            eprintln!("[rust-bar] could not open workspace dialog: {error}");
        }
    });
}

fn send_niri_action(action: &str, args: &[&str]) {
    let action = action.to_string();
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    thread::spawn(move || {
        if let Err(error) = Command::new("niri")
            .args(["msg", "action", &action])
            .args(args)
            .status()
        {
            eprintln!("[rust-bar] could not run niri action {action}: {error}");
        }
    });
}

fn query_workspaces() -> Vec<Workspace> {
    let Ok(output) = Command::new("niri")
        .args(["msg", "--json", "workspaces"])
        .output()
    else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<Workspace>>(&output.stdout)
        .map(sorted)
        .unwrap_or_default()
}

fn sorted(mut workspaces: Vec<Workspace>) -> Vec<Workspace> {
    workspaces.sort_by_key(|workspace| workspace.idx);
    workspaces
}

fn publish_workspaces(state: &Arc<Mutex<Vec<Workspace>>>, workspaces: Vec<Workspace>) {
    if let Ok(mut current) = state.lock() {
        *current = sorted(workspaces);
    }
}

fn start_workspace_event_stream(state: Arc<Mutex<Vec<Workspace>>>) {
    thread::spawn(move || loop {
        let child = Command::new("niri")
            .args(["msg", "-j", "event-stream"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        if let Ok(mut child) = child {
            if let Some(stdout) = child.stdout.take() {
                let mut workspaces = state.lock().map(|list| list.clone()).unwrap_or_default();
                for line in BufReader::new(stdout).lines().flatten() {
                    let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };

                    if let Some(changed) = event.get("WorkspacesChanged") {
                        if let Some(items) = changed.get("workspaces") {
                            if let Ok(items) =
                                serde_json::from_value::<Vec<Workspace>>(items.clone())
                            {
                                workspaces = sorted(items);
                                publish_workspaces(&state, workspaces.clone());
                            }
                        }
                    } else if let Some(activated) = event.get("WorkspaceActivated") {
                        let id = activated.get("id").and_then(|value| value.as_i64());
                        let focused = activated
                            .get("focused")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false);
                        if let Some(id) = id {
                            for workspace in &mut workspaces {
                                if workspace.id == id {
                                    workspace.is_active = true;
                                }
                                if focused {
                                    workspace.is_focused = workspace.id == id;
                                }
                            }
                            publish_workspaces(&state, workspaces.clone());
                        }
                    }
                }
            }
            let _ = child.wait();
        }
        thread::sleep(Duration::from_secs(1));
    });
}

fn workspace_button(workspace: &Workspace) -> Button {
    let button = Button::new();
    button.add_css_class("workspace-pill");
    if workspace.is_active {
        button.add_css_class("active");
    }
    if workspace.is_focused {
        button.add_css_class("focused");
    }
    if workspace.is_urgent {
        button.add_css_class("urgent");
    }
    button.set_valign(gtk::Align::Center);
    let fallback_tooltip = format!("Workspace {}", workspace.idx);
    button.set_tooltip_text(Some(workspace.name.as_deref().unwrap_or(&fallback_tooltip)));

    let dot = GtkBox::new(Orientation::Horizontal, 0);
    dot.add_css_class("workspace-dot");
    button.set_child(Some(&dot));

    let index = workspace.idx.to_string();
    button.connect_clicked(move |_| send_niri_action("focus-workspace", &[&index]));
    button
}

fn refresh_workspace_widgets(box_: &GtkBox, state: &Arc<Mutex<Vec<Workspace>>>) {
    let workspaces = state.lock().map(|list| list.clone()).unwrap_or_default();
    let mut child = box_.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        box_.remove(&widget);
    }
    for workspace in &workspaces {
        box_.append(&workspace_button(workspace));
    }
}

fn format_date() -> String {
    let now = Local::now();
    format!(
        "{} {} {}  {}",
        now.format("%a"),
        now.format("%b"),
        now.format("%-d"),
        now.format("%H:%M")
    )
}

fn image_button(icon: &str, tooltip: &str, class: &str, request: &str) -> Button {
    let button = Button::new();
    button.add_css_class("panel-button");
    button.add_css_class(class);
    button.set_tooltip_text(Some(tooltip));
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(15);
    button.set_child(Some(&image));
    let request = request.to_string();
    button.connect_clicked(move |_| {
        let _ = run_ctl(&request);
    });
    button
}

fn system_indicator_image(icon: &str) -> Image {
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(15);
    image
}

fn network_icon() -> &'static str {
    let Ok(output) = Command::new("nmcli")
        .args(["-t", "-f", "TYPE,STATE", "device"])
        .output()
    else {
        return "network-offline-symbolic";
    };
    let devices = String::from_utf8_lossy(&output.stdout);
    let rows: Vec<_> = devices.lines().collect();
    if rows
        .iter()
        .any(|row| row.ends_with(":connected") && row.starts_with("ethernet:"))
    {
        return "network-wired-symbolic";
    }
    if rows
        .iter()
        .any(|row| row.ends_with(":connected") && row.starts_with("wifi:"))
    {
        let signal = Command::new("nmcli")
            .args(["-t", "-f", "IN-USE,SIGNAL", "device", "wifi"])
            .output()
            .ok()
            .map(|result| String::from_utf8_lossy(&result.stdout).to_string())
            .and_then(|text| {
                text.lines().find_map(|line| {
                    let (active, strength) = line.split_once(':')?;
                    (active == "*")
                        .then(|| strength.parse::<u8>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        return match signal {
            75..=100 => "network-wireless-signal-excellent-symbolic",
            50..=74 => "network-wireless-signal-good-symbolic",
            25..=49 => "network-wireless-signal-ok-symbolic",
            1..=24 => "network-wireless-signal-weak-symbolic",
            _ => "network-wireless-signal-none-symbolic",
        };
    }
    "network-offline-symbolic"
}

fn volume_icon() -> &'static str {
    let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
    else {
        return "audio-volume-medium-symbolic";
    };
    quick_settings::volume_icon(&String::from_utf8_lossy(&output.stdout))
}

fn style_provider() -> CssProvider {
    let provider = CssProvider::new();
    provider.load_from_data(
        "window.panel-window { background: transparent; color: #f6f5f4; font-family: Cantarell, Inter, sans-serif; font-size: 13.5px; font-weight: 600; }\
         .panel { background-color: rgba(19, 19, 22, 0.92); min-height: 30px; padding: 2px 8px; border-bottom: 1px solid rgba(255,255,255,0.08); }\
         .panel-button { background: transparent; border: none; border-radius: 999px; padding: 2px 10px; color: #f6f5f4; }\
         .panel-button:hover { background-color: rgba(255,255,255,0.14); }\
         .panel-button.open { background-color: rgba(255,255,255,0.22); }\
         .panel-left, .panel-right { background: transparent; }\
         .workspaces { padding: 3px 8px; border-radius: 999px; }\
         .workspaces:hover { background-color: rgba(255,255,255,0.14); }\
         .workspace-pill { background: transparent; background-image: none; box-shadow: none; border: none; border-radius: 999px; padding: 3px 4px; min-width: 0; min-height: 0; }\
         .workspace-dot { min-width: 7px; min-height: 7px; border-radius: 999px; background-color: rgba(255,255,255,0.42); }\
         .workspace-pill:hover .workspace-dot { background-color: rgba(255,255,255,0.75); }\
         .workspace-pill.active .workspace-dot { background-color: rgba(255,255,255,0.65); }\
         .workspace-pill.focused .workspace-dot { min-width: 24px; min-height: 7px; background-color: #f6f5f4; }\
         .workspace-pill.urgent .workspace-dot { background-color: #e01b24; }\
         .system-menu-button image, .clipboard-button image { -gtk-icon-size: 15px; }\
         .notif-badge { background-color: #3584e4; color: #ffffff; border-radius: 999px; padding: 0 6px; font-size: 11px; font-weight: 700; }",
    );
    provider
}

fn build_bar(
    app: &Application,
    monitor: &gdk::Monitor,
    index: u32,
    workspaces: Arc<Mutex<Vec<Workspace>>>,
) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Palobar")
        .default_width(900)
        .default_height(PANEL_HEIGHT)
        .build();
    window.add_css_class("panel-window");
    window.init_layer_shell();
    window.set_namespace(Some(&format!("palobar-rust-panel-{index}")));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.auto_exclusive_zone_enable();

    let left = GtkBox::new(Orientation::Horizontal, 8);
    left.add_css_class("panel-left");
    let workspace_box = GtkBox::new(Orientation::Horizontal, 2);
    workspace_box.add_css_class("workspaces");
    left.append(&workspace_box);
    refresh_workspace_widgets(&workspace_box, &workspaces);

    let right_click = GestureClick::new();
    right_click.set_button(gdk::BUTTON_SECONDARY);
    right_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    right_click.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        open_workspace_dialog(index);
    });
    workspace_box.add_controller(right_click);

    let scroll = EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll(|_, _, dy| {
        if dy == 0.0 {
            return gtk::glib::Propagation::Proceed;
        }
        send_niri_action(
            if dy < 0.0 {
                "focus-workspace-up"
            } else {
                "focus-workspace-down"
            },
            &[],
        );
        gtk::glib::Propagation::Stop
    });
    workspace_box.add_controller(scroll);

    let clock = Button::new();
    clock.add_css_class("panel-button");
    clock.add_css_class("clock-button");
    clock.set_tooltip_text(Some("Calendar and Notifications"));
    let clock_content = GtkBox::new(Orientation::Horizontal, 12);
    let badge = Label::new(None);
    badge.add_css_class("notif-badge");
    badge.set_visible(false);
    let date = Label::new(Some(&format_date()));
    clock_content.append(&badge);
    clock_content.append(&date);
    clock.set_child(Some(&clock_content));
    let weak_app = app.downgrade();
    clock.connect_clicked(move |button| {
        if let Some(app) = weak_app.upgrade() {
            quick_settings::close(&app);
        }
        let _ = run_ctl("toggle-calendar");
        button.add_css_class("open");
    });

    let right = GtkBox::new(Orientation::Horizontal, 4);
    right.add_css_class("panel-right");
    let clipboard = image_button(
        "edit-paste-symbolic",
        "Clipboard History",
        "clipboard-button",
        "toggle-clipboard",
    );
    let system_menu = Button::new();
    system_menu.add_css_class("panel-button");
    system_menu.add_css_class("system-menu-button");
    system_menu.set_tooltip_text(Some("Quick Settings"));
    let indicators = GtkBox::new(Orientation::Horizontal, 8);
    let network = system_indicator_image(&network_icon());
    let volume = system_indicator_image(&volume_icon());
    let session = system_indicator_image("system-shutdown-symbolic");
    indicators.append(&network);
    indicators.append(&volume);
    indicators.append(&session);
    system_menu.set_child(Some(&indicators));
    let weak_app = app.downgrade();
    let target_monitor = monitor.clone();
    system_menu.connect_clicked(move |button| {
        if let Some(app) = weak_app.upgrade() {
            quick_settings::toggle(&app, &target_monitor, Some(button));
        }
    });
    let weak_app = app.downgrade();
    clipboard.connect_clicked(move |_| {
        if let Some(app) = weak_app.upgrade() {
            quick_settings::close(&app);
        }
    });
    right.append(&clipboard);
    right.append(&system_menu);

    let center = CenterBox::new();
    center.add_css_class("panel");
    center.set_start_widget(Some(&left));
    center.set_center_widget(Some(&clock));
    center.set_end_widget(Some(&right));
    window.set_child(Some(&center));

    let state_copy = workspaces.clone();
    let workspace_box_copy = workspace_box.clone();
    let mut rendered_workspaces = workspaces
        .lock()
        .map(|list| list.clone())
        .unwrap_or_default();
    gtk::glib::timeout_add_local(Duration::from_millis(120), move || {
        let current = state_copy.lock().map(|list| list.clone()).ok();
        if let Some(current) = current {
            if current != rendered_workspaces {
                rendered_workspaces = current;
                refresh_workspace_widgets(&workspace_box_copy, &state_copy);
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    let date_copy = date.clone();
    gtk::glib::timeout_add_local(Duration::from_secs(10), move || {
        date_copy.set_text(&format_date());
        gtk::glib::ControlFlow::Continue
    });

    let network_copy = network.clone();
    let volume_copy = volume.clone();
    gtk::glib::timeout_add_local(Duration::from_secs(4), move || {
        network_copy.set_icon_name(Some(network_icon()));
        volume_copy.set_icon_name(Some(volume_icon()));
        gtk::glib::ControlFlow::Continue
    });

    let badge_copy = badge.clone();
    let clock_copy = clock.clone();
    let clipboard_copy = clipboard.clone();
    gtk::glib::timeout_add_local(Duration::from_secs(1), move || {
        if let Some(state) =
            run_ctl("get-bar-state").and_then(|text| serde_json::from_str::<BarState>(&text).ok())
        {
            badge_copy.set_text(&state.unread_count.to_string());
            badge_copy.set_visible(state.unread_count > 0);
            clock_copy.set_css_classes(if state.active_popup.as_deref() == Some("calendar") {
                &["panel-button", "clock-button", "open"]
            } else {
                &["panel-button", "clock-button"]
            });
            clipboard_copy.set_css_classes(if state.active_popup.as_deref() == Some("clipboard") {
                &["panel-button", "clipboard-button", "open"]
            } else {
                &["panel-button", "clipboard-button"]
            });
        }
        gtk::glib::ControlFlow::Continue
    });

    let keys = EventControllerKey::new();
    let window_copy = window.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            window_copy.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.present();
}

pub fn run() -> gtk::glib::ExitCode {
    let initial_workspaces = query_workspaces();
    let workspaces = Arc::new(Mutex::new(initial_workspaces));
    start_workspace_event_stream(workspaces.clone());

    let app = Application::builder().application_id(APP_ID).build();
    quick_settings::register_panel_action(&app);
    let state = workspaces.clone();
    app.connect_activate(move |app| {
        if !app.windows().is_empty() {
            return;
        }
        let Some(display) = gdk::Display::default() else {
            eprintln!("[rust-bar] no GDK display available");
            app.quit();
            return;
        };
        quick_settings::install_style(&display);
        let provider = style_provider();
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let monitors = display.monitors();
        for index in 0..monitors.n_items() {
            let Some(monitor) = monitors.item(index).and_downcast::<gdk::Monitor>() else {
                continue;
            };
            build_bar(app, &monitor, index, state.clone());
        }
    });
    app.run_with_args::<&str>(&[])
}
