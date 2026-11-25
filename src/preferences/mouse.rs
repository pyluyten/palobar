use super::label;
use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

const FRAGMENT: &str = "palobar-cursor.kdl";
const HEADER: &str = "// Cursor settings managed by Palobar";
const INCLUDE: &str = "include \"palobar-cursor.kdl\"";
type Error = Box<dyn std::error::Error>;

fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("NIRI_CONFIG").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    let user = glib::user_config_dir().join("niri/config.kdl");
    let system = Path::new("/etc/niri/config.kdl");
    if !user.exists() && system.is_file() {
        system.to_owned()
    } else {
        user
    }
}

fn cursor_block(theme: &str, size: u32) -> String {
    let theme = serde_json::Value::from(theme);
    format!("cursor {{\n    xcursor-theme {theme}\n    xcursor-size {size}\n}}")
}

fn managed_settings(text: &str) -> Option<(String, u32)> {
    let mut lines = text.lines();
    if lines.next()? != HEADER || lines.next()? != "cursor {" {
        return None;
    }
    let theme = serde_json::from_str(lines.next()?.strip_prefix("    xcursor-theme ")?).ok()?;
    let size = lines
        .next()?
        .strip_prefix("    xcursor-size ")?
        .parse()
        .ok()?;
    (lines.next()? == "}" && lines.next().is_none()).then_some((theme, size))
}

fn writable_file(path: &Path) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.permissions().readonly() {
        return Err("Niri configuration is read-only or linked. Update its source using the KDL block below.".into());
    }
    Ok(())
}

struct ValidationFile(PathBuf);

impl ValidationFile {
    fn new(directory: &Path, contents: &str) -> Result<Self, Error> {
        let name = format!(".palobar-{}.kdl", glib::uuid_string_random());
        let path = directory.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let temporary = Self(path);
        file.write_all(contents.as_bytes())?;
        Ok(temporary)
    }
}

impl Drop for ValidationFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn validate(path: &Path) -> Result<(), Error> {
    let output = Command::new("niri")
        .arg("validate")
        .arg("-c")
        .arg(path)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    let log = String::from_utf8_lossy(&output.stderr);
    Err(format!("Niri rejected the candidate configuration.\n{}", log.trim()).into())
}

fn apply(path: &Path, block: &str) -> Result<(), Error> {
    writable_file(path)?;
    let directory = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let fragment_path = directory.join(FRAGMENT);
    let main = gio::File::for_path(path);
    let fragment = gio::File::for_path(&fragment_path);
    let (root, root_tag) = main.load_contents(gio::Cancellable::NONE)?;
    let root = std::str::from_utf8(&root)?;
    let contents = format!("{HEADER}\n{block}\n");
    let existing = root.trim_end().ends_with(&format!("\n{HEADER}\n{INCLUDE}"))
        || root.trim_end() == format!("{HEADER}\n{INCLUDE}");
    let fragment_tag = if existing {
        writable_file(&fragment_path)?;
        let (bytes, tag) = fragment.load_contents(gio::Cancellable::NONE)?;
        if managed_settings(std::str::from_utf8(&bytes)?).is_none() {
            return Err("The cursor include has been edited outside Palobar. Update it using the KDL block below.".into());
        }
        tag
    } else {
        if fs::symlink_metadata(&fragment_path).is_ok() || root.contains(FRAGMENT) {
            return Err(
                "A cursor include already exists. Update it using the KDL block below.".into(),
            );
        }
        None
    };
    let base = if existing {
        &root[..root
            .rfind(INCLUDE)
            .ok_or("Could not locate the cursor include")?]
    } else {
        root
    };
    let staged = ValidationFile::new(directory, &contents)?;
    let staged_name = staged
        .0
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid temporary filename")?;
    let candidate =
        ValidationFile::new(directory, &format!("{base}\ninclude \"{staged_name}\"\n"))?;
    validate(&candidate.0)?;
    if existing {
        fragment.replace_contents(
            contents.as_bytes(),
            fragment_tag.as_deref(),
            false,
            gio::FileCreateFlags::NONE,
            gio::Cancellable::NONE,
        )?;
    } else {
        fs::hard_link(&staged.0, &fragment_path)?;
        if let Err(error) = main.replace_contents(
            format!("{root}\n{HEADER}\n{INCLUDE}\n").as_bytes(),
            root_tag.as_deref(),
            true,
            gio::FileCreateFlags::NONE,
            gio::Cancellable::NONE,
        ) {
            let _ = fs::remove_file(&fragment_path);
            return Err(error.into());
        }
    }
    Ok(())
}

fn roots() -> Vec<PathBuf> {
    let home = glib::home_dir();
    let mut roots: Vec<_> = std::env::var_os("XCURSOR_PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| {
            if let Ok(suffix) = path.strip_prefix("~") {
                home.join(suffix)
            } else {
                path
            }
        })
        .collect();
    roots.push(home.join(".icons"));
    roots.push(glib::user_data_dir().join("icons"));
    roots.extend(
        glib::system_data_dirs()
            .iter()
            .map(|path| path.join("icons")),
    );
    roots
}

fn valid_theme(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

fn themes(roots: &[PathBuf]) -> BTreeSet<String> {
    roots
        .iter()
        .flat_map(|root| fs::read_dir(root).into_iter().flatten().flatten())
        .filter(|entry| entry.path().join("cursors").is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_theme(name))
        .collect()
}

fn cursor_bytes(roots: &[PathBuf], theme: &str) -> Option<Vec<u8>> {
    let mut pending = VecDeque::from([theme.to_owned()]);
    let mut visited = HashSet::new();
    while let Some(theme) = pending.pop_front() {
        if !valid_theme(&theme) || !visited.insert(theme.clone()) {
            continue;
        }
        for root in roots {
            let directory = root.join(&theme);
            for name in ["left_ptr", "default"] {
                let path = directory.join("cursors").join(name);
                if path.is_file() {
                    let mut bytes = Vec::new();
                    if File::open(path)
                        .and_then(|file| file.take(8 << 20).read_to_end(&mut bytes))
                        .is_ok()
                        && bytes.starts_with(b"Xcur")
                    {
                        return Some(bytes);
                    }
                }
            }
            let index = glib::KeyFile::new();
            if index
                .load_from_file(directory.join("index.theme"), glib::KeyFileFlags::NONE)
                .is_ok()
            {
                if let Ok(inherits) = index.string("Icon Theme", "Inherits") {
                    pending.extend(inherits.split([',', ';']).map(str::trim).map(str::to_owned));
                }
            }
        }
    }
    None
}

fn render(bytes: &[u8], size: u32) -> Option<(gdk::Texture, gdk::Cursor)> {
    let read = |offset: usize| {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let table = read(4)? as usize;
    let count = read(12)? as usize;
    if table < 16 || count > bytes.len().saturating_sub(table) / 12 {
        return None;
    }
    const IMAGE: u32 = 0xfffd0002;
    let (nominal, position) = (0..count)
        .filter_map(|index| {
            let entry = table + index * 12;
            let nominal = read(entry + 4)?;
            let position = read(entry + 8)? as usize;
            (read(entry)? == IMAGE && nominal > 0 && bytes.len().saturating_sub(position) >= 36)
                .then_some((nominal, position))
        })
        .min_by_key(|(nominal, _)| nominal.abs_diff(size))?;
    let header = read(position)? as usize;
    if header < 36 || read(position + 4)? != IMAGE || read(position + 8)? != nominal {
        return None;
    }
    let (width, height) = (read(position + 16)?, read(position + 20)?);
    let (x, y) = (read(position + 24)?, read(position + 28)?);
    if width > 1024 || height > 1024 || x >= width || y >= height {
        return None;
    }
    let start = position.checked_add(header)?;
    let pixels = bytes.get(start..start.checked_add((width * height * 4) as usize)?)?;
    let pixels = glib::Bytes::from(pixels);
    let texture: gdk::Texture = gdk::MemoryTexture::new(
        width as i32,
        height as i32,
        gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &pixels,
        width as usize * 4,
    )
    .upcast();
    let scaled = |value: u32| {
        ((f64::from(value) * f64::from(size) / f64::from(nominal)).round() as i32).clamp(1, 256)
    };
    let (scaled_width, scaled_height) = (scaled(width), scaled(height));
    let texture = if scaled_width == width as i32 && scaled_height == height as i32 {
        texture
    } else {
        let pixbuf = gdk::pixbuf_get_from_texture(&texture)?.scale_simple(
            scaled_width,
            scaled_height,
            gdk_pixbuf::InterpType::Bilinear,
        )?;
        gdk::Texture::for_pixbuf(&pixbuf)
    };
    let hotspot = |point: u32, old: u32, new: i32| {
        ((f64::from(point) * f64::from(new) / f64::from(old)).round() as i32).min(new - 1)
    };
    let cursor = gdk::Cursor::from_texture(
        &texture,
        hotspot(x, width, scaled_width),
        hotspot(y, height, scaled_height),
        None,
    );
    Some((texture, cursor))
}

fn row(group: &gtk::Box, title: &str, hint: &str, last: bool) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("prefs-row");
    if last {
        row.add_css_class("last");
    }
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(&label(title, "prefs-row-title"));
    text.append(&label(hint, "prefs-muted"));
    row.append(&text);
    group.append(&row);
    row
}

pub(super) fn build() -> gtk::ScrolledWindow {
    let path = config_path();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.add_css_class("prefs-page-body");
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    page.append(&group);
    let theme = gtk::DropDown::builder()
        .enable_search(true)
        .width_request(180)
        .build();
    theme.update_property(&[gtk::accessible::Property::Label("Cursor Theme")]);
    row(&group, "Cursor Theme", "XCURSOR_THEME", false).append(&theme);
    let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 8.0, 96.0, 1.0);
    size.set_draw_value(true);
    size.set_digits(0);
    size.set_format_value_func(|_, value| format!("{value:.0} px"));
    size.set_width_request(180);
    size.update_property(&[gtk::accessible::Property::Label("Cursor Size")]);
    row(&group, "Cursor Size", "XCURSOR_SIZE", true).append(&size);

    let preview = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    preview.add_css_class("prefs-group");
    preview.add_css_class("prefs-row");
    preview.add_css_class("last");
    preview.set_height_request(108);
    let image = gtk::Picture::new();
    image.set_halign(gtk::Align::Start);
    image.set_valign(gtk::Align::Center);
    preview.append(&image);
    let hint = label("", "prefs-muted");
    hint.set_hexpand(true);
    hint.set_xalign(0.5);
    preview.append(&hint);
    page.append(&preview);

    page.append(&label(
        "Apply the settings to Niri, or copy the KDL block into your Niri configuration. Apps already running may need to be restarted.",
        "prefs-muted",
    ));
    let target = label(&path.to_string_lossy(), "prefs-muted");
    target.set_selectable(true);
    page.append(&target);
    let example = label("", "monospace");
    example.set_selectable(true);
    example.add_css_class("prefs-group");
    example.add_css_class("prefs-row");
    example.add_css_class("last");
    page.append(&example);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let copy = gtk::Button::with_label("Copy KDL");
    let apply_button = gtk::Button::with_mnemonic("_Apply to Niri");
    for button in [&copy, &apply_button] {
        button.add_css_class("prefs-btn");
        actions.append(button);
    }
    page.append(&actions);
    let status = label("", "prefs-muted");
    status.set_selectable(true);
    page.append(&status);

    let cached = Rc::new(RefCell::new((String::new(), None)));
    let roots = Rc::new(roots());
    let update = {
        let cached = cached.clone();
        let roots = roots.clone();
        let (preview, image, hint, example) = (
            preview.clone(),
            image.clone(),
            hint.clone(),
            example.clone(),
        );
        move |theme: &gtk::DropDown, size: &gtk::Scale| {
            let Some(item) = theme.selected_item().and_downcast::<gtk::StringObject>() else {
                return;
            };
            let name = item.string();
            let size = size.value().round() as u32;
            example.set_text(&cursor_block(&name, size));
            let mut cached = cached.borrow_mut();
            if cached.0 != name {
                *cached = (name.to_string(), cursor_bytes(&roots, &name));
            }
            if let Some((texture, cursor)) =
                cached.1.as_deref().and_then(|bytes| render(bytes, size))
            {
                image.set_paintable(Some(&texture));
                preview.set_cursor(Some(&cursor));
                hint.set_text("Move the pointer here to preview the cursor.");
            } else {
                image.set_paintable(gdk::Paintable::NONE);
                preview.set_cursor(None);
                hint.set_text("No pointer preview is available for this theme.");
            }
        }
    };
    theme.connect_selected_item_notify({
        let size = size.downgrade();
        let update = update.clone();
        move |theme| {
            if let Some(size) = size.upgrade() {
                update(theme, &size);
            }
        }
    });
    size.connect_value_changed({
        let theme = theme.downgrade();
        let update = update.clone();
        move |size| {
            if let Some(theme) = theme.upgrade() {
                update(&theme, size);
            }
        }
    });
    let initial_path = path.clone();
    page.connect_map(move |_| {
        if theme.model().is_none() {
            let initial = fs::read_to_string(&initial_path)
                .ok()
                .filter(|root| root.trim_end().ends_with(&format!("{HEADER}\n{INCLUDE}")))
                .and_then(|_| fs::read_to_string(initial_path.with_file_name(FRAGMENT)).ok())
                .and_then(|text| managed_settings(&text));
            let settings = gtk::Settings::default();
            let (name, pixels) = initial.unwrap_or_else(|| {
                let name = std::env::var("XCURSOR_THEME")
                    .ok()
                    .filter(|name| valid_theme(name))
                    .or_else(|| {
                        settings
                            .as_ref()
                            .and_then(|settings| settings.gtk_cursor_theme_name())
                            .map(|name| name.to_string())
                    })
                    .filter(|name| valid_theme(name))
                    .unwrap_or_else(|| "default".to_owned());
                let pixels = std::env::var("XCURSOR_SIZE")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .filter(|size: &u32| *size > 0)
                    .or_else(|| {
                        settings.as_ref().and_then(|settings| {
                            u32::try_from(settings.gtk_cursor_theme_size()).ok()
                        })
                    })
                    .filter(|size| *size > 0)
                    .unwrap_or(24);
                (name, pixels)
            });
            let mut names = themes(&roots);
            names.insert(name.clone());
            let names: Vec<_> = names.iter().map(String::as_str).collect();
            size.set_range(f64::from(pixels.min(8)), f64::from(pixels.max(96)));
            size.set_value(pixels.into());
            theme.set_model(Some(&gtk::StringList::new(&names)));
            theme.set_selected(
                names
                    .iter()
                    .position(|item| *item == name)
                    .unwrap_or_default() as u32,
            );
        }
        update(&theme, &size);
    });
    page.connect_unmap(move |_| {
        *cached.borrow_mut() = (String::new(), None);
        image.set_paintable(gdk::Paintable::NONE);
        preview.set_cursor(None);
    });
    copy.connect_clicked({
        let example = example.clone();
        move |button| button.clipboard().set_text(&example.text())
    });
    apply_button.connect_clicked(move |button| {
        let block = example.text().to_string();
        if block.is_empty() {
            return;
        }
        let path = path.clone();
        let status = status.downgrade();
        let button = button.downgrade();
        if let Some(button) = button.upgrade() {
            button.set_sensitive(false);
        }
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                apply(&path, &block).map_err(|error| error.to_string())
            })
            .await;
            if let Some(status) = status.upgrade() {
                match result {
                    Ok(Ok(())) => status.set_text(
                        "Saved. Niri reloads the settings if this configuration is active.",
                    ),
                    Ok(Err(error)) => {
                        status.set_text(&format!("Could not apply cursor settings. {error}"))
                    }
                    Err(_) => status.set_text("Could not apply cursor settings."),
                }
            }
            if let Some(button) = button.upgrade() {
                button.set_sensitive(true);
            }
        });
    });
    gtk::ScrolledWindow::builder()
        .child(&page)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build()
}
