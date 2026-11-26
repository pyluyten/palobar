use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::rc::Rc;
use std::time::Duration;

pub const ACTION: &str = "toggle-quick-settings";
const TITLE: &str = "Quick Settings";
const CLASS: &str = "quick-settings-window";
const SINK: &str = "@DEFAULT_AUDIO_SINK@";

pub(crate) async fn command(args: &[&str]) -> Result<String, String> {
    let argv: Vec<_> = args.iter().map(OsStr::new).collect();
    let launcher = gio::SubprocessLauncher::new(
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    );
    if matches!(args[0], "wpctl" | "nmcli") {
        launcher.setenv("LC_ALL", "C", true);
    }
    let child = CommandChild(launcher.spawn(&argv).map_err(|error| error.to_string())?);
    let process = &child.0;
    let (stdout, stderr) = process
        .communicate_utf8_future(None)
        .await
        .map_err(|error| error.to_string())?;
    if process.is_successful() {
        Ok(stdout
            .as_deref()
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_owned())
    } else {
        Err(format!(
            "{} failed: {}",
            args[0],
            stderr.as_deref().unwrap_or_default().trim()
        ))
    }
}

struct CommandChild(gio::Subprocess);

impl Drop for CommandChild {
    fn drop(&mut self) {
        self.0.force_exit();
    }
}

fn report(error: &str) {
    eprintln!("[quick-settings] {error}");
}

fn active(widget: &impl IsA<gtk::Widget>, value: bool) {
    if value {
        widget.add_css_class("active");
    } else {
        widget.remove_css_class("active");
    }
}

fn icon_button(icon: &str, tooltip: &str, class: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    button.add_css_class(class);
    button
}

fn labeled_button(icon: &str, label: &str, class: &str) -> (gtk::Button, gtk::Image) {
    let image = gtk::Image::from_icon_name(icon);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Start);
    row.append(&image);
    row.append(&gtk::Label::new(Some(label)));
    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class(class);
    (button, image)
}

fn session_button(
    button: &gtk::Button,
    window: &gtk::ApplicationWindow,
    args: Vec<&'static str>,
    screenshot: bool,
) {
    let window = window.downgrade();
    button.connect_clicked(move |_| {
        let Some(window) = window.upgrade() else {
            return;
        };
        let Some(app) = window.application() else {
            return;
        };
        let hold = app.hold();
        let args = args.clone();
        glib::spawn_future_local(async move {
            if screenshot {
                glib::timeout_future(Duration::from_millis(150)).await;
            }
            if let Err(error) = command(&args).await {
                report(&error);
            }
            drop(hold);
        });
        window.close();
    });
}

fn header(card: &gtk::Box, window: &gtk::ApplicationWindow, battery: &Battery) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&battery.row);
    let screenshot = icon_button(
        "applets-screenshooter-symbolic",
        "Screenshot",
        "qs-circle-btn",
    );
    session_button(
        &screenshot,
        window,
        vec!["niri", "msg", "action", "screenshot"],
        true,
    );
    row.append(&screenshot);

    let prefs = icon_button(
        "preferences-system-symbolic",
        "Preferences",
        "qs-circle-btn",
    );
    let weak_window = window.downgrade();
    prefs.connect_clicked(move |_| match crate::preferences::launch() {
        Ok(()) => {
            if let Some(window) = weak_window.upgrade() {
                window.close();
            }
        }
        Err(error) => report(&error),
    });
    row.append(&prefs);
    row.append(&gtk::Box::builder().hexpand(true).build());

    let lock = icon_button(
        "system-lock-screen-symbolic",
        "Lock Screen",
        "qs-circle-btn",
    );
    let weak_window = window.downgrade();
    lock.connect_clicked(move |_| match crate::lockscreen::launch() {
        Ok(()) => {
            if let Some(window) = weak_window.upgrade() {
                window.close();
            }
        }
        Err(error) => report(&error),
    });
    row.append(&lock);

    let power = icon_button("system-shutdown-symbolic", "Power Options", "qs-circle-btn");
    row.append(&power);
    card.append(&row);

    let menu = gtk::Box::new(gtk::Orientation::Vertical, 4);
    menu.add_css_class("qs-submenu");
    menu.set_visible(false);
    for (icon, label, args) in [
        (
            "system-log-out-symbolic",
            "Log Out",
            vec!["niri", "msg", "action", "quit", "--skip-confirmation"],
        ),
        (
            "media-playback-pause-symbolic",
            "Suspend",
            vec!["systemctl", "suspend"],
        ),
        (
            "system-reboot-symbolic",
            "Restart",
            vec!["systemctl", "reboot"],
        ),
        (
            "system-shutdown-symbolic",
            "Power Off",
            vec!["systemctl", "poweroff"],
        ),
    ] {
        let (button, _) = labeled_button(icon, label, "qs-power-item");
        if label == "Power Off" {
            button.add_css_class("danger");
        }
        session_button(&button, window, args, false);
        menu.append(&button);
    }
    let weak_menu = menu.downgrade();
    power.connect_clicked(move |button| {
        if let Some(menu) = weak_menu.upgrade() {
            let show = !menu.is_visible();
            menu.set_visible(show);
            active(button, show);
        }
    });
    card.append(&menu);
}

#[derive(Clone, Copy)]
pub(crate) struct Volume {
    pub level: f64,
    pub muted: bool,
}

impl Volume {
    pub(crate) fn parse(output: &str) -> Option<Self> {
        let level = output
            .strip_prefix("Volume: ")?
            .split_whitespace()
            .next()?
            .parse::<f64>()
            .ok()?;
        level.is_finite().then_some(Self {
            level,
            muted: output
                .split_whitespace()
                .any(|word| word.trim_matches(['[', ']']) == "MUTED"),
        })
    }

    pub(crate) fn icon(self) -> &'static str {
        if self.muted || self.level <= 0.0 {
            "audio-volume-muted-symbolic"
        } else if self.level < 0.33 {
            "audio-volume-low-symbolic"
        } else if self.level < 0.67 {
            "audio-volume-medium-symbolic"
        } else {
            "audio-volume-high-symbolic"
        }
    }
}

pub fn volume_icon(output: &str) -> &'static str {
    Volume::parse(output)
        .map(Volume::icon)
        .unwrap_or("audio-volume-medium-symbolic")
}

#[derive(Default)]
struct AudioQueue {
    level: Cell<Option<f64>>,
    muted: Cell<Option<bool>>,
    running: Cell<bool>,
    revision: Cell<u64>,
}

struct Audio {
    scale: gtk::Scale,
    mute: gtk::Button,
    state: Cell<Volume>,
    syncing: Cell<bool>,
    queue: Rc<AudioQueue>,
    app: glib::WeakRef<gtk::Application>,
}

impl Audio {
    fn new(card: &gtk::Box, app: &gtk::Application) -> Rc<Self> {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("qs-volume");
        let mute = icon_button("audio-volume-medium-symbolic", "Mute", "qs-icon-btn");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        scale.set_hexpand(true);
        scale.set_draw_value(false);
        scale.add_css_class("qs-slider");
        mute.set_sensitive(false);
        scale.set_sensitive(false);
        row.append(&mute);
        row.append(&scale);
        card.append(&row);

        let audio = Rc::new(Self {
            mute,
            scale,
            state: Cell::new(Volume {
                level: 0.0,
                muted: false,
            }),
            syncing: Cell::new(false),
            queue: Rc::default(),
            app: app.downgrade(),
        });
        let weak = Rc::downgrade(&audio);
        audio.scale.connect_value_changed(move |scale| {
            let Some(audio) = weak.upgrade() else { return };
            if audio.syncing.get() {
                return;
            }
            let level = scale.value();
            let was_muted = audio.state.get().muted;
            let muted = was_muted && level == 0.0;
            audio.render(Volume { level, muted });
            audio.queue.level.set(Some(level));
            if was_muted && !muted {
                audio.queue.muted.set(Some(false));
            }
            audio.flush();
        });
        let weak = Rc::downgrade(&audio);
        audio.mute.connect_clicked(move |_| {
            if let Some(audio) = weak.upgrade() {
                let mut state = audio.state.get();
                state.muted = !state.muted;
                audio.render(state);
                audio.queue.muted.set(Some(state.muted));
                audio.flush();
            }
        });
        audio
    }

    fn render(&self, volume: Volume) {
        self.state.set(volume);
        self.syncing.set(true);
        self.scale.set_value(volume.level);
        self.syncing.set(false);
        self.scale
            .set_tooltip_text(Some(&format!("{:.0}%", volume.level * 100.0)));
        self.mute.set_icon_name(volume.icon());
        self.mute
            .set_tooltip_text(Some(if volume.muted { "Unmute" } else { "Mute" }));
        if volume.muted {
            self.mute.add_css_class("muted");
        } else {
            self.mute.remove_css_class("muted");
        }
    }

    fn flush(&self) {
        self.queue
            .revision
            .set(self.queue.revision.get().wrapping_add(1));
        if self.queue.running.replace(true) {
            return;
        }
        let queue = self.queue.clone();
        let hold = self.app.upgrade().map(|app| app.hold());
        glib::spawn_future_local(async move {
            loop {
                let level = queue.level.take();
                let muted = queue.muted.take();
                if level.is_none() && muted.is_none() {
                    break;
                }
                if let Some(level) = level {
                    if let Err(error) =
                        command(&["wpctl", "set-volume", SINK, &format!("{level:.2}")]).await
                    {
                        report(&error);
                    }
                }
                if let Some(muted) = muted {
                    if let Err(error) =
                        command(&["wpctl", "set-mute", SINK, if muted { "1" } else { "0" }]).await
                    {
                        report(&error);
                    }
                }
            }
            queue.running.set(false);
            drop(hold);
        });
    }

    async fn refresh(&self) {
        if self.queue.running.get() {
            return;
        }
        let revision = self.queue.revision.get();
        let volume = command(&["wpctl", "get-volume", SINK])
            .await
            .ok()
            .and_then(|output| Volume::parse(&output));
        if revision != self.queue.revision.get() {
            return;
        }
        self.mute.set_sensitive(volume.is_some());
        self.scale.set_sensitive(volume.is_some());
        if let Some(volume) = volume {
            self.render(volume);
        }
    }
}

#[derive(Clone, Copy)]
struct BatteryStatus {
    percent: u8,
    charging: bool,
}

fn battery_status() -> Option<BatteryStatus> {
    let supplies = std::fs::read_dir("/sys/class/power_supply").ok()?;
    let mut capacities = Vec::new();
    let mut energy_now = 0_u64;
    let mut energy_full = 0_u64;
    let mut charging = false;
    for supply in supplies.flatten() {
        let path = supply.path();
        if std::fs::read_to_string(path.join("type")).ok()?.trim() != "Battery" {
            continue;
        }
        if let Ok(value) = std::fs::read_to_string(path.join("capacity")) {
            if let Ok(value) = value.trim().parse::<u8>() {
                capacities.push(value);
            }
        }
        let now = std::fs::read_to_string(path.join("energy_now"))
            .or_else(|_| std::fs::read_to_string(path.join("charge_now")))
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok());
        let full = std::fs::read_to_string(path.join("energy_full"))
            .or_else(|_| std::fs::read_to_string(path.join("charge_full")))
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok());
        if let (Some(now), Some(full)) = (now, full) {
            energy_now = energy_now.saturating_add(now);
            energy_full = energy_full.saturating_add(full);
        }
        charging |= std::fs::read_to_string(path.join("status"))
            .is_ok_and(|status| status.trim() == "Charging");
    }
    let percent = if energy_full > 0 {
        ((energy_now.saturating_mul(100) / energy_full).min(100)) as u8
    } else if !capacities.is_empty() {
        (capacities
            .iter()
            .map(|value| u16::from(*value))
            .sum::<u16>()
            / capacities.len() as u16) as u8
    } else {
        return None;
    };
    Some(BatteryStatus { percent, charging })
}

fn battery_icon(status: BatteryStatus) -> &'static str {
    match (status.charging, status.percent) {
        (true, 90..=100) => "battery-level-100-plugged-in-symbolic",
        (true, 70..=89) => "battery-level-80-charging-symbolic",
        (true, 50..=69) => "battery-level-60-charging-symbolic",
        (true, 30..=49) => "battery-level-40-charging-symbolic",
        (true, 10..=29) => "battery-level-20-charging-symbolic",
        (true, _) => "battery-level-0-charging-symbolic",
        (false, 90..=100) => "battery-level-100-symbolic",
        (false, 70..=89) => "battery-level-80-symbolic",
        (false, 50..=69) => "battery-level-60-symbolic",
        (false, 30..=49) => "battery-level-40-symbolic",
        (false, 10..=29) => "battery-level-20-symbolic",
        (false, _) => "battery-level-0-symbolic",
    }
}

struct Battery {
    row: gtk::Box,
    icon: gtk::Image,
    label: gtk::Label,
}

impl Battery {
    fn new() -> Self {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("qs-battery");
        row.set_halign(gtk::Align::Start);
        row.set_visible(false);
        let icon = gtk::Image::from_icon_name("battery-level-100-symbolic");
        let label = gtk::Label::new(None);
        row.append(&icon);
        row.append(&label);
        Self { row, icon, label }
    }

    fn refresh(&self) {
        if let Some(status) = battery_status() {
            self.icon.set_icon_name(Some(battery_icon(status)));
            self.label.set_text(&format!("{}%", status.percent));
            self.row.set_tooltip_text(Some(if status.charging {
                "Battery charging"
            } else {
                "Battery"
            }));
            self.row.set_visible(true);
        } else {
            self.row.set_visible(false);
        }
    }
}

fn parse_brightness(output: &str) -> Option<f64> {
    output.lines().find_map(|line| {
        let fields: Vec<_> = line.split(',').collect();
        if fields.get(1)? != &"backlight" {
            return None;
        }
        if let Some(percent) = fields
            .get(3)
            .filter(|value| value.ends_with('%'))
            .and_then(|value| value.trim_end_matches('%').parse::<f64>().ok())
        {
            return percent
                .is_finite()
                .then_some((percent / 100.0).clamp(0.0, 1.0));
        }
        let current = fields.get(2)?.parse::<f64>().ok()?;
        let maximum = fields.get(3)?.parse::<f64>().ok()?;
        (maximum > 0.0 && current.is_finite()).then_some((current / maximum).clamp(0.0, 1.0))
    })
}

fn airplane_status(wifi: &str, wwan: Option<&str>) -> Option<bool> {
    match wifi {
        "enabled" => Some(false),
        "disabled" => Some(wwan != Some("enabled")),
        _ => None,
    }
}

#[derive(Default)]
struct BrightnessQueue {
    level: Cell<Option<u8>>,
    running: Cell<bool>,
    revision: Cell<u64>,
}

struct Brightness {
    row: gtk::Box,
    scale: gtk::Scale,
    syncing: Cell<bool>,
    queue: Rc<BrightnessQueue>,
    app: glib::WeakRef<gtk::Application>,
}

impl Brightness {
    fn new(card: &gtk::Box, app: &gtk::Application) -> Rc<Self> {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("qs-volume");
        row.set_visible(false);
        let icon = gtk::Image::from_icon_name("display-brightness-symbolic");
        icon.add_css_class("qs-brightness-icon");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        scale.set_hexpand(true);
        scale.set_draw_value(false);
        scale.add_css_class("qs-slider");
        scale.set_sensitive(false);
        row.append(&icon);
        row.append(&scale);
        card.append(&row);
        let brightness = Rc::new(Self {
            row,
            scale,
            syncing: Cell::new(false),
            queue: Rc::default(),
            app: app.downgrade(),
        });
        let weak = Rc::downgrade(&brightness);
        brightness.scale.connect_value_changed(move |scale| {
            let Some(brightness) = weak.upgrade() else {
                return;
            };
            if brightness.syncing.get() {
                return;
            }
            brightness
                .queue
                .level
                .set(Some((scale.value() * 100.0).round() as u8));
            brightness.flush();
        });
        brightness
    }

    fn render(&self, level: f64) {
        self.syncing.set(true);
        self.scale.set_value(level);
        self.syncing.set(false);
        self.scale
            .set_tooltip_text(Some(&format!("{:.0}%", level * 100.0)));
    }

    fn flush(&self) {
        self.queue
            .revision
            .set(self.queue.revision.get().wrapping_add(1));
        if self.queue.running.replace(true) {
            return;
        }
        let queue = self.queue.clone();
        let hold = self.app.upgrade().map(|app| app.hold());
        glib::spawn_future_local(async move {
            while let Some(level) = queue.level.take() {
                if let Err(error) = command(&["brightnessctl", "set", &format!("{level}%")]).await {
                    report(&error);
                }
            }
            queue.running.set(false);
            drop(hold);
        });
    }

    async fn refresh(&self) {
        if self.queue.running.get() {
            return;
        }
        let revision = self.queue.revision.get();
        let level = command(&["brightnessctl", "-m"])
            .await
            .ok()
            .and_then(|output| parse_brightness(&output));
        if revision != self.queue.revision.get() {
            return;
        }
        self.row.set_visible(level.is_some());
        self.scale.set_sensitive(level.is_some());
        if let Some(level) = level {
            self.render(level);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AccessPoint {
    ssid: String,
    strength: u8,
    connected: bool,
}

fn nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for ch in line.chars() {
        if ch == ':' && !escaped {
            fields.push(String::new());
        } else if ch == '\\' && !escaped {
            escaped = true;
            continue;
        } else if let Some(field) = fields.last_mut() {
            field.push(ch);
        }
        escaped = false;
    }
    fields
}

fn access_points(output: &str) -> Vec<AccessPoint> {
    let mut unique: HashMap<String, AccessPoint> = HashMap::new();
    for line in output.lines() {
        let fields = nmcli_fields(line);
        let [active, ssid, signal] = fields.as_slice() else {
            continue;
        };
        let Ok(strength) = signal.parse::<u8>() else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let connected = active == "*";
        let ap = unique.entry(ssid.clone()).or_insert_with(|| AccessPoint {
            ssid: ssid.clone(),
            strength,
            connected,
        });
        ap.connected |= connected;
        ap.strength = ap.strength.max(strength);
    }
    let mut list: Vec<_> = unique.into_values().collect();
    list.sort_unstable_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    list
}

fn wifi_icon(signal: u8) -> &'static str {
    match signal {
        75.. => "network-wireless-signal-excellent-symbolic",
        50..=74 => "network-wireless-signal-good-symbolic",
        25..=49 => "network-wireless-signal-ok-symbolic",
        1..=24 => "network-wireless-signal-weak-symbolic",
        _ => "network-wireless-signal-none-symbolic",
    }
}

struct Wifi {
    button: gtk::Button,
    chevron: gtk::Button,
    icon: gtk::Image,
    subtitle: gtk::Label,
    list: gtk::Box,
    enabled: Cell<bool>,
    busy: Cell<bool>,
    revision: Cell<u64>,
    rendered: RefCell<Option<Vec<AccessPoint>>>,
    app: glib::WeakRef<gtk::Application>,
}

impl Wifi {
    fn new(card: &gtk::Box, app: &gtk::Application) -> Rc<Self> {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::from_icon_name("network-wireless-offline-symbolic");
        let title = gtk::Label::builder().label("Wi-Fi").xalign(0.0).build();
        title.add_css_class("qs-toggle-title");
        let subtitle = gtk::Label::builder()
            .label("Wi-Fi Unavailable")
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(24)
            .build();
        subtitle.add_css_class("qs-toggle-subtitle");
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 1);
        labels.append(&title);
        labels.append(&subtitle);
        let contents = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        contents.append(&icon);
        contents.append(&labels);
        let button = gtk::Button::builder()
            .hexpand(true)
            .child(&contents)
            .sensitive(false)
            .build();
        button.add_css_class("qs-toggle-pill");
        let chevron = icon_button(
            "pan-down-symbolic",
            "Select Wi-Fi Network",
            "qs-chevron-btn",
        );
        chevron.set_sensitive(false);
        row.append(&button);
        row.append(&chevron);
        card.append(&row);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        list.add_css_class("qs-submenu");
        list.set_visible(false);
        card.append(&list);

        let wifi = Rc::new(Self {
            button,
            chevron,
            icon,
            subtitle,
            list,
            enabled: Cell::new(false),
            busy: Cell::new(false),
            revision: Cell::new(0),
            rendered: RefCell::new(None),
            app: app.downgrade(),
        });
        let weak = Rc::downgrade(&wifi);
        wifi.button.connect_clicked(move |_| {
            if let Some(wifi) = weak.upgrade() {
                let mode = if wifi.enabled.get() { "off" } else { "on" };
                wifi.act(vec![
                    "nmcli".into(),
                    "--wait".into(),
                    "5".into(),
                    "radio".into(),
                    "wifi".into(),
                    mode.into(),
                ]);
            }
        });
        let weak = Rc::downgrade(&wifi);
        wifi.chevron.connect_clicked(move |button| {
            if let Some(wifi) = weak.upgrade() {
                let show = !wifi.list.is_visible();
                wifi.list.set_visible(show);
                active(button, show);
                if show && wifi.enabled.get() {
                    wifi.act(
                        ["nmcli", "--wait", "5", "device", "wifi", "rescan"]
                            .map(str::to_owned)
                            .to_vec(),
                    );
                }
            }
        });
        wifi
    }

    fn act(self: &Rc<Self>, args: Vec<String>) {
        if self.busy.replace(true) {
            return;
        }
        self.revision.set(self.revision.get().wrapping_add(1));
        self.button.set_sensitive(false);
        self.list.set_sensitive(false);
        let weak = Rc::downgrade(self);
        let hold = self.app.upgrade().map(|app| app.hold());
        glib::spawn_future_local(async move {
            let refs: Vec<_> = args.iter().map(String::as_str).collect();
            if let Err(error) = command(&refs).await {
                report(&error);
            }
            if let Some(wifi) = weak.upgrade() {
                wifi.busy.set(false);
                wifi.list.set_sensitive(true);
                wifi.refresh().await;
            }
            drop(hold);
        });
    }

    async fn refresh(self: &Rc<Self>) {
        if self.busy.get() {
            return;
        }
        let revision = self.revision.get();
        let radio = command(&["nmcli", "--wait", "3", "radio", "wifi"])
            .await
            .ok();
        let enabled = radio.as_deref() == Some("enabled");
        let mut points = if enabled {
            command(&[
                "nmcli",
                "--wait",
                "3",
                "-t",
                "-f",
                "IN-USE,SSID,SIGNAL",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
            ])
            .await
            .ok()
            .map(|output| access_points(&output))
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        if revision != self.revision.get() {
            return;
        }

        self.enabled.set(enabled);
        self.button.set_sensitive(radio.is_some());
        self.chevron.set_sensitive(radio.is_some());
        active(&self.button, enabled);
        let connected = points.iter().find(|ap| ap.connected);
        self.subtitle.set_text(match connected {
            Some(ap) => &ap.ssid,
            None if enabled => "Not Connected",
            None if radio.is_some() => "Wi-Fi Off",
            None => "Wi-Fi Unavailable",
        });
        self.icon.set_icon_name(Some(
            connected
                .map(|ap| wifi_icon(ap.strength))
                .unwrap_or("network-wireless-offline-symbolic"),
        ));
        points.truncate(8);
        self.render_list(points);
    }

    fn render_list(self: &Rc<Self>, points: Vec<AccessPoint>) {
        if self.rendered.borrow().as_ref() == Some(&points) {
            return;
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        if points.is_empty() {
            self.list
                .append(&gtk::Label::new(Some("No Wi-Fi networks available")));
        }
        for ap in &points {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.append(&gtk::Image::from_icon_name(wifi_icon(ap.strength)));
            row.append(
                &gtk::Label::builder()
                    .label(&ap.ssid)
                    .hexpand(true)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(24)
                    .build(),
            );
            let strength = gtk::Label::new(Some(&format!("{}%", ap.strength)));
            strength.add_css_class("qs-ap-strength");
            row.append(&strength);
            let button = gtk::Button::builder().child(&row).build();
            button.add_css_class("qs-ap-item");
            if ap.connected {
                button.add_css_class("connected");
            }
            let ssid = ap.ssid.clone();
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(wifi) = weak.upgrade() {
                    let mut args = ["nmcli", "--wait", "10", "device", "wifi", "connect"]
                        .map(str::to_owned)
                        .to_vec();
                    args.push(ssid.clone());
                    wifi.act(args);
                }
            });
            self.list.append(&button);
        }
        self.rendered.replace(Some(points));
    }
}

struct Airplane {
    button: gtk::Button,
    enabled: Cell<bool>,
    busy: Cell<bool>,
    revision: Cell<u64>,
    app: glib::WeakRef<gtk::Application>,
}

impl Airplane {
    fn new(row: &gtk::Box, app: &gtk::Application) -> Rc<Self> {
        let (button, _) = labeled_button("airplane-mode-symbolic", "Airplane Mode", "qs-mini-pill");
        button.set_sensitive(false);
        row.append(&button);
        let airplane = Rc::new(Self {
            button,
            enabled: Cell::new(false),
            busy: Cell::new(false),
            revision: Cell::new(0),
            app: app.downgrade(),
        });
        let weak = Rc::downgrade(&airplane);
        airplane.button.connect_clicked(move |_| {
            let Some(airplane) = weak.upgrade() else {
                return;
            };
            airplane.set_enabled(!airplane.enabled.get());
        });
        airplane
    }

    fn set_enabled(self: &Rc<Self>, airplane_on: bool) {
        if self.busy.replace(true) {
            return;
        }
        self.revision.set(self.revision.get().wrapping_add(1));
        self.button.set_sensitive(false);
        let weak = Rc::downgrade(self);
        let hold = self.app.upgrade().map(|app| app.hold());
        glib::spawn_future_local(async move {
            let mode = if airplane_on { "off" } else { "on" };
            if let Err(error) = command(&["nmcli", "radio", "all", mode]).await {
                report(&error);
            }
            if let Some(airplane) = weak.upgrade() {
                airplane.busy.set(false);
                airplane.refresh().await;
            }
            drop(hold);
        });
    }

    async fn refresh(&self) {
        if self.busy.get() {
            return;
        }
        let revision = self.revision.get();
        let wifi = command(&["nmcli", "radio", "wifi"]).await.ok();
        let wwan = command(&["nmcli", "radio", "wwan"]).await.ok();
        if revision != self.revision.get() {
            return;
        }
        let airplane_on = wifi
            .as_deref()
            .and_then(|wifi| airplane_status(wifi, wwan.as_deref()));
        self.enabled.set(airplane_on.unwrap_or(false));
        self.button.set_sensitive(airplane_on.is_some());
        active(&self.button, airplane_on == Some(true));
    }
}

struct Caffeine {
    button: gtk::Button,
    enabled: Cell<bool>,
    busy: Cell<bool>,
}

impl Caffeine {
    fn new(row: &gtk::Box) -> Rc<Self> {
        let (button, _) =
            labeled_button("media-playback-pause-symbolic", "Caffeine", "qs-mini-pill");
        button.set_sensitive(false);
        row.append(&button);
        let caffeine = Rc::new(Self {
            button,
            enabled: Cell::new(false),
            busy: Cell::new(false),
        });
        let weak = Rc::downgrade(&caffeine);
        caffeine.button.connect_clicked(move |_| {
            if let Some(caffeine) = weak.upgrade() {
                caffeine.toggle();
            }
        });
        caffeine
    }

    fn toggle(self: &Rc<Self>) {
        if self.busy.replace(true) {
            return;
        }
        self.button.set_sensitive(false);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = session_caffeine(true).await;
            if let Some(caffeine) = weak.upgrade() {
                caffeine.busy.set(false);
                caffeine.update(result);
            }
        });
    }

    fn update(&self, enabled: Option<bool>) {
        self.button.set_sensitive(enabled.is_some());
        if let Some(enabled) = enabled {
            self.enabled.set(enabled);
            active(&self.button, enabled);
        }
    }

    async fn refresh(&self) {
        if !self.busy.get() {
            self.update(session_caffeine(false).await);
        }
    }
}

async fn session_caffeine(toggle: bool) -> Option<bool> {
    let bus = gio::bus_get_future(gio::BusType::Session).await.ok()?;
    let reply = bus
        .call_future(
            Some(crate::bar::APP_ID),
            "/io/Astal/Palobar/Session",
            "io.Astal.Palobar.Session",
            if toggle {
                "ToggleCaffeine"
            } else {
                "GetCaffeine"
            },
            None,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
        )
        .await
        .ok()?;
    reply.get::<(bool,)>().map(|(enabled,)| enabled)
}

pub(crate) fn appearance() -> Option<gio::Settings> {
    let schema =
        gio::SettingsSchemaSource::default()?.lookup("org.gnome.desktop.interface", true)?;
    schema
        .has_key("color-scheme")
        .then(|| gio::Settings::new_full(&schema, gio::SettingsBackend::NONE, None))
}

struct Dnd {
    button: gtk::Button,
    icon: gtk::Image,
    busy: Cell<bool>,
    revision: Cell<u64>,
}

impl Dnd {
    fn update(&self, state: Option<bool>) {
        self.button.set_sensitive(state.is_some());
        if let Some(on) = state {
            active(&self.button, on);
            self.icon.set_icon_name(Some(if on {
                "notifications-disabled-symbolic"
            } else {
                "preferences-system-notifications-symbolic"
            }));
        }
    }

    async fn refresh(&self) {
        if self.busy.get() {
            return;
        }
        let revision = self.revision.get();
        let result = crate::notifications::dnd(false).await;
        if revision == self.revision.get() {
            self.update(result);
        }
    }
}

fn quick_toggles(card: &gtk::Box, app: &gtk::Application) -> (Rc<Dnd>, Rc<Airplane>, Rc<Caffeine>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_homogeneous(true);
    let (dark, _) = labeled_button("weather-clear-night-symbolic", "Dark Style", "qs-mini-pill");
    if let Some(settings) = appearance() {
        let update = |button: &gtk::Button, settings: &gio::Settings| {
            active(button, settings.string("color-scheme") == "prefer-dark");
            button.set_sensitive(settings.is_writable("color-scheme"));
        };
        let weak = dark.downgrade();
        let handler = settings.connect_changed(Some("color-scheme"), move |settings, _| {
            if let Some(button) = weak.upgrade() {
                update(&button, settings);
            }
        });
        update(&dark, &settings);
        let clicked_settings = settings.clone();
        dark.connect_clicked(move |_| {
            let mode = if clicked_settings.string("color-scheme") == "prefer-dark" {
                "default"
            } else {
                "prefer-dark"
            };
            if let Err(error) = clicked_settings.set_string("color-scheme", mode) {
                report(&error.to_string());
            }
        });
        let handler = RefCell::new(Some(handler));
        dark.connect_destroy(move |_| {
            if let Some(handler) = handler.take() {
                settings.disconnect(handler);
            }
        });
    } else {
        dark.set_sensitive(false);
    }
    row.append(&dark);

    let (button, icon) = labeled_button(
        "preferences-system-notifications-symbolic",
        "Do Not Disturb",
        "qs-mini-pill",
    );
    button.set_sensitive(false);
    row.append(&button);
    card.append(&row);
    let dnd = Rc::new(Dnd {
        button,
        icon,
        busy: Cell::new(false),
        revision: Cell::new(0),
    });
    let extra_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    extra_row.set_homogeneous(true);
    let airplane = Airplane::new(&extra_row, app);
    let caffeine = Caffeine::new(&extra_row);
    card.append(&extra_row);
    let weak = Rc::downgrade(&dnd);
    let app = app.downgrade();
    dnd.button.connect_clicked(move |button| {
        let Some(dnd) = weak.upgrade() else { return };
        if dnd.busy.replace(true) {
            return;
        }
        dnd.revision.set(dnd.revision.get().wrapping_add(1));
        button.set_sensitive(false);
        let hold = app.upgrade().map(|app| app.hold());
        glib::spawn_future_local(async move {
            let result = crate::notifications::dnd(true).await;
            dnd.busy.set(false);
            dnd.update(result);
            drop(hold);
        });
    });
    (dnd, airplane, caffeine)
}

pub fn install_style(display: &gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("quick_settings.css"));
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
    crate::calendar::close(app);
    crate::clipboard::close(app);
    crate::workspace_dialog::close(app);
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .hexpand(true)
        .margin_top(6)
        .margin_end(10)
        .build();
    card.add_css_class("qs-card");
    let window = crate::panel_popup::show_window(
        app,
        monitor,
        TITLE,
        "quick-settings-popup",
        CLASS,
        &card,
        indicator,
    );
    let battery = Battery::new();
    header(&card, &window, &battery);
    battery.refresh();
    let audio = Audio::new(&card, app);
    let brightness = Brightness::new(&card, app);
    let wifi = Wifi::new(&card, app);
    let (dnd, airplane, caffeine) = quick_toggles(&card, app);
    let battery_refresh = glib::spawn_future_local(async move {
        loop {
            battery.refresh();
            glib::timeout_future(Duration::from_secs(30)).await;
        }
    });
    let audio_refresh = glib::spawn_future_local(async move {
        loop {
            audio.refresh().await;
            glib::timeout_future(Duration::from_secs(1)).await;
        }
    });
    let wifi_refresh = glib::spawn_future_local(async move {
        loop {
            wifi.refresh().await;
            glib::timeout_future(Duration::from_secs(2)).await;
        }
    });
    let brightness_refresh = glib::spawn_future_local(async move {
        loop {
            brightness.refresh().await;
            glib::timeout_future(Duration::from_secs(3)).await;
        }
    });
    let airplane_refresh = glib::spawn_future_local(async move {
        loop {
            airplane.refresh().await;
            glib::timeout_future(Duration::from_secs(5)).await;
        }
    });
    let dnd_refresh = glib::spawn_future_local(async move {
        loop {
            dnd.refresh().await;
            glib::timeout_future(Duration::from_secs(2)).await;
        }
    });
    let caffeine_refresh = glib::spawn_future_local(async move {
        loop {
            caffeine.refresh().await;
            glib::timeout_future(Duration::from_secs(2)).await;
        }
    });
    window.connect_unrealize(move |_| {
        battery_refresh.abort();
        audio_refresh.abort();
        brightness_refresh.abort();
        wifi_refresh.abort();
        airplane_refresh.abort();
        dnd_refresh.abort();
        caffeine_refresh.abort();
    });
    window.present();
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

pub fn toggle_first_monitor(app: &gtk::Application) {
    if let Some(monitor) = gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gdk::Monitor>()
    {
        toggle(app, &monitor, None);
    }
}

#[cfg(test)]
mod tests {
    use super::{access_points, airplane_status, parse_brightness, volume_icon};

    #[test]
    fn parses_escaped_wifi_names_and_keeps_the_strongest_access_point() {
        let points = access_points(
            r"*:\\lab\:guest:35
:\\lab\:guest:81
:other:42
::92",
        );
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].ssid, r"\lab:guest");
        assert_eq!(points[0].strength, 81);
        assert!(points[0].connected);
        assert!(!points[1].connected);
    }

    #[test]
    fn mute_takes_precedence_over_the_volume_level() {
        assert_eq!(
            volume_icon("Volume: 0.75 [MUTED]"),
            "audio-volume-muted-symbolic"
        );
        assert_eq!(volume_icon("Volume: 0.75"), "audio-volume-high-symbolic");
    }

    #[test]
    fn parses_backlight_brightness_and_ignores_other_devices() {
        assert_eq!(
            parse_brightness("amdgpu_bl1,backlight,153,255,60%"),
            Some(0.6)
        );
        assert_eq!(
            parse_brightness("amdgpu_bl1,backlight,65535,100%,65535"),
            Some(1.0)
        );
        assert_eq!(parse_brightness("input3,leds,1,1,100%"), None);
        assert_eq!(parse_brightness("amdgpu_bl1,backlight,0,0,0%"), None);
    }

    #[test]
    fn airplane_mode_requires_all_available_radios_to_be_off() {
        assert_eq!(airplane_status("disabled", Some("disabled")), Some(true));
        assert_eq!(airplane_status("disabled", None), Some(true));
        assert_eq!(airplane_status("disabled", Some("enabled")), Some(false));
        assert_eq!(airplane_status("enabled", None), Some(false));
        assert_eq!(airplane_status("unavailable", None), None);
    }
}
