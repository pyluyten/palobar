use crate::config::{read_config, update_config};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::process::Command;
use std::rc::Rc;

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct NiriWindow {
    id: u64,
    app_id: Option<String>,
    title: Option<String>,
    pid: Option<u32>,
    is_focused: bool,
}

impl NiriWindow {
    fn app_name(&self) -> &str {
        self.app_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .or(self.title.as_deref())
            .unwrap_or("")
    }
}

#[derive(Deserialize)]
enum WindowEvent {
    WindowsChanged { windows: Vec<NiriWindow> },
    WindowOpenedOrChanged { window: NiriWindow },
    WindowClosed { id: u64 },
    WindowFocusChanged { id: Option<u64> },
}

fn app_key(id: &str) -> String {
    let lower = id.trim().to_lowercase();
    lower.strip_suffix(".desktop").unwrap_or(&lower).to_owned()
}

fn desktop_id_is_valid(id: &str) -> bool {
    id.ends_with(".desktop") && !id.contains(['/', '\0', '\n', '\r'])
}

fn desktop_app(name: &str, installed: &[gio::DesktopAppInfo]) -> Option<gio::DesktopAppInfo> {
    let key = app_key(name);
    for id in [
        format!("{}.desktop", name.trim_end_matches(".desktop")),
        format!("{key}.desktop"),
    ] {
        if desktop_id_is_valid(&id) {
            if let Some(app) = gio::DesktopAppInfo::new(&id) {
                return Some(app);
            }
        }
    }
    installed
        .iter()
        .find(|app| {
            app.id().is_some_and(|id| app_key(&id) == key)
                || app
                    .startup_wm_class()
                    .is_some_and(|class| app_key(&class) == key)
                || app_key(&app.display_name()) == key
        })
        .cloned()
}

fn favorites(config: &Value) -> Vec<String> {
    let mut seen = HashSet::new();
    config
        .get("favoriteApps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| desktop_id_is_valid(id) && seen.insert(*id))
        .map(str::to_owned)
        .collect()
}

fn set_favorite(file: &gio::File, id: &str, add: bool) -> Result<(), Box<dyn std::error::Error>> {
    update_config(file, |config| {
        let mut ids = favorites(config);
        let exists = ids.iter().any(|favorite| favorite == id);
        if exists == add {
            return Ok(false);
        }
        if add {
            ids.push(id.to_owned());
        } else {
            ids.retain(|favorite| favorite != id);
        }
        config["favoriteApps"] = json!(ids);
        Ok(true)
    })
}

fn report(error: impl std::fmt::Display) {
    eprintln!("[rust-dock] {error}");
}

fn run_command(program: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "{program} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn run_action(
    window: &gtk::ApplicationWindow,
    close: bool,
    action: impl FnOnce() -> Result<(), String> + Send + 'static,
) {
    let Some(app) = window.application() else {
        return;
    };
    let hold = app.hold();
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(action).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => report(error),
            Err(_) => report("action worker panicked"),
        }
        drop(hold);
    });
    if close {
        window.close();
    }
}

fn window_action(action: &str, id: u64) -> Result<(), String> {
    run_command("niri", &["msg", "action", action, "--id", &id.to_string()]).map(drop)
}

fn kill_window(id: u64, pid: u32) -> Result<(), String> {
    let output = run_command("niri", &["msg", "--json", "windows"])?;
    let windows: Vec<NiriWindow> =
        serde_json::from_slice(&output).map_err(|error| error.to_string())?;
    if pid <= 1
        || !windows
            .iter()
            .any(|window| window.id == id && window.pid == Some(pid))
    {
        return Err("The window no longer has a matching process".into());
    }
    run_command("kill", &["-KILL", &pid.to_string()]).map(drop)
}

fn launch(app: &gio::DesktopAppInfo, window: &gtk::ApplicationWindow, new_window: bool) {
    let context = gtk::prelude::WidgetExt::display(window).app_launch_context();
    if new_window {
        let actions = app.list_actions();
        if let Some(action) = actions
            .iter()
            .find(|action| action.to_ascii_lowercase().replace(['-', '_'], "") == "newwindow")
        {
            app.launch_action(action, Some(&context));
            window.close();
            return;
        }
    }
    match app.launch(&[], Some(&context)) {
        Ok(()) => window.close(),
        Err(error) => report(error),
    }
}

type DockRef = Rc<RefCell<Dock>>;

#[derive(PartialEq, Eq)]
struct ItemKey {
    desktop_id: Option<glib::GString>,
    window_id: Option<u64>,
    pid: Option<u32>,
    fallback_name: Option<String>,
    favorite: bool,
}

#[derive(Clone)]
pub enum ApplicationsButton {
    Hidden,
    Show,
    OpenOverview(Rc<dyn Fn()>),
    Hide,
}

struct Dock {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    container: glib::WeakRef<gtk::Box>,
    config: gio::File,
    windows: Vec<NiriWindow>,
    menu: Option<gtk::Popover>,
    rendered: Option<Vec<ItemKey>>,
    applications: ApplicationsButton,
}

fn close_menu(dock: &DockRef) {
    let menu = dock.borrow_mut().menu.take();
    if let Some(menu) = menu {
        menu.popdown();
        if menu.parent().is_some() {
            menu.unparent();
        }
    }
}

fn menu_button(
    actions: &gtk::Box,
    label: &str,
    sensitive: bool,
    dock: &DockRef,
    action: impl Fn(&DockRef) + 'static,
) {
    let button = gtk::Button::with_label(label);
    button.set_sensitive(sensitive);
    let dock = Rc::downgrade(dock);
    button.connect_clicked(move |_| {
        if let Some(dock) = dock.upgrade() {
            close_menu(&dock);
            action(&dock);
        }
    });
    actions.append(&button);
}

fn show_menu(
    dock: &DockRef,
    button: &gtk::Button,
    desktop: Option<&gio::DesktopAppInfo>,
    running: Option<&NiriWindow>,
    favorite: bool,
) {
    close_menu(dock);
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let app = desktop.cloned();
    menu_button(&actions, "New Window", app.is_some(), dock, move |dock| {
        let window = dock.borrow().window.upgrade();
        if let (Some(app), Some(window)) = (&app, window) {
            launch(app, &window, true);
        }
    });
    if let Some(id) = desktop
        .and_then(|app| app.id())
        .filter(|id| desktop_id_is_valid(id))
    {
        let label = if favorite {
            "Remove from Favorites"
        } else {
            "Add to Favorites"
        };
        menu_button(&actions, label, true, dock, move |dock| {
            update_favorite(dock, &id, !favorite);
        });
    }
    if let Some(running) = running {
        let id = running.id;
        let pid = running.pid.filter(|pid| *pid > 1);
        menu_button(&actions, "Close", true, dock, move |dock| {
            if let Some(window) = dock.borrow().window.upgrade() {
                run_action(&window, false, move || window_action("close-window", id));
            }
        });
        menu_button(&actions, "Force Kill", pid.is_some(), dock, move |dock| {
            if let (Some(window), Some(pid)) = (dock.borrow().window.upgrade(), pid) {
                run_action(&window, false, move || kill_window(id, pid));
            }
        });
    }
    let menu = gtk::Popover::builder()
        .child(&actions)
        .position(gtk::PositionType::Top)
        .build();
    menu.add_css_class("dock-menu");
    let weak_dock = Rc::downgrade(dock);
    menu.connect_closed(move |menu| {
        if let Some(dock) = weak_dock.upgrade() {
            let mut dock = dock.borrow_mut();
            if dock.menu.as_ref() == Some(menu) {
                dock.menu.take();
            }
        }
        if menu.parent().is_some() {
            menu.unparent();
        }
    });
    menu.set_parent(button);
    dock.borrow_mut().menu = Some(menu.clone());
    menu.popup();
}

fn append_app(
    container: &gtk::Box,
    dock: &DockRef,
    desktop: Option<gio::DesktopAppInfo>,
    running: Option<NiriWindow>,
    favorite: bool,
) {
    let name = desktop
        .as_ref()
        .map(|app| app.display_name().to_string())
        .or_else(|| running.as_ref().map(|window| window.app_name().to_owned()))
        .unwrap_or_else(|| "Application".to_owned());
    let icon = desktop
        .as_ref()
        .and_then(|app| app.icon())
        .map(|icon| gtk::Image::from_gicon(&icon))
        .unwrap_or_else(|| {
            let icon_name = running.as_ref().and_then(|window| window.app_id.as_deref());
            let display = gtk::prelude::WidgetExt::display(container);
            let theme = gtk::IconTheme::for_display(&display);
            gtk::Image::from_icon_name(
                icon_name
                    .filter(|name| theme.has_icon(name))
                    .unwrap_or("application-x-executable"),
            )
        });
    icon.set_pixel_size(60);
    let dot = gtk::Box::builder()
        .width_request(8)
        .height_request(8)
        .halign(gtk::Align::Center)
        .opacity(if running.is_some() { 1.0 } else { 0.0 })
        .build();
    dot.add_css_class("dock-dot");
    let item = gtk::Box::new(gtk::Orientation::Vertical, 6);
    item.append(&icon);
    item.append(&dot);
    let button = gtk::Button::builder()
        .tooltip_text(&name)
        .child(&item)
        .build();
    button.add_css_class("dock-item");

    let weak_window = dock.borrow().window.clone();
    let app = desktop.clone();
    let id = running.as_ref().map(|window| window.id);
    button.connect_clicked(move |_| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        if let Some(id) = id {
            run_action(&window, true, move || {
                run_command("niri", &["msg", "action", "close-overview"])?;
                window_action("focus-window", id)
            });
        } else if let Some(app) = &app {
            launch(app, &window, false);
        }
    });
    let gesture = gtk::GestureClick::builder()
        .button(gdk::BUTTON_SECONDARY)
        .build();
    let weak_dock = Rc::downgrade(dock);
    gesture.connect_pressed(move |gesture, _, _, _| {
        if let Some((dock, button)) = weak_dock
            .upgrade()
            .zip(gesture.widget().and_downcast::<gtk::Button>())
        {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            show_menu(&dock, &button, desktop.as_ref(), running.as_ref(), favorite);
        }
    });
    button.add_controller(gesture);
    container.append(&button);
}

fn rebuild(dock: &DockRef) {
    let Some(container) = dock.borrow().container.upgrade() else {
        return;
    };
    let (config, windows) = {
        let dock = dock.borrow();
        (dock.config.clone(), dock.windows.clone())
    };
    let mut running: Vec<(String, NiriWindow)> = Vec::new();
    for window in windows {
        let key = app_key(window.app_name());
        if key.is_empty() {
            continue;
        }
        if let Some((_, existing)) = running.iter_mut().find(|(existing, _)| *existing == key) {
            if window.is_focused {
                *existing = window;
            }
        } else {
            running.push((key, window));
        }
    }
    let ids = read_config(&config)
        .map(|(config, _)| favorites(&config))
        .unwrap_or_else(|error| {
            report(error);
            Vec::new()
        });
    let mut items = Vec::new();
    for id in ids {
        let Some(app) = gio::DesktopAppInfo::new(&id) else {
            continue;
        };
        let keys = [
            app_key(&id),
            app.startup_wm_class()
                .map(|class| app_key(&class))
                .unwrap_or_default(),
            app_key(&app.display_name()),
        ];
        let mut matched = None;
        running.retain(|(key, window)| {
            if keys.contains(key) {
                if window.is_focused || matched.is_none() {
                    matched = Some(window.clone());
                }
                false
            } else {
                true
            }
        });
        items.push((Some(app), matched, true));
    }
    let installed: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter_map(|app| app.downcast().ok())
        .collect();
    for (_, window) in running {
        let app = desktop_app(window.app_name(), &installed);
        items.push((app, Some(window), false));
    }
    let rendered = items
        .iter()
        .map(|(app, window, favorite)| ItemKey {
            desktop_id: app.as_ref().and_then(|app| app.id()),
            window_id: window.as_ref().map(|window| window.id),
            pid: window.as_ref().and_then(|window| window.pid),
            fallback_name: window
                .as_ref()
                .filter(|_| app.is_none())
                .map(|window| window.app_name().to_owned()),
            favorite: *favorite,
        })
        .collect();
    if dock.borrow().rendered.as_ref() == Some(&rendered) {
        return;
    }
    dock.borrow_mut().rendered = Some(rendered);
    close_menu(dock);
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    for (app, window, favorite) in items {
        append_app(&container, dock, app, window, favorite);
    }
    if container.first_child().is_none() {
        container.append(
            &gtk::Label::builder()
                .label("Drop an app here to add it to favorites")
                .height_request(86)
                .build(),
        );
    }
    let mode = dock.borrow().applications.clone();
    if matches!(mode, ApplicationsButton::Hidden) {
        return;
    }
    let hide = matches!(mode, ApplicationsButton::Hide);
    let image = gtk::Image::from_icon_name("view-app-grid-symbolic");
    image.set_pixel_size(48);
    let applications = gtk::ToggleButton::builder()
        .active(hide)
        .tooltip_text(if hide {
            "Hide Applications"
        } else {
            "Show Applications"
        })
        .child(&image)
        .build();
    applications.add_css_class("dock-item");
    let weak_window = dock.borrow().window.clone();
    applications.connect_clicked(move |_| {
        if let Some(window) = weak_window.upgrade() {
            match &mode {
                ApplicationsButton::Hide => window.close(),
                ApplicationsButton::OpenOverview(open) => open(),
                ApplicationsButton::Show => match crate::launcher::launch() {
                    Ok(()) => window.close(),
                    Err(error) => report(error),
                },
                ApplicationsButton::Hidden => {}
            }
        }
    });
    container.append(&applications);
}

fn update_favorite(dock: &DockRef, id: &str, add: bool) -> bool {
    let config = dock.borrow().config.clone();
    match set_favorite(&config, id, add) {
        Ok(()) => {
            rebuild(dock);
            true
        }
        Err(error) => {
            report(error);
            false
        }
    }
}

fn update_windows(dock: &DockRef, event: WindowEvent) -> bool {
    let windows = &mut dock.borrow_mut().windows;
    match event {
        WindowEvent::WindowsChanged { windows: snapshot } => {
            if *windows == snapshot {
                return false;
            }
            *windows = snapshot;
        }
        WindowEvent::WindowOpenedOrChanged { window } => {
            if let Some(existing) = windows.iter_mut().find(|existing| existing.id == window.id) {
                if *existing == window {
                    return false;
                }
                *existing = window;
            } else {
                windows.push(window);
            }
        }
        WindowEvent::WindowClosed { id } => {
            let len = windows.len();
            windows.retain(|window| window.id != id);
            return windows.len() != len;
        }
        WindowEvent::WindowFocusChanged { id: None } => return false,
        WindowEvent::WindowFocusChanged { id: Some(id) } => {
            let mut changed = false;
            for window in windows {
                let focused = window.id == id;
                changed |= window.is_focused != focused;
                window.is_focused = focused;
            }
            return changed;
        }
    }
    true
}

fn watch_windows(dock: &DockRef, window: &gtk::ApplicationWindow) -> Result<(), glib::Error> {
    let process = gio::Subprocess::newv(
        &["niri", "msg", "-j", "event-stream"].map(OsStr::new),
        gio::SubprocessFlags::STDOUT_PIPE,
    )?;
    let input = gio::DataInputStream::new(&process.stdout_pipe().expect("stdout pipe requested"));
    let dock = dock.clone();
    let reader = glib::spawn_future_local(async move {
        while let Ok(Some(line)) = input.read_line_future(glib::Priority::DEFAULT).await {
            if let Ok(event) = serde_json::from_slice(&line) {
                if update_windows(&dock, event) {
                    rebuild(&dock);
                }
            }
        }
    });
    window.connect_unrealize(move |_| {
        reader.abort();
        process.force_exit();
    });
    Ok(())
}

fn build(app: &gtk::Application) {
    if let Some(window) = app.windows().first() {
        window.close();
        return;
    }
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let Some(monitor) = display.monitors().item(0).and_downcast::<gdk::Monitor>() else {
        return;
    };

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Palobar Dock")
        .build();
    window.add_css_class("standalone-dock");
    window.init_layer_shell();
    window.set_namespace(Some("standalone-dock"));
    window.set_monitor(Some(&monitor));
    window.set_anchor(Edge::Bottom, true);
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_margin(Edge::Bottom, 24);
    let container = widget(&window, ApplicationsButton::Show);
    window.set_child(Some(&container));

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(|controller, key, _, _| {
        if key == gdk::Key::Escape {
            if let Some(window) = controller.widget().and_downcast::<gtk::ApplicationWindow>() {
                window.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);

    window.present();
}

pub fn widget(window: &gtk::ApplicationWindow, applications: ApplicationsButton) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    container.add_css_class("dock");

    let path = glib::user_config_dir().join("palobar");
    if let Err(error) = std::fs::create_dir_all(&path) {
        report(error);
    }
    let config = gio::File::for_path(path.join("config.json"));
    let dock = Rc::new(RefCell::new(Dock {
        window: window.downgrade(),
        container: container.downgrade(),
        config: config.clone(),
        windows: Vec::new(),
        menu: None,
        rendered: None,
        applications,
    }));
    let drop = gtk::DropTarget::new(String::static_type(), gdk::DragAction::COPY);
    let drop_dock = dock.clone();
    drop.connect_drop(move |_, value, _, _| {
        let Ok(id) = value.get::<String>() else {
            return false;
        };
        desktop_id_is_valid(&id)
            && gio::DesktopAppInfo::new(&id).is_some()
            && update_favorite(&drop_dock, &id, true)
    });
    container.add_controller(drop);

    match gio::File::for_path(path)
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
    {
        Ok(monitor) => {
            let weak_dock = Rc::downgrade(&dock);
            monitor.connect_changed(move |_, file, other, _| {
                if file.equal(&config) || other.is_some_and(|file| file.equal(&config)) {
                    if let Some(dock) = weak_dock.upgrade() {
                        rebuild(&dock);
                    }
                }
            });
            window.connect_unrealize(move |_| {
                monitor.cancel();
            });
        }
        Err(error) => report(error),
    }
    let close_dock = dock.clone();
    window.connect_unrealize(move |_| close_menu(&close_dock));
    rebuild(&dock);
    if let Err(error) = watch_windows(&dock, window) {
        report(error);
    }
    container
}

pub fn install_style(display: &gdk::Display) {
    let css = gtk::CssProvider::new();
    css.load_from_data(include_str!("dock.css"));
    gtk::style_context_add_provider_for_display(
        display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn run() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.Astal.Palobar.RustDock")
        .build();
    app.connect_startup(|_| {
        if let Some(display) = gdk::Display::default() {
            install_style(&display);
        }
    });
    app.connect_activate(build);
    app.run_with_args::<&str>(&[])
}
