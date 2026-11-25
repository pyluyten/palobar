use super::label;
use crate::quick_settings::{command, Volume};
use gtk::{glib, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Copy)]
enum Direction {
    Output,
    Input,
}

impl Direction {
    fn title(self) -> &'static str {
        match self {
            Self::Output => "Output",
            Self::Input => "Input",
        }
    }

    fn alias(self) -> &'static str {
        match self {
            Self::Output => "@DEFAULT_AUDIO_SINK@",
            Self::Input => "@DEFAULT_AUDIO_SOURCE@",
        }
    }

    fn icon(self, volume: Volume) -> &'static str {
        match self {
            Self::Output => volume.icon(),
            Self::Input if volume.muted => "microphone-sensitivity-muted-symbolic",
            Self::Input => "microphone-sensitivity-medium-symbolic",
        }
    }
}

struct Device {
    id: u32,
    name: String,
    default: bool,
    volume: Option<Volume>,
}

#[derive(Default)]
struct Devices {
    outputs: Vec<Device>,
    inputs: Vec<Device>,
}

fn parse_status(output: &str) -> Devices {
    let mut result = Devices::default();
    let mut section = None;
    let mut audio = false;
    for line in output.lines() {
        let line = line.trim();
        if line == "Audio" {
            audio = true;
            continue;
        }
        if !audio {
            continue;
        }
        if !line.is_empty() && !line.starts_with(['│', '├', '└']) {
            break;
        }
        let header = line.starts_with(['├', '└']);
        let line = line.trim_start_matches(['│', '├', '└', '─', ' ', '\t']);
        if header {
            section = match line {
                "Sinks:" => Some(Direction::Output),
                "Sources:" => Some(Direction::Input),
                _ => None,
            };
            continue;
        }
        let Some(direction) = section else { continue };
        let default = line.starts_with('*');
        let line = line.trim_start_matches('*').trim_start();
        let Some((id, description)) = line.split_once('.') else {
            continue;
        };
        let Ok(id) = id.parse() else { continue };
        let (name, volume) = description.trim().rsplit_once("[vol: ").map_or(
            (description.trim(), None),
            |(name, details)| {
                (
                    name.trim_end(),
                    Volume::parse(&format!("Volume: {}", details.trim_end_matches(']'))),
                )
            },
        );
        let device = Device {
            id,
            name: if name.is_empty() {
                format!("Device {id}")
            } else {
                name.to_owned()
            },
            default,
            volume,
        };
        match direction {
            Direction::Output => result.outputs.push(device),
            Direction::Input => result.inputs.push(device),
        }
    }
    result
}

async fn resolve_default(devices: &mut [Device], direction: Direction) {
    if devices.is_empty() {
        return;
    }
    if !devices.iter().any(|device| device.default) {
        let resolved = command(&["wpctl", "inspect", direction.alias()])
            .await
            .ok()
            .and_then(|output| {
                output
                    .lines()
                    .next()?
                    .strip_prefix("id ")?
                    .split_once(',')?
                    .0
                    .parse::<u32>()
                    .ok()
            });
        if let Some(device) = devices
            .iter_mut()
            .find(|device| Some(device.id) == resolved)
        {
            device.default = true;
        }
    }
    if let Some(device) = devices
        .iter_mut()
        .find(|device| device.default && device.volume.is_none())
    {
        device.volume = command(&["wpctl", "get-volume", &device.id.to_string()])
            .await
            .ok()
            .and_then(|output| Volume::parse(&output));
    }
}

#[derive(Default)]
struct Pending {
    default: Option<u32>,
    level: Option<(u32, f64)>,
    mute: Option<(u32, bool)>,
}

struct Controls {
    direction: Direction,
    dropdown: gtk::DropDown,
    mute: gtk::Button,
    scale: gtk::Scale,
    percent: gtk::Label,
    volume_row: gtk::Box,
    devices: RefCell<Vec<(u32, String)>>,
    current: Cell<Option<(u32, Volume)>>,
    syncing: Cell<bool>,
    pending: RefCell<Pending>,
    running: Cell<bool>,
    revision: Cell<u64>,
    app: glib::WeakRef<gtk::Application>,
}

impl Controls {
    fn new(page: &gtk::Box, direction: Direction, app: &gtk::Application) -> Rc<Self> {
        let title = direction.title();
        page.append(&label(title, "prefs-group-title"));
        let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
        group.add_css_class("prefs-group");
        let device_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        device_row.add_css_class("prefs-row");
        let device_title = label(&format!("{title} Device"), "prefs-row-title");
        device_title.set_hexpand(true);
        device_row.append(&device_title);
        let dropdown = gtk::DropDown::from_strings(&[&format!("No {title} Device")]);
        dropdown.set_sensitive(false);
        dropdown.set_size_request(200, -1);
        device_row.append(&dropdown);
        group.append(&device_row);

        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        volume_row.add_css_class("prefs-row");
        volume_row.add_css_class("last");
        let volume_title = label(&format!("{title} Volume"), "prefs-row-title");
        volume_row.append(&volume_title);
        let mute = gtk::Button::builder()
            .icon_name(direction.icon(Volume {
                level: 0.0,
                muted: false,
            }))
            .build();
        mute.add_css_class("prefs-btn");
        volume_row.append(&mute);
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.update_property(&[gtk::accessible::Property::Label(&format!("{title} Volume"))]);
        volume_row.append(&scale);
        let percent = label("0%", "prefs-muted");
        percent.set_width_chars(4);
        percent.set_xalign(1.0);
        volume_row.append(&percent);
        volume_row.set_sensitive(false);
        group.append(&volume_row);
        page.append(&group);

        let controls = Rc::new(Self {
            direction,
            dropdown,
            mute,
            scale,
            percent,
            volume_row,
            devices: RefCell::default(),
            current: Cell::default(),
            syncing: Cell::default(),
            pending: RefCell::default(),
            running: Cell::default(),
            revision: Cell::default(),
            app: app.downgrade(),
        });
        let weak = Rc::downgrade(&controls);
        controls.dropdown.connect_selected_notify(move |dropdown| {
            let Some(controls) = weak.upgrade() else {
                return;
            };
            if controls.syncing.get() {
                return;
            }
            let Some(id) = controls
                .devices
                .borrow()
                .get(dropdown.selected() as usize)
                .map(|(id, _)| *id)
            else {
                return;
            };
            if controls
                .current
                .get()
                .is_some_and(|(selected, _)| selected == id)
            {
                return;
            }
            controls.pending.borrow_mut().default = Some(id);
            controls.dropdown.set_sensitive(false);
            controls.volume_row.set_sensitive(false);
            controls.flush();
        });
        let weak = Rc::downgrade(&controls);
        controls.scale.connect_value_changed(move |scale| {
            let Some(controls) = weak.upgrade() else {
                return;
            };
            if controls.syncing.get() {
                return;
            }
            let Some((id, previous)) = controls.current.get() else {
                return;
            };
            let level = scale.value();
            let muted = previous.muted && level == 0.0;
            controls.render_volume(Some((id, Volume { level, muted })));
            let mut pending = controls.pending.borrow_mut();
            pending.level = Some((id, level));
            if previous.muted && !muted {
                pending.mute = Some((id, false));
            }
            drop(pending);
            controls.flush();
        });
        let weak = Rc::downgrade(&controls);
        controls.mute.connect_clicked(move |_| {
            let Some(controls) = weak.upgrade() else {
                return;
            };
            let Some((id, mut volume)) = controls.current.get() else {
                return;
            };
            volume.muted = !volume.muted;
            controls.render_volume(Some((id, volume)));
            controls.pending.borrow_mut().mute = Some((id, volume.muted));
            controls.flush();
        });
        controls
    }

    fn render_volume(&self, current: Option<(u32, Volume)>) {
        self.current.set(current);
        let volume = current.map_or(
            Volume {
                level: 0.0,
                muted: false,
            },
            |(_, volume)| volume,
        );
        let syncing = self.syncing.replace(true);
        self.scale.set_value(volume.level);
        self.percent
            .set_text(&format!("{:.0}%", volume.level * 100.0));
        self.mute.set_icon_name(self.direction.icon(volume));
        self.mute
            .set_tooltip_text(Some(if volume.muted { "Unmute" } else { "Mute" }));
        self.volume_row.set_sensitive(current.is_some());
        self.syncing.set(syncing);
    }

    fn render(&self, devices: Vec<Device>) {
        let entries: Vec<_> = devices
            .iter()
            .map(|device| (device.id, device.name.clone()))
            .collect();
        let selected = devices.iter().position(|device| device.default);
        self.syncing.set(true);
        if *self.devices.borrow() != entries {
            let none = format!("No {} Device", self.direction.title());
            let labels: Vec<_> = if entries.is_empty() {
                vec![none.as_str()]
            } else {
                entries.iter().map(|(_, name)| name.as_str()).collect()
            };
            self.dropdown
                .set_model(Some(&gtk::StringList::new(&labels)));
            self.devices.replace(entries);
        }
        self.dropdown.set_selected(if devices.is_empty() {
            0
        } else {
            selected.map_or(gtk::INVALID_LIST_POSITION, |selected| selected as u32)
        });
        self.dropdown.set_sensitive(!devices.is_empty());
        self.render_volume(selected.and_then(|index| {
            let device = &devices[index];
            device.volume.map(|volume| (device.id, volume))
        }));
        self.syncing.set(false);
    }

    fn flush(self: &Rc<Self>) {
        self.revision.set(self.revision.get().wrapping_add(1));
        if self.running.replace(true) {
            return;
        }
        let controls = self.clone();
        let hold = self.app.upgrade().map(|app| app.hold());
        glib::spawn_future_local(async move {
            loop {
                let Pending {
                    default,
                    level,
                    mute,
                } = controls.pending.take();
                if default.is_none() && level.is_none() && mute.is_none() {
                    break;
                }
                if let Some(id) = default {
                    invoke(&["wpctl", "set-default", &id.to_string()]).await;
                }
                if let Some((id, level)) = level {
                    invoke(&[
                        "wpctl",
                        "set-volume",
                        &id.to_string(),
                        &format!("{level:.2}"),
                    ])
                    .await;
                }
                if let Some((id, muted)) = mute {
                    invoke(&[
                        "wpctl",
                        "set-mute",
                        &id.to_string(),
                        if muted { "1" } else { "0" },
                    ])
                    .await;
                }
            }
            controls.running.set(false);
            drop(hold);
        });
    }
}

async fn invoke(args: &[&str]) {
    if let Err(error) = command(args).await {
        eprintln!("[sound] {error}");
    }
}

pub(super) fn build(app: &gtk::Application) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.add_css_class("prefs-page-body");
    let output = Controls::new(&page, Direction::Output, app);
    let input = Controls::new(&page, Direction::Input, app);
    let status = label("Audio is unavailable", "prefs-muted");
    status.set_visible(false);
    page.append(&status);
    let refresh = Rc::new(RefCell::new(None::<glib::JoinHandle<()>>));
    let cancel = refresh.clone();
    page.connect_unmap(move |_| {
        if let Some(task) = cancel.take() {
            task.abort();
        }
    });
    page.connect_map(move |_| {
        if refresh.borrow().is_some() {
            return;
        }
        let (output, input, status) = (output.clone(), input.clone(), status.clone());
        refresh.replace(Some(glib::spawn_future_local(async move {
            loop {
                let output_revision = output.revision.get();
                let input_revision = input.revision.get();
                let snapshot = command(&["wpctl", "status"]).await;
                status.set_visible(snapshot.is_err());
                let mut devices =
                    snapshot.map_or_else(|_| Devices::default(), |data| parse_status(&data));
                resolve_default(&mut devices.outputs, Direction::Output).await;
                resolve_default(&mut devices.inputs, Direction::Input).await;
                if !output.running.get() && output_revision == output.revision.get() {
                    output.render(devices.outputs);
                }
                if !input.running.get() && input_revision == input.revision.get() {
                    input.render(devices.inputs);
                }
                glib::timeout_future(Duration::from_millis(750)).await;
            }
        })));
    });
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn separates_audio_endpoints_and_preserves_names_and_mute() {
        let status = "Audio\n ├─ Sinks:\n │  *   52. Internal [USB]   [vol: 0.61 MUTED]\n ├─ Sources:\n │      53. Mic. Test [vol: 1.00]\n └─ Streams:\n │  99. Browser\n\nVideo\n ├─ Sources:\n │  10. Camera\n";
        let devices = parse_status(status);
        assert_eq!(devices.outputs.len(), 1);
        assert_eq!(devices.outputs[0].name, "Internal [USB]");
        assert!(devices.outputs[0].default);
        assert!(devices.outputs[0].volume.unwrap().muted);
        assert_eq!(devices.inputs.len(), 1);
        assert_eq!(devices.inputs[0].id, 53);
        assert_eq!(devices.inputs[0].name, "Mic. Test");
        assert_eq!(devices.inputs[0].volume.unwrap().level, 1.0);
    }
}
