use crate::panel_popup;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, graphene};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::time::Duration;

pub const ACTION: &str = "display-workspaces";
const CLASS: &str = "workspace-dialog-window";

#[derive(Deserialize)]
struct Workspace {
    id: u64,
    idx: u64,
    name: Option<String>,
    output: Option<String>,
}

#[derive(Deserialize)]
struct Window {
    id: u64,
    workspace_id: Option<u64>,
    title: Option<String>,
    app_id: Option<String>,
    layout: Option<Layout>,
}

#[derive(Deserialize)]
struct Layout {
    pos_in_scrolling_layout: Option<(u64, u64)>,
}

fn request(message: Value) -> Result<Value, String> {
    (|| -> Result<Value, Box<dyn std::error::Error>> {
        let socket = std::env::var_os("NIRI_SOCKET").ok_or("NIRI_SOCKET is unavailable")?;
        let mut stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        serde_json::to_writer(&mut stream, &message)?;
        stream.write_all(b"\n")?;
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply)?;
        let mut reply: Value = serde_json::from_str(&reply)?;
        if reply.get("Ok").is_some() {
            Ok(reply["Ok"].take())
        } else {
            Err(reply["Err"]
                .as_str()
                .unwrap_or("Niri rejected the request")
                .to_owned()
                .into())
        }
    })()
    .map_err(|error| error.to_string())
}

fn snapshot() -> Result<(Vec<Workspace>, Vec<Window>), String> {
    let mut workspaces = request(json!("Workspaces"))?;
    let mut windows = request(json!("Windows"))?;
    Ok((
        serde_json::from_value(workspaces["Workspaces"].take())
            .map_err(|error| error.to_string())?,
        serde_json::from_value(windows["Windows"].take()).map_err(|error| error.to_string())?,
    ))
}

type Tile = (glib::WeakRef<gtk::Button>, glib::WeakRef<gtk::Box>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum AfterAction {
    KeepOpen,
    Reload,
    Close,
}

struct Dialog {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    overlay: gtk::Overlay,
    ghosts: gtk::Fixed,
    rows: gtk::Box,
    status: gtk::Label,
    loading: Cell<bool>,
    moving: Cell<bool>,
    tiles: RefCell<HashMap<u64, Tile>>,
    closed_during_load: RefCell<HashSet<u64>>,
}

impl Dialog {
    fn status(&self, message: &str) {
        self.status.set_text(message);
        self.status.set_visible(!message.is_empty());
    }

    fn load(self: &Rc<Self>) {
        if self.loading.replace(true) {
            return;
        }
        let dialog = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(snapshot).await;
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            dialog.loading.set(false);
            match result {
                Ok(Ok((workspaces, windows))) => dialog.render(workspaces, windows),
                Ok(Err(error)) => dialog.status(&format!("Could not load workspaces. {error}")),
                Err(_) => dialog.status("Could not load workspaces."),
            }
            dialog.closed_during_load.borrow_mut().clear();
        });
    }

    fn action(self: &Rc<Self>, action: Value, message: &'static str, after: AfterAction) {
        if after == AfterAction::Reload && self.moving.replace(true) {
            return;
        }
        let dialog = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || request(json!({"Action": action}))).await;
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            if after == AfterAction::Reload {
                dialog.moving.set(false);
            }
            match result {
                Ok(Ok(_)) => {
                    dialog.status("");
                    match after {
                        AfterAction::Reload => dialog.load(),
                        AfterAction::Close => {
                            if let Some(window) = dialog.window.upgrade() {
                                window.close();
                            }
                        }
                        AfterAction::KeepOpen => {}
                    }
                }
                Ok(Err(error)) => dialog.status(&format!("{message} {error}")),
                Err(_) => dialog.status(message),
            }
        });
    }

    fn remove_window(&self, id: u64) {
        if self.loading.get() {
            self.closed_during_load.borrow_mut().insert(id);
        }
        let Some((tile, column)) = self.tiles.borrow_mut().remove(&id) else {
            return;
        };
        if let (Some(tile), Some(column)) = (tile.upgrade(), column.upgrade()) {
            column.remove(&tile);
            if column.first_child().is_none() {
                if let Some(columns) = column.parent().and_downcast::<gtk::Box>() {
                    columns.remove(&column);
                }
            }
        }
    }

    fn render(self: &Rc<Self>, mut workspaces: Vec<Workspace>, windows: Vec<Window>) {
        while let Some(row) = self.rows.first_child() {
            self.rows.remove(&row);
        }
        while let Some(ghost) = self.ghosts.first_child() {
            self.ghosts.remove(&ghost);
        }
        self.tiles.borrow_mut().clear();
        workspaces.sort_by(|a, b| a.output.cmp(&b.output).then(a.idx.cmp(&b.idx)));
        let outputs = workspaces
            .iter()
            .filter_map(|workspace| workspace.output.as_ref())
            .collect::<HashSet<_>>();
        let show_output = outputs.len() > 1;
        let mut groups: HashMap<u64, Vec<Window>> = HashMap::new();
        for window in windows {
            if let Some(id) = window.workspace_id {
                if !self.closed_during_load.borrow().contains(&window.id) {
                    groups.entry(id).or_default().push(window);
                }
            }
        }
        self.status(if workspaces.is_empty() {
            "No workspaces available."
        } else {
            ""
        });
        for workspace in workspaces {
            let windows = groups.remove(&workspace.id).unwrap_or_default();
            self.append_workspace(workspace, windows, show_output);
        }
    }

    fn append_workspace(
        self: &Rc<Self>,
        workspace: Workspace,
        mut windows: Vec<Window>,
        show_output: bool,
    ) {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.add_css_class("workspace-dialog-row");
        section.set_widget_name(&format!("workspace-destination-{}", workspace.id));
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("workspace-dialog-header");
        let output = if show_output {
            workspace
                .output
                .as_deref()
                .map(|output| format!(" · {output}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let fallback = format!("Workspace {}", workspace.idx);
        let heading = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        heading.set_text(&format!(
            "{}{output}",
            workspace.name.as_deref().unwrap_or(&fallback)
        ));
        let entry = gtk::Entry::builder()
            .hexpand(true)
            .placeholder_text("Workspace name")
            .visible(false)
            .build();
        entry.set_text(workspace.name.as_deref().unwrap_or_default());
        let rename = button("Rename");
        let revert = gtk::Button::from_icon_name("edit-undo-symbolic");
        revert.add_css_class("workspace-dialog-button");
        revert.set_tooltip_text(Some("Revert"));
        revert.update_property(&[gtk::accessible::Property::Label("Revert workspace name")]);
        revert.set_sensitive(workspace.name.is_some());
        let cancel = button("Cancel");
        cancel.set_visible(false);
        let stop_editing = {
            let (heading, entry, rename, cancel) = (
                heading.downgrade(),
                entry.downgrade(),
                rename.downgrade(),
                cancel.downgrade(),
            );
            move || {
                if let (Some(heading), Some(entry), Some(rename), Some(cancel)) = (
                    heading.upgrade(),
                    entry.upgrade(),
                    rename.upgrade(),
                    cancel.upgrade(),
                ) {
                    heading.set_visible(true);
                    entry.set_visible(false);
                    cancel.set_visible(false);
                    rename.set_label("Rename");
                }
            }
        };
        let current = Rc::new(RefCell::new(workspace.name));
        let change_name = {
            let (header, heading, revert) =
                (header.downgrade(), heading.downgrade(), revert.downgrade());
            let dialog = Rc::downgrade(self);
            let stop_editing = stop_editing.clone();
            let current = current.clone();
            let id = workspace.id;
            move |name: Option<String>| {
                let Some(header_widget) = header.upgrade() else {
                    return;
                };
                if !header_widget.is_sensitive() {
                    return;
                }
                if *current.borrow() == name {
                    stop_editing();
                    return;
                }
                let (action, error_message) = match &name {
                    Some(name) => (
                        json!({"SetWorkspaceName": {"name": name, "workspace": {"Id": id}}}),
                        "Could not rename the workspace.",
                    ),
                    None => (
                        json!({"UnsetWorkspaceName": {"reference": {"Id": id}}}),
                        "Could not revert the workspace name.",
                    ),
                };
                header_widget.set_sensitive(false);
                let (header, heading, revert, dialog, current, stop_editing) = (
                    header.clone(),
                    heading.clone(),
                    revert.clone(),
                    dialog.clone(),
                    current.clone(),
                    stop_editing.clone(),
                );
                let title = format!("{}{output}", name.as_deref().unwrap_or(&fallback));
                glib::spawn_future_local(async move {
                    let result =
                        gio::spawn_blocking(move || request(json!({"Action": action}))).await;
                    let (Some(header), Some(dialog)) = (header.upgrade(), dialog.upgrade()) else {
                        return;
                    };
                    header.set_sensitive(true);
                    match result {
                        Ok(Ok(_)) => {
                            if let Some(heading) = heading.upgrade() {
                                heading.set_text(&title);
                            }
                            if let Some(revert) = revert.upgrade() {
                                revert.set_sensitive(name.is_some());
                            }
                            let reverted = name.is_none();
                            current.replace(name);
                            stop_editing();
                            dialog.status("");
                            if reverted {
                                dialog.load();
                            }
                        }
                        Ok(Err(error)) => dialog.status(&format!("{error_message} {error}")),
                        Err(_) => dialog.status(error_message),
                    }
                });
            }
        };
        revert.connect_clicked({
            let change_name = change_name.clone();
            move |_| change_name(None)
        });
        let save = move |entry: &gtk::Entry| {
            let text = entry.text();
            let name = text.trim();
            if !name.is_empty() {
                change_name(Some(name.to_owned()));
            }
        };
        entry.connect_activate({
            let save = save.clone();
            move |entry| save(entry)
        });
        cancel.connect_clicked(move |_| stop_editing());
        rename.connect_clicked({
            let (entry, heading, cancel) =
                (entry.downgrade(), heading.downgrade(), cancel.downgrade());
            move |rename| {
                let (Some(entry), Some(heading), Some(cancel)) =
                    (entry.upgrade(), heading.upgrade(), cancel.upgrade())
                else {
                    return;
                };
                if WidgetExt::is_visible(&entry) {
                    save(&entry);
                    return;
                }
                entry.set_text(current.borrow().as_deref().unwrap_or_default());
                entry.set_visible(true);
                heading.set_visible(false);
                cancel.set_visible(true);
                rename.set_label("Save");
                entry.grab_focus();
                entry.select_region(0, -1);
            }
        });
        header.append(&heading);
        header.append(&entry);
        header.append(&rename);
        header.append(&revert);
        header.append(&cancel);
        section.append(&header);

        let columns = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        columns.add_css_class("workspace-dialog-columns");
        let position = |window: &Window| {
            window
                .layout
                .as_ref()
                .and_then(|layout| layout.pos_in_scrolling_layout)
        };
        windows.sort_by_key(|window| (position(window).unwrap_or((u64::MAX, 0)), window.id));
        let mut previous = None;
        let mut column = gtk::Box::new(gtk::Orientation::Vertical, 8);
        for window in windows {
            let index = position(&window).map(|(column, _)| column);
            if column.parent().is_none() || index.is_none() || index != previous {
                column = gtk::Box::new(gtk::Orientation::Vertical, 8);
                columns.append(&column);
            }
            previous = index;
            self.append_window(&column, window);
        }
        section.append(
            &gtk::ScrolledWindow::builder()
                .child(&columns)
                .vscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .build(),
        );
        self.rows.append(&section);
    }

    fn append_window(self: &Rc<Self>, column: &gtk::Box, window: Window) {
        let title = window
            .title
            .as_deref()
            .filter(|title| !title.is_empty())
            .or(window.app_id.as_deref())
            .unwrap_or("Window");
        let tile = gtk::Button::builder()
            .tooltip_text(title)
            .child(&window_icon(&window, &self.overlay.display()))
            .build();
        tile.add_css_class("workspace-window-tile");
        tile.update_property(&[gtk::accessible::Property::Label(title)]);
        tile.set_cursor_from_name(Some("grab"));
        let id = window.id;
        let workspace = window.workspace_id;
        tile.connect_clicked({
            let dialog = Rc::downgrade(self);
            move |_| {
                let Some(dialog) = dialog.upgrade() else {
                    return;
                };
                if dialog.moving.get() || dialog.loading.get() {
                    return;
                }
                dialog.action(
                    json!({"FocusWindow": {"id": id}}),
                    "Could not focus the window.",
                    AfterAction::Close,
                );
            }
        });
        let drag = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_PRIMARY)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let ghost = Rc::new(RefCell::new(None::<gtk::Image>));
        let clear = {
            let ghost = ghost.clone();
            let overlay = self.ghosts.downgrade();
            move |drag: &gtk::GestureDrag| {
                let Some(image) = ghost.take() else {
                    return false;
                };
                if let Some(layer) = overlay.upgrade() {
                    if image.parent().is_some() {
                        layer.remove(&image);
                    }
                }
                if let Some(tile) = drag.widget() {
                    tile.set_opacity(1.0);
                }
                true
            }
        };
        drag.connect_drag_update({
            let dialog = Rc::downgrade(self);
            let ghost = ghost.clone();
            move |drag, dx, dy| {
                let (Some(dialog), Some(tile), Some((x, y))) =
                    (dialog.upgrade(), drag.widget(), drag.start_point())
                else {
                    return;
                };
                if dialog.moving.get() || dialog.loading.get() {
                    return;
                }
                let Some(point) = tile.compute_point(
                    &dialog.overlay,
                    &graphene::Point::new((x + dx) as f32, (y + dy) as f32),
                ) else {
                    return;
                };
                let (left, top) = (f64::from(point.x()) - 18.0, f64::from(point.y()) - 18.0);
                let current_ghost = ghost.borrow().clone();
                if let Some(image) = current_ghost {
                    dialog.ghosts.move_(&image, left, top);
                } else if tile.drag_check_threshold(
                    x as i32,
                    y as i32,
                    (x + dx) as i32,
                    (y + dy) as i32,
                ) {
                    drag.set_state(gtk::EventSequenceState::Claimed);
                    let icon = window_icon(&window, &tile.display());
                    dialog.ghosts.put(&icon, left, top);
                    ghost.replace(Some(icon));
                    tile.set_opacity(0.45);
                }
            }
        });
        drag.connect_drag_end({
            let dialog = Rc::downgrade(self);
            let clear = clear.clone();
            move |drag, dx, dy| {
                if !clear(drag) || !drag.current_event().is_some_and(|event| {
                    matches!(event.event_type(), gdk::EventType::ButtonRelease | gdk::EventType::TouchEnd)
                }) { return; }
                let (Some(dialog), Some(tile), Some((x, y))) = (dialog.upgrade(), drag.widget(), drag.start_point()) else { return };
                if dialog.moving.get() || dialog.loading.get() { return; }
                let Some(point) = tile.compute_point(&dialog.overlay, &graphene::Point::new((x + dx) as f32, (y + dy) as f32)) else { return };
                let mut picked = dialog.overlay.pick(point.x().into(), point.y().into(), gtk::PickFlags::DEFAULT);
                let mut destination = None;
                while let Some(widget) = picked {
                    if let Some(id) = widget.widget_name().strip_prefix("workspace-destination-").and_then(|id| id.parse::<u64>().ok()) {
                        destination = Some(id);
                        break;
                    }
                    picked = widget.parent();
                }
                match destination {
                    Some(target) if Some(target) != workspace => dialog.action(json!({
                        "MoveWindowToWorkspace": {"window_id": id, "reference": {"Id": target}, "focus": false}
                    }), "Could not move the window.", AfterAction::Reload),
                    None => dialog.action(json!({"CloseWindow": {"id": id}}), "Could not close the window.", AfterAction::KeepOpen),
                    _ => {}
                }
            }
        });
        drag.connect_cancel(move |drag, _| {
            clear(drag);
        });
        tile.add_controller(drag);
        column.append(&tile);
        self.tiles
            .borrow_mut()
            .insert(id, (tile.downgrade(), column.downgrade()));
    }
}

fn window_icon(window: &Window, display: &gdk::Display) -> gtk::Image {
    let app = window.app_id.as_deref().and_then(|id| {
        if id.ends_with(".desktop") {
            gio::DesktopAppInfo::new(id)
        } else {
            gio::DesktopAppInfo::new(&format!("{id}.desktop"))
        }
    });
    let theme = gtk::IconTheme::for_display(display);
    let image = if let Some(icon) = app.and_then(|app| app.icon()) {
        gtk::Image::from_gicon(&icon)
    } else {
        gtk::Image::from_icon_name(
            window
                .app_id
                .as_deref()
                .filter(|id| theme.has_icon(id))
                .unwrap_or("application-x-executable"),
        )
    };
    image.set_pixel_size(36);
    image
}

fn button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("workspace-dialog-button");
    button
}

fn watch_closed(window: &gtk::ApplicationWindow, dialog: &Rc<Dialog>) {
    let args = ["niri", "msg", "--json", "event-stream"].map(OsStr::new);
    let Ok(process) = gio::Subprocess::newv(&args, gio::SubprocessFlags::STDOUT_PIPE) else {
        return;
    };
    let Some(stdout) = process.stdout_pipe() else {
        return;
    };
    let input = gio::DataInputStream::new(&stdout);
    window.connect_unrealize(move |_| process.force_exit());
    let dialog = Rc::downgrade(dialog);
    glib::spawn_future_local(async move {
        while let Ok(Some(line)) = input.read_line_future(glib::Priority::DEFAULT).await {
            let Some(dialog) = dialog.upgrade() else {
                break;
            };
            if let Ok(event) = serde_json::from_slice::<Value>(&line) {
                if let Some(id) = event["WindowClosed"]["id"].as_u64() {
                    dialog.remove_window(id);
                }
            }
        }
    });
}

pub fn close(app: &gtk::Application) {
    panel_popup::close(app, CLASS);
}

pub fn open(app: &gtk::Application, monitor: &gdk::Monitor) {
    if let Some(window) = app
        .windows()
        .into_iter()
        .find(|window| window.has_css_class(CLASS))
    {
        if window.monitor().as_ref() == Some(monitor) {
            window.present();
            return;
        }
        window.close();
    }
    crate::calendar::close(app);
    crate::clipboard::close(app);
    crate::quick_settings::close(app);
    let geometry = monitor.geometry();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Workspaces")
        .default_width(580.min(geometry.width()))
        .default_height(500.min(geometry.height()))
        .build();
    window.add_css_class(CLASS);
    window.init_layer_shell();
    window.set_namespace(Some("palobar-workspaces"));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("workspace-dialog");
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::builder()
        .label("Workspaces")
        .xalign(0.0)
        .hexpand(true)
        .build();
    title.add_css_class("workspace-dialog-title");
    heading.append(&title);
    let dismiss = gtk::Button::from_icon_name("window-close-symbolic");
    dismiss.add_css_class("workspace-dialog-button");
    dismiss.set_tooltip_text(Some("Close"));
    dismiss.connect_clicked({
        let window = window.downgrade();
        move |_| {
            if let Some(window) = window.upgrade() {
                window.close();
            }
        }
    });
    heading.append(&dismiss);
    card.append(&heading);
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.append(
        &gtk::ScrolledWindow::builder()
            .child(&rows)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    );
    let status = gtk::Label::builder()
        .label("Loading workspaces…")
        .wrap(true)
        .build();
    status.add_css_class("workspace-dialog-status");
    card.append(&status);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&card));
    let ghosts = gtk::Fixed::builder().can_target(false).build();
    overlay.add_overlay(&ghosts);
    window.set_child(Some(&overlay));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|keys, key, _, _| {
        if key != gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(window) = keys.widget().and_downcast::<gtk::ApplicationWindow>() {
            window.close();
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
    let dialog = Rc::new(Dialog {
        window: window.downgrade(),
        overlay,
        ghosts,
        rows,
        status,
        loading: Cell::default(),
        moving: Cell::default(),
        tiles: RefCell::default(),
        closed_during_load: RefCell::default(),
    });
    watch_closed(&window, &dialog);
    window.connect_unrealize({
        let dialog = dialog.clone();
        move |_| {
            dialog.tiles.borrow_mut().clear();
        }
    });
    window.present();
    dialog.load();
}

fn open_monitor(app: &gtk::Application, index: u32) -> glib::ExitCode {
    if let Some(display) = gdk::Display::default() {
        let monitors = display.monitors();
        if let Some(monitor) = monitors
            .item(index)
            .or_else(|| monitors.item(0))
            .and_downcast::<gdk::Monitor>()
        {
            open(app, &monitor);
            return glib::ExitCode::SUCCESS;
        }
    }
    glib::ExitCode::FAILURE
}

pub fn register_panel_action(app: &gtk::Application) {
    let action = gio::SimpleAction::new(ACTION, Some(glib::VariantTy::UINT32));
    let app_ref = app.downgrade();
    action.connect_activate(move |_, arg| {
        if let (Some(app), Some(index)) =
            (app_ref.upgrade(), arg.and_then(|value| value.get::<u32>()))
        {
            open_monitor(&app, index);
        }
    });
    app.add_action(&action);
}

pub fn install_style(display: &gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("workspace_dialog.css"));
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn run(args: &[OsString]) -> glib::ExitCode {
    let index: Option<u32> = match args {
        [] => Some(0),
        [index] => index.to_str().and_then(|value| value.parse().ok()),
        _ => None,
    };
    let Some(index) = index else {
        eprintln!("Usage: palobar display-workspaces [MONITOR]");
        return glib::ExitCode::FAILURE;
    };
    let panel_id = crate::bar::APP_ID;
    if let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        let parameters = (
            ACTION,
            vec![index.to_variant()],
            HashMap::<String, glib::Variant>::new(),
        );
        if bus
            .call_sync(
                Some(panel_id),
                &format!("/{}", panel_id.replace('.', "/")),
                "org.gtk.Actions",
                "Activate",
                Some(&parameters.to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1000,
                gio::Cancellable::NONE,
            )
            .is_ok()
        {
            return glib::ExitCode::SUCCESS;
        }
    }
    let app = gtk::Application::builder()
        .application_id("io.Astal.Palobar.RustWorkspaces")
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_startup(|_| {
        if let Some(display) = gdk::Display::default() {
            install_style(&display);
        }
    });
    app.connect_command_line(|app, command| {
        let index = command
            .arguments()
            .get(1)
            .and_then(|arg| arg.to_str()?.parse().ok())
            .unwrap_or_default();
        open_monitor(app, index)
    });
    app.run_with_args(&["palobar", &index.to_string()])
}
