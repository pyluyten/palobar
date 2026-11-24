use super::label;
use gtk::prelude::*;
use gtk::{gio, glib};
use sourceview5::prelude::{BufferExt, ViewExt};
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

async fn load_preview() -> Result<(gio::File, String, bool), String> {
    let process = gio::Subprocess::newv(
        &[OsStr::new("niri"), OsStr::new("validate")],
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_MERGE,
    )
    .map_err(|error| format!("Could not run niri validate: {error}"))?;
    let (log, _) = process
        .communicate_utf8_future(None)
        .await
        .map_err(|error| format!("Could not validate the Niri configuration: {error}"))?;
    let path = log
        .as_deref()
        .and_then(|log| {
            log.lines().find_map(|line| {
                let (_, path) = line.split_once("loaded config from \"")?;
                path.strip_suffix('"').map(PathBuf::from)
            })
        })
        .or_else(|| std::env::var_os("NIRI_CONFIG").map(PathBuf::from))
        .unwrap_or_else(|| glib::user_config_dir().join("niri/config.kdl"));
    let file = gio::File::for_path(path);
    let (bytes, _) = file
        .load_contents_future()
        .await
        .map_err(|error| format!("Could not read the Niri configuration: {error}"))?;
    Ok((
        file,
        String::from_utf8_lossy(&bytes).into_owned(),
        process.is_successful(),
    ))
}

fn open_editor(window: &gtk::ApplicationWindow, file: &gio::File) {
    let result = gio::AppInfo::default_for_type("text/plain", false)
        .ok_or_else(|| "No default text editor is configured.".to_owned())
        .and_then(|editor| {
            let context = WidgetExt::display(window).app_launch_context();
            editor
                .launch(&[file.clone()], Some(&context))
                .map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        let dialog = gtk::MessageDialog::builder()
            .transient_for(window)
            .modal(true)
            .destroy_with_parent(true)
            .buttons(gtk::ButtonsType::Close)
            .text("Could not open the Niri configuration")
            .secondary_text(error)
            .build();
        dialog.connect_response(|dialog, _| dialog.destroy());
        dialog.present();
    }
}

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("prefs-page-body");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = label("Niri Configuration", "prefs-group-title");
    title.set_hexpand(true);
    header.append(&title);
    let edit = gtk::Button::with_label("Edit");
    edit.add_css_class("prefs-btn");
    edit.set_sensitive(false);
    header.append(&edit);
    page.append(&header);
    let status = label("", "prefs-muted");
    status.set_visible(false);
    page.append(&status);

    sourceview5::init();
    let languages = sourceview5::LanguageManager::new();
    let language_dir = std::env::var("PALOBAR_GTKSOURCE_LANGUAGE_DIR").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .display()
            .to_string()
    });
    let defaults = languages.search_path();
    let search_path: Vec<&str> = std::iter::once(language_dir.as_str())
        .chain(defaults.iter().map(|dir| dir.as_str()))
        .collect();
    languages.set_search_path(&search_path);
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_language(languages.language("kdl").as_ref());
    let preview = sourceview5::View::with_buffer(&buffer);
    preview.set_show_line_numbers(true);
    preview.set_editable(false);
    preview.set_cursor_visible(false);
    preview.set_monospace(true);
    preview.set_wrap_mode(gtk::WrapMode::Char);
    preview.set_left_margin(8);
    preview.set_right_margin(8);
    preview.set_top_margin(8);
    page.append(
        &gtk::ScrolledWindow::builder()
            .min_content_height(232)
            .vexpand(true)
            .child(&preview)
            .build(),
    );

    let config = Rc::new(RefCell::new(None));
    edit.connect_clicked({
        let window = window.downgrade();
        let config = config.clone();
        move |_| {
            if let (Some(window), Some(file)) = (window.upgrade(), config.borrow().as_ref()) {
                open_editor(&window, file);
            }
        }
    });
    let loading = Rc::new(Cell::new(false));
    page.connect_map(move |_| {
        if loading.replace(true) {
            return;
        }
        config.replace(None);
        edit.set_sensitive(false);
        preview.set_tooltip_text(None);
        buffer.set_text("");
        status.set_text("Loading configuration…");
        status.set_visible(true);
        let loading = loading.clone();
        let config = config.clone();
        let (edit, preview, buffer, status) = (
            edit.downgrade(),
            preview.downgrade(),
            buffer.downgrade(),
            status.downgrade(),
        );
        glib::spawn_future_local(async move {
            let result = load_preview().await;
            loading.set(false);
            let (Some(edit), Some(preview), Some(buffer), Some(status)) = (
                edit.upgrade(),
                preview.upgrade(),
                buffer.upgrade(),
                status.upgrade(),
            ) else {
                return;
            };
            match result {
                Ok((file, text, valid)) => {
                    buffer.set_text(&text);
                    status.set_text("Niri configuration validation failed.");
                    status.set_visible(!valid);
                    preview.set_tooltip_text(file.path().as_deref().and_then(Path::to_str));
                    config.replace(Some(file));
                    edit.set_sensitive(true);
                }
                Err(error) => status.set_text(&error),
            }
        });
    });
    page
}
