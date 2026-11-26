use super::{choose_image, desktop_services, label};
use crate::background::{mode_index, Background, MODES};
use crate::config::{read_config, update_config};
use crate::lockscreen;
use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib, pango};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const DELAYS: [(&str, Option<u64>); 7] = [
    ("Disabled", None),
    ("Immediately", Some(0)),
    ("30 seconds", Some(30)),
    ("1 minute", Some(60)),
    ("2 minutes", Some(120)),
    ("5 minutes", Some(300)),
    ("10 minutes", Some(600)),
];

struct Page {
    file: gio::File,
    status: gtk::Label,
    path: gtk::Label,
    preview: gtk::DrawingArea,
    background: Rc<RefCell<Option<Background>>>,
    preview_css: gtk::CssProvider,
    images: RefCell<Option<(String, String)>>,
    color: gtk::ColorButton,
    mode: gtk::DropDown,
    saved_mode: Cell<u32>,
    delay: gtk::DropDown,
    delay_hint: gtk::Label,
    saved_delay: Cell<u32>,
}

impl Page {
    fn report(&self, message: Option<&str>) {
        self.status.set_text(message.unwrap_or_default());
        self.status.set_visible(message.is_some());
    }

    fn refresh(&self) {
        let config = match read_config(&self.file) {
            Ok((config, _)) => config,
            Err(error) => {
                self.report(Some(&format!(
                    "Could not read lock screen settings. {error}"
                )));
                return;
            }
        };
        let lock = &config["lockscreen"];
        let image = lock["image"].as_str().unwrap_or_default();
        let wallpaper = config["wallpaper"]["path"].as_str().unwrap_or_default();
        self.path.set_text(if image.is_empty() {
            "Defaults to wallpaper"
        } else {
            image
        });
        let mut images = self.images.borrow_mut();
        if !matches!(images.as_ref(), Some((old, fallback)) if old == image && fallback == wallpaper)
        {
            let background = [image, wallpaper]
                .into_iter()
                .filter(|path| !path.is_empty())
                .find_map(|path| {
                    let image =
                        gdk_pixbuf::Pixbuf::from_file_at_scale(path, 440, 248, true).ok()?;
                    Background::new(&image)
                });
            self.background.replace(background);
            *images = Some((image.to_owned(), wallpaper.to_owned()));
        }
        let color = lock["color"]
            .as_str()
            .and_then(|color| gdk::RGBA::parse(color).ok())
            .unwrap_or_else(|| {
                gdk::RGBA::parse(crate::lockscreen::FALLBACK_COLOR).expect("valid fallback color")
            });
        self.preview_css.load_from_data(&format!(
            ".prefs-wp-preview {{ background-color: {color}; }}"
        ));
        self.color.set_rgba(&color);
        let mode = mode_index(lock["mode"].as_str());
        self.saved_mode.set(mode);
        self.mode.set_selected(mode);
        self.preview.queue_draw();
        let enabled = config["power"]["screenBlankEnabled"]
            .as_bool()
            .unwrap_or(true);
        self.delay.set_sensitive(enabled);
        self.delay_hint.set_text(if enabled {
            "Wait after the screen turns off"
        } else {
            "Enable Automatic Screen Blank in Power"
        });
        let seconds = lock["autoLockDelaySeconds"].as_f64();
        let selected = DELAYS
            .iter()
            .position(|(_, delay)| delay.map(|delay| delay as f64) == seconds)
            .unwrap_or_default();
        self.saved_delay.set(selected as u32);
        self.delay.set_selected(selected as u32);
    }

    fn save(&self, key: &str, value: Value) {
        let result = (|| {
            if let Some(parent) = self
                .file
                .path()
                .and_then(|path| path.parent().map(ToOwned::to_owned))
            {
                std::fs::create_dir_all(parent)?;
            }
            update_config(&self.file, |config| {
                let lock = &mut config["lockscreen"];
                if lock.is_null() {
                    *lock = json!({});
                }
                let lock = lock
                    .as_object_mut()
                    .ok_or("Lock screen configuration must be a JSON object")?;
                lock.insert(key.to_owned(), value.clone());
                Ok(true)
            })
        })();
        self.report(None);
        self.refresh();
        if let Err(error) = result {
            self.report(Some(&format!(
                "Could not save lock screen settings. {error}"
            )));
        }
    }
}

fn row(group: &gtk::Box, last: bool) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("prefs-row");
    if last {
        row.add_css_class("last");
    }
    group.append(&row);
    row
}

fn append_info(row: &gtk::Box, title: &str, hint: &str) -> gtk::Label {
    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.append(&label(title, "prefs-row-title"));
    let hint = label(hint, "prefs-muted");
    info.append(&hint);
    row.append(&info);
    hint
}

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("prefs-page-body");
    page.append(&label("Authentication", "prefs-group-title"));
    page.append(&desktop_services::authentication(&page));
    page.append(&label("Lock Screen", "prefs-group-title"));
    let availability = label("", "prefs-muted");
    page.append(&availability);
    let status = label("", "error");
    status.set_visible(false);
    page.append(&status);
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    page.append(&group);

    let preview = row(&group, false);
    preview.set_halign(gtk::Align::Center);
    let frame = gtk::Overlay::new();
    frame.add_css_class("prefs-wp-preview");
    frame.set_overflow(gtk::Overflow::Hidden);
    let drawing = gtk::DrawingArea::builder()
        .content_width(220)
        .content_height(124)
        .build();
    frame.set_child(Some(&drawing));
    preview.append(&frame);
    let preview_css = gtk::CssProvider::new();
    frame
        .style_context()
        .add_provider(&preview_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    let image = row(&group, false);
    let path = append_info(&image, "Lock Screen Image", "Defaults to wallpaper");
    path.set_wrap(false);
    path.set_ellipsize(pango::EllipsizeMode::Middle);
    path.set_max_width_chars(32);
    let choose = gtk::Button::with_mnemonic("_Choose…");
    let reset = gtk::Button::with_mnemonic("_Reset");
    for button in [&choose, &reset] {
        button.add_css_class("prefs-btn");
        image.append(button);
    }

    let color_row = row(&group, false);
    let color_label = label("Fallback Color", "prefs-row-title");
    color_label.set_hexpand(true);
    color_row.append(&color_label);
    let color = gtk::ColorButton::new();
    color.set_title("Fallback Color");
    color.set_use_alpha(false);
    color.update_property(&[gtk::accessible::Property::Label("Fallback Color")]);
    color_row.append(&color);

    let mode_row = row(&group, false);
    let mode_label = label("_Display Mode", "prefs-row-title");
    mode_label.set_use_underline(true);
    mode_label.set_hexpand(true);
    mode_row.append(&mode_label);
    let mode = gtk::DropDown::from_strings(&MODES);
    mode_label.set_mnemonic_widget(Some(&mode));
    mode.update_property(&[gtk::accessible::Property::Label("Display Mode")]);
    mode_row.append(&mode);

    let auto_lock = row(&group, false);
    let delay_hint = append_info(&auto_lock, "Automatic Lock", "");
    let names = DELAYS.map(|(name, _)| name);
    let delay = gtk::DropDown::from_strings(&names);
    delay.update_property(&[gtk::accessible::Property::Label("Automatic Lock")]);
    auto_lock.append(&delay);

    let test = row(&group, true);
    append_info(
        &test,
        "Test Lock Screen",
        "Locks this session using the saved settings",
    );
    let lock_now = gtk::Button::with_mnemonic("_Lock Now");
    lock_now.add_css_class("prefs-btn");
    test.append(&lock_now);

    let background = Rc::new(RefCell::new(None::<Background>));
    drawing.set_draw_func({
        let mode = mode.downgrade();
        let background = background.clone();
        move |_, context, width, height| {
            if let (Some(mode), Some(background)) = (mode.upgrade(), background.borrow().as_ref()) {
                let mode = MODES
                    .get(mode.selected() as usize)
                    .copied()
                    .unwrap_or(MODES[0]);
                background.draw(context, width, height, mode, 0.25);
            }
        }
    });
    let state = Rc::new(Page {
        file: gio::File::for_path(glib::user_config_dir().join("palobar/config.json")),
        status,
        path,
        preview: drawing,
        background,
        preview_css,
        images: RefCell::default(),
        color,
        mode,
        saved_mode: Cell::default(),
        delay,
        delay_hint,
        saved_delay: Cell::default(),
    });
    choose.connect_clicked({
        let window = window.downgrade();
        let state = Rc::downgrade(&state);
        move |_| {
            if let Some(window) = window.upgrade() {
                let state = state.clone();
                choose_image(&window, "Choose Lock Screen Image", move |result| {
                    if let Some(state) = state.upgrade() {
                        match result {
                            Ok(path) => state.save("image", json!(path)),
                            Err(error) => state.report(Some(error)),
                        }
                    }
                });
            }
        }
    });
    reset.connect_clicked({
        let state = Rc::downgrade(&state);
        move |_| {
            if let Some(state) = state.upgrade() {
                state.save("image", json!(""));
            }
        }
    });
    state.color.connect_color_set({
        let state = Rc::downgrade(&state);
        move |button| {
            if let Some(state) = state.upgrade() {
                let color = button.rgba();
                let [red, green, blue] = [color.red(), color.green(), color.blue()]
                    .map(|value| (value * 255.0).round() as u8);
                state.save("color", json!(format!("#{red:02x}{green:02x}{blue:02x}")));
            }
        }
    });
    state.mode.connect_selected_notify({
        let state = Rc::downgrade(&state);
        move |mode| {
            if let Some(state) = state.upgrade() {
                if mode.selected() != state.saved_mode.get() {
                    if let Some(mode) = MODES.get(mode.selected() as usize) {
                        state.save("mode", json!(mode));
                    }
                }
            }
        }
    });
    state.delay.connect_selected_notify({
        let state = Rc::downgrade(&state);
        move |delay| {
            if let Some(state) = state.upgrade() {
                if delay.selected() != state.saved_delay.get() {
                    if let Some((_, seconds)) = DELAYS.get(delay.selected() as usize) {
                        state.save("autoLockDelaySeconds", json!(seconds));
                    }
                }
            }
        }
    });
    lock_now.connect_clicked({
        let state = Rc::downgrade(&state);
        move |button| {
            let Some(page) = state.upgrade() else { return };
            let process = match lockscreen::start_lock(gio::SubprocessFlags::STDERR_PIPE) {
                Ok(process) => process,
                Err(error) => {
                    page.report(Some(&format!("Could not lock the screen. {error}")));
                    return;
                }
            };
            button.set_sensitive(false);
            let button = button.downgrade();
            let state = state.clone();
            glib::spawn_future_local(async move {
                let result = process.communicate_utf8_future(None).await;
                if let Some(button) = button.upgrade() {
                    button.set_sensitive(true);
                }
                if let Some(page) = state.upgrade() {
                    let error = match result {
                        Ok(_) if process.is_successful() => return,
                        Ok((_, stderr)) => stderr.map_or_else(
                            || "The lock screen exited unsuccessfully.".into(),
                            |text| text.trim().to_owned(),
                        ),
                        Err(error) => error.to_string(),
                    };
                    page.report(Some(&format!("Could not lock the screen. {error}")));
                }
            });
        }
    });
    page.connect_map(move |_| {
        state.refresh();
        let support = lockscreen::check_support();
        group.set_sensitive(support.is_ok());
        availability.set_visible(support.is_err());
        if let Err(error) = support {
            availability.set_text(&error);
        }
    });
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}
