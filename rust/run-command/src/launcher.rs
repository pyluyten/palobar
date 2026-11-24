use crate::dock::{self, ApplicationsButton};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::rc::Rc;

const APP_ID: &str = "io.Astal.Palobar.RustLauncher";

struct App {
    desktop: gio::DesktopAppInfo,
    id: glib::GString,
    name: glib::GString,
    folded: String,
    terms: Vec<String>,
}

fn installed_apps() -> Vec<App> {
    let mut apps: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter(|app| app.should_show())
        .filter_map(|app| {
            let desktop = app.downcast::<gio::DesktopAppInfo>().ok()?;
            let id = desktop.id()?;
            let name = desktop.display_name();
            let mut terms = vec![
                id.to_lowercase(),
                desktop.executable().to_string_lossy().to_lowercase(),
            ];
            terms.extend(
                desktop
                    .keywords()
                    .into_iter()
                    .map(|value| value.to_lowercase()),
            );
            terms.extend(desktop.generic_name().map(|value| value.to_lowercase()));
            terms.extend(desktop.description().map(|value| value.to_lowercase()));
            Some(App {
                desktop,
                id,
                folded: name.to_lowercase(),
                name,
                terms,
            })
        })
        .collect();
    apps.sort_by(|a, b| a.folded.cmp(&b.folded).then_with(|| a.id.cmp(&b.id)));
    apps
}

fn score(text: &str, query: &str) -> Option<usize> {
    if text == query {
        return Some(0);
    }
    if text.starts_with(query) {
        return Some(1);
    }
    if text
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(query))
    {
        return Some(2);
    }
    if text.contains(query) {
        return Some(3);
    }
    let mut remaining = query.chars();
    let mut next = remaining.next()?;
    for c in text.chars() {
        if c == next {
            match remaining.next() {
                Some(ch) => next = ch,
                None => return Some(5),
            }
        }
    }
    None
}

fn search(apps: &[App], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    if words.is_empty() {
        return (0..apps.len()).collect();
    }
    let mut matches: Vec<_> = apps
        .iter()
        .enumerate()
        .filter_map(|(index, app)| {
            let mut rank = 0;
            for word in &words {
                let name = score(&app.folded, word);
                let term = app
                    .terms
                    .iter()
                    .filter_map(|text| score(text, word))
                    .filter(|score| *score < 5)
                    .min()
                    .map(|score| score + 6);
                rank += name.or(term)?;
            }
            let phrase = score(&app.folded, &query).unwrap_or(7);
            Some((phrase, rank, index))
        })
        .collect();
    matches.sort_unstable();
    matches.into_iter().map(|(_, _, index)| index).collect()
}

struct Tile {
    child: gtk::FlowBoxChild,
    icon: gtk::Image,
    label: gtk::Label,
    index: Cell<Option<usize>>,
}

struct Grid {
    root: gtk::FlowBox,
    tiles: Vec<Tile>,
}

impl Grid {
    fn new(columns: u32, size: usize, icon_size: i32) -> Self {
        let root = gtk::FlowBox::builder()
            .valign(gtk::Align::Start)
            .halign(gtk::Align::Center)
            .homogeneous(true)
            .min_children_per_line(columns)
            .max_children_per_line(columns)
            .row_spacing(12)
            .column_spacing(12)
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(true)
            .build();
        root.add_css_class("launcher-grid");
        let mut tiles = Vec::with_capacity(size);
        for _ in 0..size {
            let icon = gtk::Image::builder().pixel_size(icon_size).build();
            let label = gtk::Label::builder()
                .max_width_chars(13)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            label.add_css_class("launcher-tile-label");
            let tile = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(8)
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .build();
            tile.append(&icon);
            tile.append(&label);
            let child = gtk::FlowBoxChild::builder()
                .child(&tile)
                .visible(false)
                .build();
            child.add_css_class("launcher-flow-child");
            root.insert(&child, -1);
            tiles.push(Tile {
                child,
                icon,
                label,
                index: Cell::new(None),
            });
        }
        Self { root, tiles }
    }
}

struct Launcher {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    apps: Vec<App>,
    icon_theme: gtk::IconTheme,
    scale: i32,
    icon_size: i32,
    grids: [Grid; 2],
    stack: gtk::Stack,
    dots: gtk::Box,
    previous: gtk::Button,
    next: gtk::Button,
    empty: gtk::Label,
    results: RefCell<Vec<usize>>,
    current_page: Cell<usize>,
    active_grid: Cell<usize>,
}

impl Launcher {
    fn launch(&self, index: usize) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let context = gtk::prelude::WidgetExt::display(&window).app_launch_context();
        match self.apps[index].desktop.launch(&[], Some(&context)) {
            Ok(()) => window.close(),
            Err(error) => eprintln!(
                "[launcher] Could not launch {}: {error}",
                self.apps[index].name
            ),
        }
    }

    fn fill(&self) {
        let grid = &self.grids[self.active_grid.get()];
        let results = self.results.borrow();
        let start = self.current_page.get() * grid.tiles.len();
        grid.root.unselect_all();
        for (slot, tile) in grid.tiles.iter().enumerate() {
            let index = results.get(start + slot).copied();
            if tile.index.replace(index) == index {
                continue;
            }
            if let Some(index) = index {
                let app = &self.apps[index];
                let icon = app
                    .desktop
                    .icon()
                    .unwrap_or_else(|| gio::ThemedIcon::new("application-x-executable").upcast());
                let paintable = self.icon_theme.lookup_by_gicon(
                    &icon,
                    self.icon_size,
                    self.scale,
                    tile.icon.direction(),
                    gtk::IconLookupFlags::empty(),
                );
                tile.icon.set_paintable(Some(&paintable));
                tile.label.set_text(&app.name);
                let tooltip = app.desktop.description();
                tile.child
                    .set_tooltip_text(tooltip.as_deref().or(Some(&app.name)));
            } else {
                tile.icon.clear();
                tile.label.set_text("");
                tile.child.set_tooltip_text(None);
            }
            tile.child.set_visible(index.is_some());
        }
        if let Some(first) = grid.tiles.first().filter(|tile| tile.index.get().is_some()) {
            grid.root.select_child(&first.child);
        }
        self.empty.set_visible(results.is_empty());
    }

    fn update_pages(self: &Rc<Self>) {
        let count = self
            .results
            .borrow()
            .len()
            .div_ceil(self.grids[0].tiles.len());
        self.previous.set_visible(count > 1);
        self.next.set_visible(count > 1);
        self.previous.set_sensitive(self.current_page.get() > 0);
        self.next.set_sensitive(self.current_page.get() + 1 < count);
        self.dots.set_visible(count > 1);
        let mut children = self.dots.first_child();
        for page in 0..count {
            let button = match children.and_downcast::<gtk::Button>() {
                Some(button) => {
                    children = button.next_sibling();
                    button
                }
                None => {
                    children = None;
                    let button = gtk::Button::new();
                    button.add_css_class("launcher-dot");
                    button.set_tooltip_text(Some(&format!("Page {}", page + 1)));
                    let weak = Rc::downgrade(self);
                    button.connect_clicked(move |_| {
                        if let Some(launcher) = weak.upgrade() {
                            launcher.go(page);
                        }
                    });
                    self.dots.append(&button);
                    button
                }
            };
            if page == self.current_page.get() {
                button.add_css_class("active");
            } else {
                button.remove_css_class("active");
            }
        }
        while let Some(child) = children {
            children = child.next_sibling();
            self.dots.remove(&child);
        }
    }

    fn go(self: &Rc<Self>, page: usize) -> bool {
        let current = self.current_page.get();
        if page == current
            || page
                >= self
                    .results
                    .borrow()
                    .len()
                    .div_ceil(self.grids[0].tiles.len())
        {
            return false;
        }
        let active = 1 - self.active_grid.get();
        self.active_grid.set(active);
        self.current_page.set(page);
        self.fill();
        let direction = if page > current {
            gtk::StackTransitionType::SlideLeft
        } else {
            gtk::StackTransitionType::SlideRight
        };
        self.stack
            .set_visible_child_full(if active == 0 { "first" } else { "second" }, direction);
        self.update_pages();
        true
    }

    fn step(self: &Rc<Self>, forward: bool) -> bool {
        if forward {
            self.go(self.current_page.get() + 1)
        } else {
            self.current_page
                .get()
                .checked_sub(1)
                .is_some_and(|page| self.go(page))
        }
    }

    fn filter(self: &Rc<Self>, text: &str) {
        self.results.replace(search(&self.apps, text));
        self.current_page.set(0);
        self.fill();
        self.update_pages();
    }

    fn bind_grid(self: &Rc<Self>, grid_index: usize) {
        let grid = &self.grids[grid_index];
        let weak = Rc::downgrade(self);
        grid.root.connect_child_activated(move |_, child| {
            let Some(launcher) = weak.upgrade() else {
                return;
            };
            if let Some(index) = launcher.grids[grid_index]
                .tiles
                .get(child.index() as usize)
                .and_then(|tile| tile.index.get())
            {
                launcher.launch(index);
            }
        });
        for (slot, tile) in grid.tiles.iter().enumerate() {
            let drag = gtk::DragSource::builder()
                .actions(gdk::DragAction::COPY)
                .build();
            let weak = Rc::downgrade(self);
            drag.connect_prepare(move |_, _, _| {
                let launcher = weak.upgrade()?;
                let index = launcher.grids[grid_index].tiles[slot].index.get()?;
                Some(gdk::ContentProvider::for_value(
                    &launcher.apps[index].id.as_str().to_value(),
                ))
            });
            let icon = tile.icon.downgrade();
            drag.connect_drag_begin(move |drag, _| {
                if let Some(icon) = icon.upgrade() {
                    drag.set_icon(Some(&gtk::WidgetPaintable::new(Some(&icon))), 27, 27);
                }
            });
            drag.connect_drag_end(|drag, _, _| drag.set_icon(gdk::Paintable::NONE, 0, 0));
            tile.child.add_controller(drag);
        }
    }
}

fn arrow(icon: &str, tooltip: &str) -> gtk::Button {
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(22);
    let button = gtk::Button::builder()
        .child(&image)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("launcher-nav-btn");
    button
}

fn build(app: &gtk::Application, monitor: &gdk::Monitor, query: &str, overview: bool) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Applications")
        .build();
    window.add_css_class("launcher-window");
    window.init_layer_shell();
    window.set_namespace(Some("activities-launcher"));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }

    let geometry = monitor.geometry();
    let columns = ((geometry.width() - 100) / 108).clamp(3, 8) as u32;
    let rows = ((geometry.height() - 230) / 120).clamp(1, 3) as usize;
    let icon_size = if geometry.width() < 1000 { 64 } else { 81 };
    let grids = std::array::from_fn(|_| Grid::new(columns, columns as usize * rows, icon_size));
    let stack = gtk::Stack::builder()
        .transition_duration(220)
        .hexpand(true)
        .vexpand(true)
        .build();
    stack.add_named(&grids[0].root, Some("first"));
    stack.add_named(&grids[1].root, Some("second"));
    let previous = arrow("go-previous-symbolic", "Previous Page");
    let next = arrow("go-next-symbolic", "Next Page");
    let dots = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();
    let empty = gtk::Label::builder()
        .label("No Applications Found")
        .visible(false)
        .build();
    empty.add_css_class("launcher-empty");
    let launcher = Rc::new(Launcher {
        window: window.downgrade(),
        apps: installed_apps(),
        icon_theme: gtk::IconTheme::for_display(&monitor.display()),
        scale: monitor.scale_factor(),
        icon_size,
        grids,
        stack,
        dots,
        next,
        previous,
        empty,
        results: RefCell::default(),
        current_page: Cell::default(),
        active_grid: Cell::default(),
    });
    launcher.bind_grid(0);
    launcher.bind_grid(1);

    let search = gtk::Entry::builder()
        .placeholder_text("Type to search")
        .primary_icon_name("system-search-symbolic")
        .text(query)
        .halign(gtk::Align::Center)
        .width_request(400.min(geometry.width() - 48))
        .build();
    search.add_css_class("launcher-search");
    let weak = Rc::downgrade(&launcher);
    search.connect_changed(move |entry| {
        if let Some(launcher) = weak.upgrade() {
            launcher.filter(&entry.text());
        }
    });
    let weak = Rc::downgrade(&launcher);
    search.connect_activate(move |_| {
        if let Some(launcher) = weak.upgrade() {
            if let Some(&index) = launcher.results.borrow().first() {
                launcher.launch(index);
            }
        }
    });
    let search_keys = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(&launcher);
    search_keys.connect_key_pressed(move |_, key, _, _| {
        if matches!(key, gdk::Key::Down | gdk::Key::KP_Down) {
            if let Some(launcher) = weak.upgrade() {
                let grid = &launcher.grids[launcher.active_grid.get()];
                if let Some(first) = grid.tiles.first().filter(|tile| tile.index.get().is_some()) {
                    grid.root.select_child(&first.child);
                    first.child.grab_focus();
                    return glib::Propagation::Stop;
                }
            }
        }
        glib::Propagation::Proceed
    });
    search.add_controller(search_keys);

    for (button, forward) in [(&launcher.previous, false), (&launcher.next, true)] {
        let weak = Rc::downgrade(&launcher);
        button.connect_clicked(move |_| {
            if let Some(launcher) = weak.upgrade() {
                launcher.step(forward);
            }
        });
    }
    let grid_row = gtk::Box::builder()
        .spacing(12)
        .vexpand(true)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    grid_row.append(&launcher.previous);
    grid_row.append(&launcher.stack);
    grid_row.append(&launcher.next);
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let last_scroll = Cell::new(0);
    let weak = Rc::downgrade(&launcher);
    scroll.connect_scroll(move |_, dx, dy| {
        let now = glib::monotonic_time();
        let delta = if dx.abs() > dy.abs() { dx } else { dy };
        if delta.abs() > 0.2
            && now - last_scroll.get() >= 220_000
            && weak
                .upgrade()
                .is_some_and(|launcher| launcher.step(delta > 0.0))
        {
            last_scroll.set(now);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    grid_row.add_controller(scroll);

    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Fill)
        .margin_top(42)
        .margin_bottom(24)
        .margin_start(18)
        .margin_end(18)
        .width_request(1020.min(geometry.width() - 36))
        .build();
    let dock = dock::widget(
        &window,
        if overview {
            ApplicationsButton::Hide
        } else {
            ApplicationsButton::Hidden
        },
    );
    dock.set_halign(gtk::Align::Center);
    container.append(&search);
    container.append(&launcher.empty);
    container.append(&grid_row);
    container.append(&launcher.dots);
    container.append(&dock);
    window.set_child(Some(&container));
    launcher.filter(query);

    let click = gtk::GestureClick::new();
    let weak = window.downgrade();
    let weak_container = container.downgrade();
    click.connect_released(move |_, _, x, y| {
        if let Some(window) = weak.upgrade() {
            let picked = window.pick(x, y, gtk::PickFlags::DEFAULT);
            if picked.as_ref() == Some(window.upcast_ref())
                || picked == weak_container.upgrade().map(|box_| box_.upcast())
            {
                window.close();
            }
        }
    });
    window.add_controller(click);

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, _| {
        match key {
            gdk::Key::Escape => {
                if let Some(window) = launcher.window.upgrade() {
                    window.close();
                }
            }
            gdk::Key::Page_Down | gdk::Key::KP_Page_Down => {
                launcher.step(true);
            }
            gdk::Key::Page_Up | gdk::Key::KP_Page_Up => {
                launcher.step(false);
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
    glib::spawn_future_local(close_panel_popups());
    window.present();
    search.grab_focus();
    search.set_position(-1);
}

async fn close_panel_popups() {
    if let Ok(bus) = gio::bus_get_future(gio::BusType::Session).await {
        let panel_id = crate::bar::APP_ID;
        let path = format!("/{}", panel_id.replace('.', "/"));
        let parameters = (
            "close-popups",
            Vec::<glib::Variant>::new(),
            HashMap::<String, glib::Variant>::new(),
        );
        let _ = bus
            .call_future(
                Some(panel_id),
                &path,
                "org.gtk.Actions",
                "Activate",
                Some(&parameters.to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1000,
            )
            .await;
    }
    let _ = crate::quick_settings::command(&["palobar-ctl", "close-popup"]).await;
}

pub fn launch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    gio::Subprocess::newv(
        &[exe.as_os_str(), OsStr::new("toggle-launcher")],
        gio::SubprocessFlags::NONE,
    )
    .map(drop)
    .map_err(|error| error.to_string())
}

pub fn run(args: &[OsString]) -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_startup(|_| {
        if let Some(display) = gdk::Display::default() {
            dock::install_style(&display);
            let css = gtk::CssProvider::new();
            css.load_from_data(include_str!("launcher.css"));
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
    app.connect_command_line(|app, command| {
        let args = command.arguments();
        let mode = args
            .get(1)
            .and_then(|arg| arg.to_str())
            .unwrap_or("toggle-launcher");
        if let Some(window) = app.windows().first() {
            if mode != "open-launcher" {
                window.close();
            }
            return glib::ExitCode::SUCCESS;
        }
        if mode == "close-launcher" {
            return glib::ExitCode::SUCCESS;
        }
        let Some(display) = gdk::Display::default() else {
            return glib::ExitCode::FAILURE;
        };
        let monitors = display.monitors();
        let index = args
            .get(2)
            .and_then(|arg| arg.to_str())
            .and_then(|arg| arg.parse().ok())
            .unwrap_or(0);
        let query = args
            .get(3)
            .and_then(|arg| arg.to_str())
            .and_then(|arg| arg.strip_prefix("query="))
            .unwrap_or_default();
        if let Some(monitor) = monitors
            .item(index)
            .or_else(|| monitors.item(0))
            .and_downcast::<gdk::Monitor>()
        {
            build(app, &monitor, query, mode == "open-launcher");
            glib::ExitCode::SUCCESS
        } else {
            glib::ExitCode::FAILURE
        }
    });
    let argv: Vec<_> = std::iter::once("palobar-rust".into())
        .chain(args.iter().map(|arg| arg.to_string_lossy()))
        .collect();
    app.run_with_args(&argv)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn ranks_exact_and_prefix_matches_ahead_of_fuzzy_matches() {
        assert_eq!(score("terminal", "terminal"), Some(0));
        assert_eq!(score("terminal emulator", "term"), Some(1));
        assert_eq!(score("gnome terminal", "term"), Some(2));
        assert_eq!(score("xterminal", "term"), Some(3));
        assert_eq!(score("terminal", "tml"), Some(5));
        assert_eq!(score("terminal", "xyz"), None);
    }
}
