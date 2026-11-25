use super::label;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

type Field = (&'static str, Option<String>);

fn read_text(path: impl AsRef<Path>) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn os_info(key: &str) -> Option<String> {
    glib::os_info(key).map(|value| value.to_string())
}

fn dmi(key: &str) -> Option<String> {
    read_text(Path::new("/sys/class/dmi/id").join(key)).filter(|value| {
        !matches!(
            value.to_ascii_lowercase().as_str(),
            "to be filled by o.e.m."
                | "default string"
                | "system manufacturer"
                | "system product name"
                | "not specified"
        )
    })
}

fn hardware_model() -> Option<String> {
    let (vendor, model) = match dmi("product_name") {
        Some(model) => (dmi("sys_vendor"), Some(model)),
        None => (dmi("board_vendor"), dmi("board_name")),
    };
    match (vendor, model) {
        (Some(vendor), Some(model)) if vendor != model => Some(format!("{vendor} {model}")),
        (vendor, model) => model.or(vendor),
    }
}

fn field_value<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let value = value.trim();
        (key.trim() == name && !value.is_empty()).then_some(value)
    })
}

fn processor() -> String {
    let count = glib::num_processors();
    let info = read_text("/proc/cpuinfo").unwrap_or_default();
    field_value(&info, "model name")
        .or_else(|| field_value(&info, "Hardware"))
        .map_or_else(
            || format!("{count} logical processors"),
            |model| format!("{model} × {count}"),
        )
}

fn memory() -> Option<String> {
    let info = read_text("/proc/meminfo")?;
    let kib = field_value(&info, "MemTotal")?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    (kib > 0)
        .then_some(kib)?
        .checked_mul(1024)
        .map(|bytes| glib::format_size_full(bytes, glib::FormatSizeFlags::IEC_UNITS).to_string())
}

fn storage() -> Option<String> {
    let info = gio::File::for_path("/")
        .query_filesystem_info("filesystem::size", gio::Cancellable::NONE)
        .ok()?;
    let bytes = info.attribute_uint64("filesystem::size");
    (bytes > 0).then(|| glib::format_size(bytes).to_string())
}

fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn niri_version() -> Option<String> {
    if let Some(version) = command("niri", &["msg", "-j", "version"])
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| {
            json["compositor"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
    {
        return Some(version);
    }
    command("niri", &["--version"])
        .map(|version| version.strip_prefix("niri ").unwrap_or(&version).to_owned())
}

fn graphics() -> Option<String> {
    let listing = command("lspci", &["-mm"]).unwrap_or_default();
    let mut names: BTreeSet<_> = listing
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('"').skip(1).step_by(2);
            if !matches!(
                columns.next()?,
                "VGA compatible controller" | "3D controller" | "Display controller"
            ) {
                return None;
            }
            Some(format!("{} {}", columns.next()?, columns.next()?))
        })
        .collect();
    if names.is_empty() {
        for entry in fs::read_dir("/sys/class/drm").ok()?.flatten() {
            let name = entry.file_name();
            let Some(index) = name.to_str().and_then(|name| name.strip_prefix("card")) else {
                continue;
            };
            if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let device = entry.path().join("device");
            if let (Some(vendor), Some(id)) = (
                read_text(device.join("vendor")),
                read_text(device.join("device")),
            ) {
                let vendor = vendor.trim_start_matches("0x");
                let id = id.trim_start_matches("0x");
                names.insert(format!("PCI {vendor}:{id}"));
            }
        }
    }
    (!names.is_empty()).then(|| names.into_iter().collect::<Vec<_>>().join("\n"))
}

fn value(field: &Field) -> &str {
    field.1.as_deref().unwrap_or("Unavailable")
}

fn group() -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    group
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

fn append_field(group: &gtk::Box, field: &Field, last: bool) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.set_hexpand(true);
    content.append(&label(field.0, "prefs-muted"));
    let value = label(value(field), "prefs-row-title");
    value.set_selectable(true);
    value.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    content.append(&value);
    row(group, last).append(&content);
}

fn column(title: &str, fields: &[Field]) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.append(&label(title, "prefs-group-title"));
    let group = group();
    for (index, field) in fields.iter().enumerate() {
        append_field(&group, field, index + 1 == fields.len());
    }
    column.append(&group);
    column
}

fn scroll(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}

fn present_details(parent: &gtk::ApplicationWindow, hardware: &[Field], software: &[Field]) {
    let dialog = gtk::Window::builder()
        .title("System Information")
        .transient_for(parent)
        .destroy_with_parent(true)
        .modal(true)
        .default_width(660)
        .default_height(460)
        .build();
    dialog.add_css_class("prefs-window");
    let header = gtk::HeaderBar::new();
    let copy = gtk::Button::with_mnemonic("_Copy");
    copy.add_css_class("prefs-btn");
    let report = [("Hardware", hardware), ("Software", software)]
        .into_iter()
        .map(|(title, fields)| {
            let mut lines = vec![title.to_owned()];
            lines.extend(
                fields
                    .iter()
                    .map(|field| format!("{}: {}", field.0, value(field))),
            );
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    copy.connect_clicked(move |button| button.clipboard().set_text(&report));
    header.pack_start(&copy);
    dialog.set_titlebar(Some(&header));

    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    columns.set_homogeneous(true);
    columns.add_css_class("prefs-page-body");
    columns.append(&column("Hardware", hardware));
    columns.append(&column("Software", software));
    dialog.set_child(Some(&scroll(&columns)));
    dialog.present();
}

fn populate(page: &gtk::Box, window: &gtk::ApplicationWindow) {
    let display = gtk::prelude::WidgetExt::display(window);
    let icons = gtk::IconTheme::for_display(&display);
    let logo = os_info("LOGO").filter(|name| icons.has_icon(name));
    let icon = gtk::Image::from_icon_name(logo.as_deref().unwrap_or("computer-symbolic"));
    icon.set_pixel_size(76);
    icon.set_halign(gtk::Align::Center);
    icon.set_margin_top(4);
    icon.set_margin_bottom(4);
    page.append(&icon);

    let os = (
        "Operating System",
        os_info("PRETTY_NAME").or_else(|| os_info("NAME")),
    );
    let hostname = ("Device Name", Some(glib::host_name().to_string()));
    let hardware = vec![
        ("Hardware Model", hardware_model()),
        ("Processor", Some(processor())),
        ("Memory", memory()),
        ("System Storage", storage()),
    ];
    let device = group();
    append_field(&device, &hostname, true);
    page.append(&device);
    let summary = group();
    for field in std::iter::once(&os).chain(&hardware) {
        append_field(&summary, field, false);
    }
    let details = gtk::Button::with_mnemonic("System _Information");
    details.add_css_class("prefs-btn");
    row(&summary, true).append(&details);
    page.append(&summary);

    let windowing = match display.type_().name() {
        "GdkWaylandDisplay" => Some("Wayland".to_owned()),
        "GdkX11Display" => Some("X11".to_owned()),
        _ => std::env::var("XDG_SESSION_TYPE").ok(),
    };
    let window = window.downgrade();
    details.connect_clicked(move |button| {
        button.set_sensitive(false);
        let button = button.downgrade();
        let window = window.clone();
        let (os, hostname, hardware, windowing) = (
            os.clone(),
            hostname.clone(),
            hardware.clone(),
            windowing.clone(),
        );
        glib::spawn_future_local(async move {
            let (graphics, niri, architecture) =
                gio::spawn_blocking(|| (graphics(), niri_version(), command("uname", &["-m"])))
                    .await
                    .unwrap_or_default();
            if let Some(button) = button.upgrade() {
                button.set_sensitive(true);
            }
            let Some(window) = window.upgrade().filter(|window| window.is_visible()) else {
                return;
            };
            let mut hardware = hardware;
            hardware.insert(0, hostname);
            hardware.extend([
                ("Graphics", graphics),
                ("Firmware Version", dmi("bios_version")),
            ]);
            let software = [
                os,
                ("OS Build", os_info("BUILD_ID")),
                ("Architecture", architecture),
                ("Niri Version", niri),
                (
                    "Palobar Version",
                    Some(include_str!("../../../../VERSION").trim().to_owned()),
                ),
                ("Windowing System", windowing),
                ("Kernel Version", read_text("/proc/sys/kernel/osrelease")),
            ];
            present_details(&window, &hardware, &software);
        });
    });
}

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.add_css_class("prefs-page-body");
    let window = window.downgrade();
    page.connect_map(move |page| {
        if page.first_child().is_none() {
            if let Some(window) = window.upgrade() {
                populate(page, &window);
            }
        }
    });
    scroll(&page)
}
