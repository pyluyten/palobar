#[path = "calendar/media.rs"]
mod media;
#[path = "calendar/month.rs"]
mod month;
#[path = "calendar/notifications.rs"]
mod notifications;

use chrono::Local;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::time::Duration;

pub const ACTION: &str = "toggle-calendar";
const CLASS: &str = "date-menu-window";

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .max_width_chars(32)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    label.add_css_class(class);
    label
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    button.add_css_class("date-button");
    button
}

fn scroll(child: &gtk::Box, height: i32) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(height)
        .propagate_natural_height(true)
        .child(child)
        .build()
}

fn clear(box_: &gtk::Box) {
    while let Some(child) = box_.first_child() {
        box_.remove(&child);
    }
}

pub fn install_style(display: &gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("calendar.css"));
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn is_open(app: &gtk::Application) -> bool {
    app.windows()
        .iter()
        .any(|window| window.has_css_class(CLASS))
}

pub fn close(app: &gtk::Application) -> bool {
    crate::panel_popup::close(app, CLASS)
}

pub fn toggle(app: &gtk::Application, monitor: &gdk::Monitor, indicator: Option<&gtk::Button>) {
    if close(app) {
        return;
    }
    crate::quick_settings::close(app);
    crate::clipboard::close(app);
    crate::workspace_dialog::close(app);
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(18)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Start)
        .margin_top(6)
        .build();
    card.add_css_class("date-menu-card");
    let window = crate::panel_popup::show_window(
        app,
        monitor,
        "Calendar and Notifications",
        "calendar-popup",
        CLASS,
        &card,
        indicator,
    );
    let feed = gtk::Box::new(gtk::Orientation::Vertical, 14);
    feed.add_css_class("date-menu-feed");
    card.append(&feed);
    let media = media::section(&feed);
    feed.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let notifications = notifications::section(&feed, &window);
    card.append(&gtk::Separator::new(gtk::Orientation::Vertical));

    let calendar = gtk::Box::new(gtk::Orientation::Vertical, 12);
    calendar.add_css_class("date-menu-calendar");
    let header = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let weekday = label("", "date-menu-weekday");
    let date = label("", "date-menu-date");
    header.append(&weekday);
    header.append(&date);
    calendar.append(&header);
    calendar.append(&month::widget());
    card.append(&calendar);
    let date_update = glib::spawn_future_local(async move {
        loop {
            let now = Local::now();
            weekday.set_text(&now.format("%A").to_string());
            date.set_text(&now.format("%B %-d, %Y").to_string());
            glib::timeout_future(Duration::from_secs(30)).await;
        }
    });
    window.connect_unrealize(move |_| {
        media.abort();
        notifications.abort();
        date_update.abort();
    });
    window.present();
}

pub fn toggle_first_monitor(app: &gtk::Application) {
    if let Some(monitor) = gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gdk::Monitor>()
    {
        toggle(app, &monitor, None);
    }
}

pub fn register_panel_action(app: &gtk::Application) {
    let weak = app.downgrade();
    let action = gio::SimpleAction::new(ACTION, None);
    action.connect_activate(move |_, _| {
        if let Some(app) = weak.upgrade() {
            toggle_first_monitor(&app);
        }
    });
    app.add_action(&action);
}
