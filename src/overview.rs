use crate::{bar, calendar, clipboard, dock, launcher, quick_settings};
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CAPTURE_CLASS: &str = "overview-capture-window";

fn typing_key(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<char> {
    use gdk::ModifierType as Mod;
    if modifiers.intersects(
        Mod::CONTROL_MASK | Mod::ALT_MASK | Mod::SUPER_MASK | Mod::META_MASK | Mod::HYPER_MASK,
    ) {
        return None;
    }
    key.to_unicode().filter(|key| key.is_alphabetic())
}

fn track_input_region(window: &gtk::ApplicationWindow, widgets: [&gtk::Widget; 2]) {
    let widgets = widgets.map(|widget| widget.downgrade());
    window.connect_realize(move |window| {
        let (Some(surface), Some(clock)) = (window.surface(), window.frame_clock()) else {
            return;
        };
        surface.set_input_region(&cairo::Region::create());
        let window_weak = window.downgrade();
        let widgets = widgets.clone();
        let previous = RefCell::new(Vec::new());
        let handler = clock.connect_local("layout", true, move |_| {
            if let Some(window) = window_weak.upgrade() {
                let bounds: Vec<_> = widgets
                    .iter()
                    .filter_map(glib::WeakRef::upgrade)
                    .filter(|widget| widget.is_mapped())
                    .filter_map(|widget| widget.compute_bounds(&window))
                    .filter(|rect| rect.width() > 0.0 && rect.height() > 0.0)
                    .map(|rect| {
                        let x = rect.x().floor();
                        let y = rect.y().floor();
                        cairo::RectangleInt::new(
                            x as i32,
                            y as i32,
                            ((rect.x() + rect.width()).ceil() - x) as i32,
                            ((rect.y() + rect.height()).ceil() - y) as i32,
                        )
                    })
                    .collect();
                if *previous.borrow() != bounds {
                    surface.set_input_region(&cairo::Region::create_rectangles(&bounds));
                    previous.replace(bounds);
                }
            }
            None
        });
        let handler = RefCell::new(Some(handler));
        window.connect_unrealize(move |_| {
            if let Some(handler) = handler.take() {
                clock.disconnect(handler);
            }
        });
    });
}

fn open_launcher(app: &gtk::Application, monitor: &gdk::Monitor, query: &str) {
    if launcher::window(app).is_none() {
        for window in app.windows() {
            if window.has_css_class(CAPTURE_CLASS) {
                window.close();
            }
        }
        launcher::build(app, monitor, query, true);
    }
}

fn capture(app: &gtk::Application, monitor: &gdk::Monitor, index: u32) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Overview")
        .build();
    window.add_css_class(CAPTURE_CLASS);
    window.init_layer_shell();
    window.set_namespace(Some(&format!("overview-capture-{index}")));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }

    let search = gtk::Entry::builder()
        .placeholder_text("Type to search")
        .primary_icon_name("system-search-symbolic")
        .halign(gtk::Align::Center)
        .width_request(400.min((monitor.geometry().width() - 32).max(1)))
        .build();
    search.add_css_class("launcher-search");

    let weak_app = app.downgrade();
    let target = monitor.clone();
    let dock = dock::widget(
        &window,
        dock::ApplicationsButton::OpenOverview(Rc::new(move || {
            if let Some(app) = weak_app.upgrade() {
                open_launcher(&app, &target, "");
            }
        })),
    );
    dock.set_halign(gtk::Align::Center);
    let contents = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .margin_top(42)
        .margin_bottom(24)
        .build();
    contents.append(&search);
    contents.append(&gtk::Box::builder().vexpand(true).build());
    contents.append(&dock);
    window.set_child(Some(&contents));
    track_input_region(&window, [search.upcast_ref(), dock.upcast_ref()]);

    let weak_app = app.downgrade();
    let target = monitor.clone();
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let action = match key {
            gdk::Key::Escape | gdk::Key::Return | gdk::Key::KP_Enter => "close-overview",
            gdk::Key::Left | gdk::Key::KP_Left => "focus-column-left",
            gdk::Key::Right | gdk::Key::KP_Right => "focus-column-right",
            gdk::Key::Up | gdk::Key::KP_Up => "focus-workspace-up",
            gdk::Key::Down | gdk::Key::KP_Down => "focus-workspace-down",
            _ => {
                if let (Some(ch), Some(app)) = (typing_key(key, modifiers), weak_app.upgrade()) {
                    open_launcher(&app, &target, ch.encode_utf8(&mut [0; 4]));
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
        };
        bar::send_niri_action(action, &[]);
        glib::Propagation::Stop
    });
    window.add_controller(keys);
    window.present();
    search.grab_focus();
    window
}

pub fn install(app: &gtk::Application, display: &gdk::Display, open: Arc<AtomicBool>) {
    let css = gtk::CssProvider::new();
    css.load_from_data("window.overview-capture-window { background: transparent; }");
    gtk::style_context_add_provider_for_display(
        display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let app = app.downgrade();
    let monitors = display.monitors();
    let mut captures: Vec<(gdk::Monitor, glib::WeakRef<gtk::ApplicationWindow>)> = Vec::new();
    let mut was_open = false;
    glib::timeout_add_local(Duration::from_millis(60), move || {
        let Some(app) = app.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let overview_open = open.load(Ordering::Relaxed);
        if was_open && !overview_open {
            launcher::close_overview_launcher(&app);
        }
        was_open = overview_open;
        let show = overview_open
            && launcher::window(&app).is_none()
            && !calendar::is_open(&app)
            && !clipboard::is_open(&app)
            && !quick_settings::is_open(&app);
        captures.retain(|(monitor, window)| {
            let Some(window) = window.upgrade() else {
                return false;
            };
            if !show || !monitor.is_valid() {
                window.close();
                return false;
            }
            true
        });
        if show {
            for index in 0..monitors.n_items() {
                if let Some(monitor) = monitors.item(index).and_downcast::<gdk::Monitor>() {
                    if !captures.iter().any(|(existing, _)| existing == &monitor) {
                        captures
                            .push((monitor.clone(), capture(&app, &monitor, index).downgrade()));
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::{gdk, typing_key};

    #[test]
    fn overview_search_accepts_letters_and_ignores_shortcuts() {
        assert_eq!(
            typing_key(gdk::Key::a, gdk::ModifierType::empty()),
            Some('a')
        );
        assert_eq!(
            typing_key(gdk::Key::A, gdk::ModifierType::SHIFT_MASK),
            Some('A')
        );
        assert_eq!(typing_key(gdk::Key::Left, gdk::ModifierType::empty()), None);
        assert_eq!(
            typing_key(gdk::Key::a, gdk::ModifierType::CONTROL_MASK),
            None
        );
        assert_eq!(typing_key(gdk::Key::a, gdk::ModifierType::ALT_MASK), None);
        assert_eq!(typing_key(gdk::Key::a, gdk::ModifierType::SUPER_MASK), None);
    }
}
