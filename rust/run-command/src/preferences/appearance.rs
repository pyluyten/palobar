use super::label;
use crate::config::{read_config, update_config};
use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib, pango};
use serde_json::{json, Value};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

const DEFAULT_COLOR: &str = "#131316";
const MODES: [&str; 5] = ["fill", "fit", "stretch", "center", "tile"];
const ACCENTS: [(&str, &str, &str); 9] = [
    ("blue", "Blue", "#3584e4"),
    ("teal", "Teal", "#2190a4"),
    ("green", "Green", "#3a944a"),
    ("yellow", "Yellow", "#c88800"),
    ("orange", "Orange", "#ed5b00"),
    ("red", "Red", "#e62d42"),
    ("pink", "Pink", "#d56199"),
    ("purple", "Purple", "#9141ac"),
    ("slate", "Slate", "#6f8396"),
];

fn section(page: &gtk::Box, title: &str) -> gtk::Box {
    page.append(&label(title, "prefs-group-title"));
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    page.append(&group);
    group
}

fn row(group: &gtk::Box, last: bool) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("prefs-row");
    if last {
        row.add_css_class("last");
    }
    group.append(&row);
    row
}

fn style_card(title: &str, dark: bool) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let preview = gtk::Overlay::new();
    preview.add_css_class("prefs-style-preview");
    preview.set_child(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let back = gtk::Box::builder()
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .build();
    back.add_css_class("prefs-mockup-back");
    preview.add_overlay(&back);
    let front = gtk::Box::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .build();
    front.add_css_class("prefs-mockup-front");
    if dark {
        front.add_css_class("dark");
    }
    preview.add_overlay(&front);
    content.append(&preview);
    content.append(&gtk::Label::new(Some(title)));
    let button = gtk::Button::builder().child(&content).build();
    button.add_css_class("prefs-style-card");
    button
}

fn show_error(status: &gtk::Label, error: Option<&str>) {
    status.set_text(error.unwrap_or_default());
    status.set_visible(error.is_some());
}

fn bind_choices(
    group: &gtk::Box,
    status: &gtk::Label,
    key: &'static str,
    choices: &[(&'static str, gtk::Button)],
) {
    let settings = gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
        .filter(|schema| schema.has_key(key))
        .map(|schema| gio::Settings::new_full(&schema, gio::SettingsBackend::NONE, None));
    let fallback = choices[0].0;
    let references: Vec<_> = choices
        .iter()
        .map(|(value, button)| (*value, button.downgrade()))
        .collect();
    let refresh = move |settings: Option<&gio::Settings>| {
        let value = settings.map(|settings| settings.string(key));
        let selected = value
            .as_deref()
            .filter(|value| references.iter().any(|(id, _)| id == value))
            .unwrap_or(fallback);
        let writable = settings.is_some_and(|settings| settings.is_writable(key));
        for (id, button) in &references {
            if let Some(button) = button.upgrade() {
                if *id == selected {
                    button.add_css_class("selected");
                } else {
                    button.remove_css_class("selected");
                }
                button.update_state(&[gtk::accessible::State::Selected(Some(*id == selected))]);
                button.set_sensitive(writable);
            }
        }
    };
    let Some(settings) = settings else {
        refresh(None);
        return;
    };
    let changed = settings.connect_changed(Some(key), {
        let refresh = refresh.clone();
        move |settings, _| refresh(Some(settings))
    });
    let writable = settings.connect_writable_changed(Some(key), {
        let refresh = refresh.clone();
        move |settings, _| refresh(Some(settings))
    });
    refresh(Some(&settings));
    for (value, button) in choices {
        let value = *value;
        let settings = settings.clone();
        let status = status.clone();
        button.connect_clicked(move |_| {
            let error = settings
                .set_string(key, value)
                .err()
                .map(|error| format!("Could not save appearance settings: {error}"));
            show_error(&status, error.as_deref());
        });
    }
    let handlers = Cell::new(Some((changed, writable)));
    group.connect_destroy(move |_| {
        if let Some((changed, writable)) = handlers.take() {
            settings.disconnect(changed);
            settings.disconnect(writable);
        }
    });
}

fn desktop_appearance(page: &gtk::Box, status: &gtk::Label) {
    let styles = section(page, "Style");
    let style_row = row(&styles, true);
    style_row.set_halign(gtk::Align::Center);
    let choices = [
        ("default", style_card("Default", false)),
        ("prefer-dark", style_card("Dark", true)),
    ];
    for (_, button) in &choices {
        style_row.append(button);
    }
    bind_choices(&styles, status, "color-scheme", &choices);

    let accents = section(page, "Accent Color");
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(ACCENTS.len() as u32)
        .halign(gtk::Align::Center)
        .column_spacing(4)
        .build();
    row(&accents, true).append(&flow);
    let choices: Vec<_> = ACCENTS
        .iter()
        .map(|&(id, name, hex)| {
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("prefs-accent-dot");
            let button = gtk::Button::builder()
                .tooltip_text(name)
                .child(&dot)
                .build();
            button.update_property(&[gtk::accessible::Property::Label(name)]);
            button.add_css_class("prefs-accent-btn");
            let provider = gtk::CssProvider::new();
            provider.load_from_data(&format!(
                ".prefs-accent-dot {{ background-color: {hex}; }} \
                 .prefs-accent-btn.selected {{ border-color: {hex}; }}"
            ));
            dot.style_context()
                .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            button
                .style_context()
                .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            flow.insert(&button, -1);
            (id, button)
        })
        .collect();
    bind_choices(&accents, status, "accent-color", &choices);
}

struct Wallpaper {
    config: PathBuf,
    status: gtk::Label,
    path_label: gtk::Label,
    picture: gtk::Picture,
    background: gtk::DrawingArea,
    color: Rc<Cell<gdk::RGBA>>,
}

impl Wallpaper {
    fn save(&self, key: &str, value: &str) -> bool {
        let result = (|| {
            if let Some(directory) = self.config.parent() {
                std::fs::create_dir_all(directory)?;
            }
            update_config(&gio::File::for_path(&self.config), |config| {
                let wallpaper = &mut config["wallpaper"];
                if wallpaper.is_null() {
                    *wallpaper = json!({});
                }
                let wallpaper = wallpaper
                    .as_object_mut()
                    .ok_or("Wallpaper configuration must be a JSON object")?;
                wallpaper.insert(key.to_owned(), value.into());
                Ok(true)
            })
        })();
        let error = result
            .err()
            .map(|error| format!("Could not save background settings: {error}"));
        show_error(&self.status, error.as_deref());
        error.is_none()
    }

    fn show_image(&self, path: &str) {
        self.path_label.set_text(if path.is_empty() {
            "None (solid color)"
        } else {
            path
        });
        let texture = if path.is_empty() {
            None
        } else {
            match gdk_pixbuf::Pixbuf::from_file_at_scale(path, 440, 248, true) {
                Ok(pixbuf) => Some(gdk::Texture::for_pixbuf(&pixbuf)),
                Err(error) => {
                    show_error(
                        &self.status,
                        Some(&format!("Could not preview wallpaper: {error}")),
                    );
                    None
                }
            }
        };
        self.picture.set_paintable(texture.as_ref());
    }
}

fn wallpaper(page: &gtk::Box, window: &gtk::ApplicationWindow, status: &gtk::Label) {
    let path = glib::user_config_dir().join("palobar/config.json");
    let config = match read_config(&gio::File::for_path(&path)) {
        Ok((config, _)) => config,
        Err(error) => {
            show_error(
                status,
                Some(&format!("Could not load background settings: {error}")),
            );
            Value::Null
        }
    };
    let config = &config["wallpaper"];
    let initial_color = config["color"]
        .as_str()
        .and_then(|value| gdk::RGBA::parse(value).ok())
        .unwrap_or_else(|| gdk::RGBA::parse(DEFAULT_COLOR).expect("valid default background"));
    let color = Rc::new(Cell::new(initial_color));

    let group = section(page, "Background");
    let preview_row = row(&group, false);
    preview_row.set_halign(gtk::Align::Center);
    let preview = gtk::Overlay::new();
    preview.add_css_class("prefs-wp-preview");
    preview.set_overflow(gtk::Overflow::Hidden);
    let background = gtk::DrawingArea::builder()
        .content_width(220)
        .content_height(124)
        .build();
    background.set_draw_func({
        let color = color.clone();
        move |_, context, _, _| {
            let color = color.get();
            context.set_source_rgb(
                color.red().into(),
                color.green().into(),
                color.blue().into(),
            );
            let _ = context.paint();
        }
    });
    preview.set_child(Some(&background));
    let picture = gtk::Picture::new();
    preview.add_overlay(&picture);
    preview_row.append(&preview);

    let image_row = row(&group, false);
    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.append(&label("Wallpaper Image", "prefs-row-title"));
    let path_label = label("", "prefs-muted");
    path_label.set_wrap(false);
    path_label.set_ellipsize(pango::EllipsizeMode::Middle);
    path_label.set_max_width_chars(32);
    info.append(&path_label);
    image_row.append(&info);
    let state = Rc::new(Wallpaper {
        config: path,
        status: status.clone(),
        path_label,
        picture,
        background,
        color,
    });
    page.connect_map({
        let state = state.clone();
        let path = Cell::new(Some(config["path"].as_str().unwrap_or_default().to_owned()));
        move |_| {
            if let Some(path) = path.take() {
                state.show_image(&path);
            }
        }
    });

    let choose = gtk::Button::with_mnemonic("_Choose…");
    choose.add_css_class("prefs-btn");
    choose.connect_clicked({
        let window = window.downgrade();
        let state = Rc::downgrade(&state);
        move |_| {
            let (Some(window), Some(wallpaper)) = (window.upgrade(), state.upgrade()) else {
                return;
            };
            if gio::SettingsSchemaSource::default()
                .and_then(|source| source.lookup("org.gtk.gtk4.Settings.FileChooser", true))
                .is_none()
            {
                show_error(
                    &wallpaper.status,
                    Some("The GTK file chooser settings schema is missing."),
                );
                return;
            }
            let chooser = gtk::FileChooserDialog::new(
                Some("Choose Wallpaper"),
                Some(&window),
                gtk::FileChooserAction::Open,
                &[
                    ("Cancel", gtk::ResponseType::Cancel),
                    ("Choose", gtk::ResponseType::Accept),
                ],
            );
            chooser.set_modal(true);
            chooser.set_destroy_with_parent(true);
            chooser.set_default_response(gtk::ResponseType::Accept);
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            filter.add_pixbuf_formats();
            chooser.add_filter(&filter);
            chooser.connect_response({
                let state = state.clone();
                move |chooser, response| {
                    if let (gtk::ResponseType::Accept, Some(state)) = (response, state.upgrade()) {
                        let filename = chooser.file().and_then(|file| file.path());
                        if let Some(path) = filename.as_deref().and_then(|path| path.to_str()) {
                            if state.save("path", path) {
                                state.show_image(path);
                            }
                        } else {
                            show_error(
                                &state.status,
                                Some("Choose an image with a valid local path."),
                            );
                        }
                    }
                    chooser.destroy();
                }
            });
            chooser.present();
        }
    });
    image_row.append(&choose);
    let clear = gtk::Button::with_mnemonic("C_lear");
    clear.add_css_class("prefs-btn");
    clear.connect_clicked({
        let state = state.clone();
        move |_| {
            if state.save("path", "") {
                state.show_image("");
            }
        }
    });
    image_row.append(&clear);

    let mode_row = row(&group, false);
    let mode_label = label("Display Mode", "prefs-row-title");
    mode_label.set_hexpand(true);
    mode_row.append(&mode_label);
    let mode = gtk::DropDown::from_strings(&MODES);
    mode.update_property(&[gtk::accessible::Property::Label("Display Mode")]);
    let initial = MODES
        .iter()
        .position(|value| Some(*value) == config["mode"].as_str())
        .unwrap_or_default() as u32;
    mode.set_selected(initial);
    mode.connect_selected_notify({
        let state = state.clone();
        let saved = Cell::new(initial);
        move |mode| {
            let selected = mode.selected();
            if selected == saved.get() {
                return;
            }
            if let Some(value) = MODES.get(selected as usize) {
                if state.save("mode", value) {
                    saved.set(selected);
                    return;
                }
            }
            mode.set_selected(saved.get());
        }
    });
    mode_row.append(&mode);

    let color_row = row(&group, true);
    let color_label = label("Background Color", "prefs-row-title");
    color_label.set_hexpand(true);
    color_row.append(&color_label);
    let color_button = gtk::ColorButton::with_rgba(&initial_color);
    color_button.set_title("Background Color");
    color_button.set_use_alpha(false);
    color_button.update_property(&[gtk::accessible::Property::Label("Background Color")]);
    color_button.connect_color_set(move |button| {
        let color = button.rgba();
        let [red, green, blue] =
            [color.red(), color.green(), color.blue()].map(|value| (value * 255.0).round() as u8);
        if state.save("color", &format!("#{red:02x}{green:02x}{blue:02x}")) {
            state.color.set(color);
            state.background.queue_draw();
        } else {
            button.set_rgba(&state.color.get());
        }
    });
    color_row.append(&color_button);
}

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("prefs-page-body");
    let status = label("", "error");
    status.set_visible(false);
    page.append(&status);
    desktop_appearance(&page, &status);
    wallpaper(&page, window, &status);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}
