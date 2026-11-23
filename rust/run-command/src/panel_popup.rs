use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn show_window(
    app: &gtk::Application,
    monitor: &gdk::Monitor,
    title: &str,
    namespace: &str,
    class: &str,
    card: &gtk::Box,
    indicator: Option<&gtk::Button>,
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(title)
        .build();
    window.add_css_class(class);
    window.init_layer_shell();
    window.set_namespace(Some(namespace));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
        window.set_anchor(edge, true);
    }

    let area = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    card.set_hexpand(true);
    area.append(card);
    window.set_child(Some(&area));
    let click = gtk::GestureClick::builder()
        .button(gdk::BUTTON_PRIMARY)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let weak_window = window.downgrade();
    click.connect_pressed(move |gesture, _, x, y| {
        let Some(area) = gesture.widget() else { return };
        if area.pick(x, y, gtk::PickFlags::DEFAULT).as_ref() == Some(&area) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(window) = weak_window.upgrade() {
                window.close();
            }
        }
    });
    area.add_controller(click);
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
    if let Some(indicator) = indicator {
        indicator.add_css_class("open");
        let indicator = indicator.downgrade();
        window.connect_unrealize(move |_| {
            if let Some(indicator) = indicator.upgrade() {
                indicator.remove_css_class("open");
            }
        });
    }
    window
}

pub fn close(app: &gtk::Application, class: &str) -> bool {
    if let Some(window) = app
        .windows()
        .into_iter()
        .find(|window| window.has_css_class(class))
    {
        window.close();
        true
    } else {
        false
    }
}
