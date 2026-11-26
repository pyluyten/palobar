use crate::config::read_config;
use gtk::prelude::*;
use gtk::{gio, glib};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

type Command = Vec<OsString>;

#[derive(Default)]
struct Child {
    command: Command,
    process: Option<gio::Subprocess>,
}

impl Child {
    fn stop(&mut self) {
        if let Some(process) = self.process.take() {
            process.force_exit();
            let _ = process.wait(gio::Cancellable::NONE);
        }
    }

    fn update(slot: &Rc<RefCell<Self>>, command: Command) {
        let mut child = slot.borrow_mut();
        if child.command == command && (command.is_empty() || child.process.is_some()) {
            return;
        }
        child.stop();
        child.command = command;
        if child.command.is_empty() {
            return;
        }
        let argv: Vec<_> = child.command.iter().map(OsString::as_os_str).collect();
        match gio::Subprocess::newv(&argv, gio::SubprocessFlags::NONE) {
            Ok(process) => {
                child.process = Some(process.clone());
                let slot = Rc::downgrade(slot);
                let watched = process.clone();
                process.wait_async(gio::Cancellable::NONE, move |_| {
                    if let Some(slot) = slot.upgrade() {
                        let mut child = slot.borrow_mut();
                        if child.process.as_ref() == Some(&watched) {
                            child.process = None;
                        }
                    }
                });
            }
            Err(error) => eprintln!(
                "[palobar session] Could not start {:?}: {error}",
                child.command[0]
            ),
        }
    }
}

fn wallpaper(config: &Value) -> Command {
    let color = config["color"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or("#131316");
    let mut command: Command = ["swaybg", "-c", color]
        .into_iter()
        .map(Into::into)
        .collect();
    if let Some(path) = config["path"]
        .as_str()
        .filter(|path| Path::new(path).is_file())
    {
        let mode = crate::background::MODES
            [crate::background::mode_index(config["mode"].as_str()) as usize];
        command.extend(["-m", mode, "-i", path].into_iter().map(OsString::from));
    }
    command
}

fn seconds(value: &Value, default: f64, multiplier: f64) -> Option<u64> {
    let value = (value.as_f64().unwrap_or(default) * multiplier).round();
    (value.is_finite() && value > 0.0 && value < u64::MAX as f64).then_some(value as u64)
}

fn idle(power: &Value, lock: &Value, executable: &OsStr) -> Command {
    let mut command = Vec::new();
    let lock_command = format!(
        "{} request-lock",
        glib::shell_quote(executable).to_string_lossy()
    );
    let lock_delay = lock["autoLockDelaySeconds"]
        .as_f64()
        .filter(|delay| *delay >= 0.0 && *delay < u64::MAX as f64)
        .map(|delay| delay.round() as u64);
    let blank_time = power["screenBlankEnabled"]
        .as_bool()
        .unwrap_or(true)
        .then(|| seconds(&power["screenBlankMinutes"], 10.0, 60.0))
        .flatten();
    if let Some(blank_time) = blank_time {
        let mut blank = "niri msg action power-off-monitors".to_owned();
        if lock_delay == Some(0) {
            blank.push_str(" && ");
            blank.push_str(&lock_command);
        }
        command.extend([
            "timeout".into(),
            blank_time.to_string().into(),
            blank.into(),
            "resume".into(),
            "niri msg action power-on-monitors".into(),
        ]);
        if let Some(at) = lock_delay
            .filter(|delay| *delay > 0)
            .and_then(|delay| blank_time.checked_add(delay))
        {
            command.extend(["timeout".into(), at.to_string().into(), lock_command.into()]);
        }
    }
    if power["autoSuspendEnabled"].as_bool().unwrap_or(false) {
        if let Some(suspend_time) = seconds(&power["autoSuspendMinutes"], 120.0, 60.0) {
            command.extend([
                "timeout".into(),
                suspend_time.to_string().into(),
                "systemctl suspend".into(),
            ]);
        }
    }
    if command.is_empty() {
        command
    } else {
        ["swayidle".into(), "-w".into()]
            .into_iter()
            .chain(command)
            .collect()
    }
}

fn idle_with_caffeine(power: &Value, lock: &Value, executable: &OsStr, caffeine: bool) -> Command {
    if caffeine {
        Vec::new()
    } else {
        idle(power, lock, executable)
    }
}

struct Services {
    config: gio::File,
    executable: PathBuf,
    wallpaper: Rc<RefCell<Child>>,
    idle: Rc<RefCell<Child>>,
    caffeine: Cell<bool>,
    pending: RefCell<Option<glib::SourceId>>,
}

impl Services {
    fn refresh(&self) {
        match read_config(&self.config) {
            Ok((config, _)) => {
                Child::update(&self.wallpaper, wallpaper(&config["wallpaper"]));
                Child::update(
                    &self.idle,
                    idle_with_caffeine(
                        &config["power"],
                        &config["lockscreen"],
                        self.executable.as_os_str(),
                        self.caffeine.get(),
                    ),
                );
            }
            Err(error) => eprintln!("[palobar session] Could not read the configuration: {error}"),
        }
    }
}

pub(crate) fn start(app: &gtk::Application) {
    if let Err(error) = start_services(app) {
        eprintln!("[palobar session] Could not start desktop services: {error}");
    }
}

fn start_services(app: &gtk::Application) -> Result<(), Box<dyn std::error::Error>> {
    let directory = glib::user_config_dir().join("palobar");
    std::fs::create_dir_all(&directory)?;
    let monitor = gio::File::for_path(&directory)
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)?;
    let services = Rc::new(Services {
        config: gio::File::for_path(directory.join("config.json")),
        executable: std::env::current_exe()?,
        wallpaper: Rc::default(),
        idle: Rc::default(),
        caffeine: Cell::new(false),
        pending: RefCell::default(),
    });
    monitor.connect_changed({
        let services = Rc::downgrade(&services);
        move |_, file, other, _| {
            let Some(services) = services.upgrade() else {
                return;
            };
            if !file.equal(&services.config)
                && !other.is_some_and(|file| file.equal(&services.config))
            {
                return;
            }
            if let Some(source) = services.pending.take() {
                source.remove();
            }
            let weak = Rc::downgrade(&services);
            services.pending.replace(Some(glib::timeout_add_local_once(
                Duration::from_millis(120),
                move || {
                    if let Some(services) = weak.upgrade() {
                        services.pending.take();
                        services.refresh();
                    }
                },
            )));
        }
    });
    services.refresh();
    let registration = app.dbus_connection().and_then(|connection| {
        let result = gio::DBusNodeInfo::for_xml(
            r#"<node><interface name="io.Astal.Palobar.Session">
                <method name="GetCaffeine"><arg type="b" direction="out"/></method>
                <method name="ToggleCaffeine"><arg type="b" direction="out"/></method>
            </interface></node>"#,
        )
        .and_then(|node| {
            let interface = node
                .lookup_interface("io.Astal.Palobar.Session")
                .expect("session interface");
            let services = Rc::downgrade(&services);
            connection
                .register_object("/io/Astal/Palobar/Session", &interface)
                .method_call(move |_, _, _, _, method, _, call| {
                    let Some(services) = services.upgrade() else {
                        return;
                    };
                    let enabled = if method == "ToggleCaffeine" {
                        let enabled = !services.caffeine.get();
                        services.caffeine.set(enabled);
                        services.refresh();
                        enabled
                    } else {
                        services.caffeine.get()
                    };
                    call.return_value(Some(&(enabled,).to_variant()));
                })
                .build()
                .map(|id| (connection, id))
        });
        match result {
            Ok(registration) => Some(registration),
            Err(error) => {
                eprintln!("[palobar session] Could not register the Caffeine control: {error}");
                None
            }
        }
    });
    let registration = RefCell::new(registration);
    app.connect_shutdown(move |_| {
        monitor.cancel();
        if let Some(source) = services.pending.take() {
            source.remove();
        }
        services.idle.borrow_mut().stop();
        services.wallpaper.borrow_mut().stop();
        if let Some((connection, id)) = registration.take() {
            let _ = connection.unregister_object(id);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_screen_blank_lock_and_suspend() {
        let power = serde_json::json!({"screenBlankMinutes": 1, "autoSuspendEnabled": true, "autoSuspendMinutes": 15});
        let execute = |delay| {
            idle(
                &power,
                &serde_json::json!({"autoLockDelaySeconds": delay}),
                OsStr::new("/path with spaces/palobar"),
            )
            .into_iter()
            .map(|argument| argument.into_string().unwrap())
            .collect::<Vec<_>>()
        };
        assert_eq!(
            &execute(30)[7..],
            [
                "timeout",
                "90",
                "'/path with spaces/palobar' request-lock",
                "timeout",
                "900",
                "systemctl suspend"
            ]
        );
        assert_eq!(
            execute(0)[4],
            "niri msg action power-off-monitors && '/path with spaces/palobar' request-lock"
        );
        let disabled = serde_json::json!({"screenBlankEnabled": false});
        assert!(idle(&disabled, &Value::Null, OsStr::new("palobar")).is_empty());
    }

    #[test]
    fn caffeine_disables_the_session_idle_service_until_toggled_off() {
        let power = serde_json::json!({"screenBlankMinutes": 1, "autoSuspendEnabled": true, "autoSuspendMinutes": 15});
        let lock = serde_json::json!({"autoLockDelaySeconds": 30});
        let executable = OsStr::new("palobar");
        assert!(!idle_with_caffeine(&power, &lock, executable, false).is_empty());
        assert!(idle_with_caffeine(&power, &lock, executable, true).is_empty());
    }
}
