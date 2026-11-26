use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::collections::HashMap;
use std::rc::Rc;

const CONTROL_PATH: &str = "/io/Astal/Palobar/Control";
const CONTROL_INTERFACE: &str = "io.Astal.Palobar.Control";

pub(crate) fn register_state(
    app: &gtk::Application,
    notifications: Option<&Rc<crate::notifications::Service>>,
) {
    let Some(connection) = app.dbus_connection() else {
        return;
    };
    let result = gio::DBusNodeInfo::for_xml(
        r#"<node><interface name="io.Astal.Palobar.Control">
            <method name="GetState"><arg type="s" direction="out"/><arg type="u" direction="out"/></method>
            <method name="GetNotificationState"><arg type="b" direction="out"/><arg type="b" direction="out"/></method>
            <method name="SetNotificationsEnabled"><arg type="b" direction="in"/><arg type="b" direction="out"/></method>
        </interface></node>"#,
    ).and_then(|node| {
        let interface = node.lookup_interface(CONTROL_INTERFACE).expect("control interface");
        let app = app.downgrade();
        let notifications = notifications.map(Rc::downgrade);
        connection.register_object(CONTROL_PATH, &interface)
            .method_call(move |_, _, _, _, method, parameters, call| {
                let active = app.upgrade().and_then(|app| {
                    app.windows().into_iter().filter(|window| window.is_visible()).find_map(|window| {
                        [
                            ("workspace-dialog-window", "workspaces"),
                            ("quick-settings-window", "quick-settings"),
                            ("date-menu-window", "calendar"),
                            ("clipboard-window", "clipboard"),
                        ].into_iter().find_map(|(class, name)| window.has_css_class(class).then_some(name))
                    })
                });
                let unread = notifications.as_ref().and_then(|service| service.upgrade())
                    .map_or(0, |service| service.unread_count());
                match method {
                    "GetState" => {
                        call.return_value(Some(&(active.unwrap_or(""), unread).to_variant()));
                    }
                    "GetNotificationState" => {
                        let service = notifications
                            .as_ref()
                            .and_then(|service| service.upgrade());
                        let state = service
                            .as_ref()
                            .map(|service| (true, service.is_enabled()))
                            .unwrap_or((false, false));
                        call.return_value(Some(&state.to_variant()));
                    }
                    "SetNotificationsEnabled" => {
                        let Some(enabled) = parameters.get::<(bool,)>().map(|(enabled,)| enabled)
                        else {
                            call.return_dbus_error(
                                "io.Astal.Palobar.Control.InvalidArguments",
                                "Expected one boolean argument",
                            );
                            return;
                        };
                        let result = notifications
                            .as_ref()
                            .and_then(|service| service.upgrade())
                            .ok_or_else(|| "Palobar notification service is unavailable".to_owned())
                            .and_then(|service| service.set_enabled(enabled));
                        match result {
                            Ok(()) => call.return_value(Some(&(enabled,).to_variant())),
                            Err(error) => call.return_dbus_error(
                                "io.Astal.Palobar.Control.NotificationError",
                                &error,
                            ),
                        }
                    }
                    _ => call.return_dbus_error(
                        "io.Astal.Palobar.Control.UnknownMethod",
                        "Unknown control method",
                    ),
                }
            }).build()
    });
    match result {
        Ok(id) => {
            let registration = std::cell::Cell::new(Some(id));
            app.connect_shutdown(move |_| {
                if let Some(id) = registration.take() {
                    let _ = connection.unregister_object(id);
                }
            });
        }
        Err(error) => eprintln!("[palobar] Could not register the control interface: {error}"),
    }
}

pub(crate) fn control_action(action: &str) -> glib::ExitCode {
    let parameters = (
        action,
        Vec::<glib::Variant>::new(),
        HashMap::<String, glib::Variant>::new(),
    );
    let path = format!("/{}", crate::bar::APP_ID.replace('.', "/"));
    control_call(
        &path,
        "org.gtk.Actions",
        "Activate",
        Some(&parameters.to_variant()),
    )
    .map_or_else(report_control_failure, |_| glib::ExitCode::SUCCESS)
}

pub(crate) fn print_state(command: &str) -> glib::ExitCode {
    match control_call(CONTROL_PATH, CONTROL_INTERFACE, "GetState", None) {
        Ok(reply) => {
            let Some((active, unread)) = reply.get::<(String, u32)>() else {
                return glib::ExitCode::FAILURE;
            };
            let active = (!active.is_empty()).then_some(active);
            match command {
                "get-active-popup" => println!("{}", active.as_deref().unwrap_or("none")),
                "get-unread-count" => println!("{unread}"),
                _ => println!(
                    "{}",
                    serde_json::json!({"active_popup": active, "unread_count": unread})
                ),
            }
            glib::ExitCode::SUCCESS
        }
        Err(error) => report_control_failure(error),
    }
}

fn control_call(
    path: &str,
    interface: &str,
    method: &str,
    parameters: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    connection.call_sync(
        Some(crate::bar::APP_ID),
        path,
        interface,
        method,
        parameters,
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        1000,
        gio::Cancellable::NONE,
    )
}

fn report_control_failure(error: glib::Error) -> glib::ExitCode {
    eprintln!("[palobar] Could not contact the Rust panel: {error}");
    glib::ExitCode::FAILURE
}

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
