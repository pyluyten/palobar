use gtk::prelude::*;
use gtk::{gio, glib};
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

const CONFIRM_SECONDS: i64 = 15;
pub const PREVIEW_COMMAND: &str = "output-preview";

#[derive(Deserialize)]
struct Mode {
    width: u32,
    height: u32,
    refresh_rate: u32,
}

impl Mode {
    fn argument(&self) -> String {
        format!(
            "{}x{}@{:.3}",
            self.width,
            self.height,
            f64::from(self.refresh_rate) / 1000.0
        )
    }

    fn label(&self) -> String {
        format!(
            "{} × {} · {:.3} Hz",
            self.width,
            self.height,
            f64::from(self.refresh_rate) / 1000.0
        )
    }
}

#[derive(Deserialize)]
struct Logical {
    width: u32,
    height: u32,
    scale: f64,
}

#[derive(Deserialize)]
struct Output {
    name: String,
    model: Option<String>,
    modes: Vec<Mode>,
    current_mode: Option<usize>,
    #[serde(default)]
    is_custom_mode: bool,
    logical: Option<Logical>,
}

impl Output {
    fn mode(&self) -> Option<&Mode> {
        self.modes.get(self.current_mode?)
    }
}

#[derive(Clone, Copy)]
enum Setting {
    Mode,
    Scale,
}

impl Setting {
    fn argument(self) -> &'static str {
        match self {
            Self::Mode => "mode",
            Self::Scale => "scale",
        }
    }
}

async fn outputs() -> Result<BTreeMap<String, Output>, String> {
    let result = crate::quick_settings::command(&["niri", "msg", "-j", "outputs"]).await?;
    serde_json::from_str(&result).map_err(|error| error.to_string())
}

fn percent(scale: f64) -> String {
    let formatted = format!("{:.2}", scale * 100.0);
    format!("{}%", formatted.trim_end_matches('0').trim_end_matches('.'))
}

fn drawing(output: &Output) -> gtk::DrawingArea {
    let ratio = output
        .logical
        .as_ref()
        .map(|logical| (logical.width, logical.height))
        .or_else(|| output.mode().map(|mode| (mode.width, mode.height)))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map_or(16.0 / 9.0, |(width, height)| {
            f64::from(width) / f64::from(height)
        });
    let area = gtk::DrawingArea::builder()
        .content_width(112)
        .content_height(76)
        .build();
    area.set_draw_func(move |_, cr, width, height| {
        let width = f64::from(width);
        let height = f64::from(height);
        if width <= 8.0 || height <= 20.0 {
            return;
        }
        let screen = (width - 8.0).min((height - 20.0) * ratio);
        let screen_height = screen / ratio;
        let x = (width - screen) / 2.0;
        let y = (height - screen_height - 14.0) / 2.0;
        cr.set_source_rgba(0.31, 0.56, 0.86, 0.20);
        cr.rectangle(x, y, screen, screen_height);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.31, 0.56, 0.86, 0.95);
        cr.set_line_width(2.0);
        let _ = cr.stroke();
        cr.move_to(width / 2.0, y + screen_height);
        cr.line_to(width / 2.0, y + screen_height + 10.0);
        cr.move_to(width / 2.0 - 13.0, y + screen_height + 10.0);
        cr.line_to(width / 2.0 + 13.0, y + screen_height + 10.0);
        let _ = cr.stroke();
    });
    area
}

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build();
    label.add_css_class(class);
    label
}

struct Page {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    cards: gtk::Box,
    busy: Cell<bool>,
    confirmation: RefCell<Option<gio::OutputStream>>,
}

impl Page {
    fn rebuild(self: &Rc<Self>, result: Result<BTreeMap<String, Output>, String>) {
        while let Some(child) = self.cards.first_child() {
            self.cards.remove(&child);
        }
        match result {
            Ok(outputs) if !outputs.is_empty() => {
                for output in outputs.values() {
                    self.cards.append(&self.card(output));
                }
            }
            Ok(_) => self
                .cards
                .append(&label("No connected displays", "prefs-muted")),
            Err(error) => {
                eprintln!("[outputs] {error}");
                self.cards
                    .append(&label("Could not list Niri displays", "prefs-muted"));
            }
        }
    }

    fn card(self: &Rc<Self>, output: &Output) -> gtk::Box {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("prefs-group");
        let active = output.mode().zip(output.logical.as_ref());
        let summary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        summary.add_css_class("prefs-row");
        if active.is_none() {
            summary.add_css_class("last");
        }
        summary.append(&drawing(output));
        let details = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let name = output
            .model
            .as_deref()
            .filter(|model| !model.is_empty())
            .map_or_else(
                || output.name.clone(),
                |model| format!("{model} ({})", output.name),
            );
        details.append(&label(&name, "prefs-row-title"));
        let status = active.map_or_else(
            || "Display off".into(),
            |(mode, logical)| {
                format!(
                    "{} × {} · {}",
                    mode.width,
                    mode.height,
                    percent(logical.scale)
                )
            },
        );
        details.append(&label(&status, "prefs-muted"));
        summary.append(&details);
        card.append(&summary);
        if let Some((mode, logical)) = active {
            if !output.is_custom_mode {
                let choices = output
                    .modes
                    .iter()
                    .map(|mode| (mode.label(), mode.argument()))
                    .collect();
                self.setting(
                    &card,
                    &output.name,
                    "Resolution",
                    Setting::Mode,
                    choices,
                    &mode.argument(),
                );
            }
            let mut scales = vec![1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, logical.scale];
            scales.sort_by(f64::total_cmp);
            scales.dedup();
            let choices = scales
                .into_iter()
                .map(|scale| (percent(scale), scale.to_string()))
                .collect();
            self.setting(
                &card,
                &output.name,
                "Scale",
                Setting::Scale,
                choices,
                &logical.scale.to_string(),
            );
        }
        card
    }

    fn setting(
        self: &Rc<Self>,
        parent: &gtk::Box,
        output: &str,
        title: &str,
        setting: Setting,
        choices: Vec<(String, String)>,
        current: &str,
    ) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("prefs-row");
        if matches!(setting, Setting::Scale) {
            row.add_css_class("last");
        }
        let title = label(title, "prefs-row-title");
        title.set_hexpand(true);
        row.append(&title);
        let labels: Vec<_> = choices.iter().map(|(label, _)| label.as_str()).collect();
        let dropdown = gtk::DropDown::from_strings(&labels);
        let original = choices
            .iter()
            .position(|(_, value)| value == current)
            .map_or(gtk::INVALID_LIST_POSITION, |index| index as u32);
        dropdown.set_selected(original);
        dropdown.set_size_request(145, -1);
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("prefs-btn");
        apply.set_sensitive(false);
        let weak = apply.downgrade();
        dropdown.connect_selected_notify(move |dropdown| {
            if let Some(apply) = weak.upgrade() {
                let selected = dropdown.selected();
                apply.set_sensitive(selected != original && selected != gtk::INVALID_LIST_POSITION);
            }
        });
        let weak_selection = dropdown.downgrade();
        let weak_page = Rc::downgrade(self);
        let output = output.to_owned();
        apply.connect_clicked(move |_| {
            if let (Some(page), Some(dropdown)) = (weak_page.upgrade(), weak_selection.upgrade()) {
                if let Some((_, value)) = choices.get(dropdown.selected() as usize) {
                    page.apply(output.clone(), setting, value.clone());
                }
            }
        });
        row.append(&dropdown);
        row.append(&apply);
        parent.append(&row);
    }

    fn apply(self: &Rc<Self>, output: String, setting: Setting, value: String) {
        if self.busy.replace(true) {
            return;
        }
        self.cards.set_sensitive(false);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = preview(&weak, &output, setting, &value).await;
            let snapshot = outputs().await;
            let Some(page) = weak.upgrade() else { return };
            page.busy.set(false);
            page.cards.set_sensitive(true);
            page.rebuild(snapshot);
            if let (Err(error), Some(window)) = (result, page.window.upgrade()) {
                let dialog = gtk::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .destroy_with_parent(true)
                    .buttons(gtk::ButtonsType::Close)
                    .text("Could not change display settings")
                    .secondary_text(error)
                    .build();
                dialog.connect_response(|dialog, _| dialog.destroy());
                dialog.present();
            }
        });
    }
}

fn confirm(input: &RefCell<Option<gio::OutputStream>>, keep: bool) {
    if let Some(input) = input.take() {
        if keep {
            let _ = input.write_bytes(&glib::Bytes::from_static(b"keep\n"), gio::Cancellable::NONE);
        }
        let _ = input.close(gio::Cancellable::NONE);
    }
}

async fn preview(
    page: &std::rc::Weak<Page>,
    output: &str,
    setting: Setting,
    value: &str,
) -> Result<(), String> {
    let snapshot = outputs().await?;
    let output_state = snapshot
        .get(output)
        .ok_or("The display is no longer active.")?;
    let (mode, logical) = output_state
        .mode()
        .zip(output_state.logical.as_ref())
        .ok_or("The display is no longer active.")?;
    let previous = match setting {
        Setting::Mode => {
            if output_state.is_custom_mode
                || !output_state
                    .modes
                    .iter()
                    .any(|mode| mode.argument() == value)
            {
                return Err("This display mode is no longer available for testing.".into());
            }
            mode.argument()
        }
        Setting::Scale => logical.scale.to_string(),
    };
    if value == previous {
        return Ok(());
    }
    let Some(page_ref) = page.upgrade() else {
        return Ok(());
    };
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let argv = [
        executable.as_os_str(),
        OsStr::new(PREVIEW_COMMAND),
        OsStr::new(output),
        OsStr::new(setting.argument()),
        OsStr::new(value),
        OsStr::new(&previous),
    ];
    let process = gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDIN_PIPE | gio::SubprocessFlags::STDOUT_PIPE,
    )
    .map_err(|error| error.to_string())?;
    page_ref.confirmation.replace(process.stdin_pipe());
    drop(page_ref);
    let stdout = gio::DataInputStream::new(&process.stdout_pipe().expect("stdout pipe requested"));
    let ready = stdout.read_line_future(glib::Priority::DEFAULT).await;
    let deadline = ready.ok().flatten().and_then(|line| {
        std::str::from_utf8(&line)
            .ok()?
            .strip_prefix("ready ")?
            .parse::<i64>()
            .ok()
    });
    let deadline = deadline.filter(|deadline| *deadline > glib::monotonic_time());
    let mut dialog = None;
    let mut timer = None;
    if let Some(page_ref) = page.upgrade() {
        if let (Some(deadline), Some(window)) = (deadline, page_ref.window.upgrade()) {
            let confirmation = gtk::MessageDialog::builder()
                .transient_for(&window)
                .modal(true)
                .destroy_with_parent(true)
                .text("Keep these display settings?")
                .secondary_text(countdown(deadline))
                .build();
            confirmation.add_button("Revert", gtk::ResponseType::Cancel);
            confirmation.add_button("Keep Changes", gtk::ResponseType::Accept);
            confirmation.set_default_response(gtk::ResponseType::Cancel);
            let owner = Rc::downgrade(&page_ref);
            confirmation.connect_response(move |dialog, response| {
                if let Some(owner) = owner.upgrade() {
                    confirm(&owner.confirmation, response == gtk::ResponseType::Accept);
                }
                dialog.hide();
            });
            let owner = Rc::downgrade(&page_ref);
            confirmation.connect_close_request(move |_| {
                if let Some(owner) = owner.upgrade() {
                    confirm(&owner.confirmation, false);
                }
                glib::Propagation::Proceed
            });
            let weak = confirmation.downgrade();
            timer = Some(glib::timeout_add_local(
                Duration::from_millis(200),
                move || {
                    if let Some(dialog) = weak.upgrade() {
                        dialog.set_secondary_text(Some(&countdown(deadline)));
                    }
                    glib::ControlFlow::Continue
                },
            ));
            confirmation.present();
            dialog = Some(confirmation);
        } else {
            confirm(&page_ref.confirmation, false);
        }
    }
    let waited = process.wait_future().await;
    if let Some(timer) = timer {
        timer.remove();
    }
    if let Some(dialog) = dialog {
        dialog.destroy();
    }
    if let Some(page_ref) = page.upgrade() {
        confirm(&page_ref.confirmation, false);
    }
    waited.map_err(|error| error.to_string())?;
    match process.has_exited().then(|| process.exit_status()) {
        Some(0 | 2) => Ok(()),
        Some(4) => Err("Niri could not restore the previous display setting.".into()),
        _ => Err("Niri rejected the requested display setting.".into()),
    }
}

fn countdown(deadline: i64) -> String {
    let seconds = (deadline - glib::monotonic_time()).max(0);
    format!(
        "Previous settings will be restored in {} seconds.",
        (seconds + 999_999) / 1_000_000
    )
}

// This worker owns the deadline independently of the preferences window.
pub fn run_preview(args: &[OsString]) -> glib::ExitCode {
    let [output, setting, value, previous] = args else {
        return glib::ExitCode::FAILURE;
    };
    if !matches!(setting.to_str(), Some("mode" | "scale")) {
        return glib::ExitCode::FAILURE;
    }
    unsafe { libc::setpgid(0, 0) };
    // Reserve stdout for the readiness protocol.
    let change = |value: &OsStr| {
        Command::new("niri")
            .args([
                OsStr::new("msg"),
                OsStr::new("output"),
                output,
                setting,
                value,
            ])
            .stdout(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    };
    if !change(value) {
        return if change(previous) {
            glib::ExitCode::new(3)
        } else {
            glib::ExitCode::new(4)
        };
    }
    let deadline = glib::monotonic_time() + CONFIRM_SECONDS * 1_000_000;
    let ready = writeln!(io::stdout(), "ready {deadline}").and_then(|()| io::stdout().flush());
    if ready.is_ok() && keep_before(deadline) {
        return glib::ExitCode::SUCCESS;
    }
    if change(previous) {
        glib::ExitCode::new(2)
    } else {
        glib::ExitCode::new(4)
    }
}

fn keep_before(deadline: i64) -> bool {
    for byte in b"keep\n" {
        let remaining = deadline - glib::monotonic_time();
        if remaining <= 0 {
            return false;
        }
        let timeout = ((remaining + 999) / 1000) as i32;
        let mut fd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut fd, 1, timeout) } <= 0 || glib::monotonic_time() >= deadline {
            return false;
        }
        let mut received = [0_u8];
        if unsafe { libc::read(libc::STDIN_FILENO, received.as_mut_ptr().cast(), 1) } != 1
            || received[0] != *byte
        {
            return false;
        }
    }
    true
}

fn build(app: &gtk::Application) {
    if let Some(window) = app.windows().first() {
        window.present();
        return;
    }
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Palobar Settings")
        .default_width(740)
        .default_height(540)
        .icon_name("io.Astal.palobar-prefs")
        .build();
    window.add_css_class("prefs-window");
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&label("Output", "prefs-page-header"));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("prefs-page-body");
    content.append(&label("Temporary settings", "prefs-group-title"));
    let cards = gtk::Box::new(gtk::Orientation::Vertical, 12);
    cards.append(&label("Loading displays…", "prefs-muted"));
    content.append(&cards);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();
    root.append(&scroll);
    window.set_child(Some(&root));
    let page = Rc::new(Page {
        window: window.downgrade(),
        cards,
        busy: Cell::default(),
        confirmation: RefCell::default(),
    });
    let weak = Rc::downgrade(&page);
    window.connect_unrealize(move |_| confirm(&page.confirmation, false));
    glib::spawn_future_local(async move {
        let snapshot = outputs().await;
        if let Some(page) = weak.upgrade() {
            page.rebuild(snapshot);
        }
    });
    window.present();
}

fn follow_color_scheme(app: &gtk::Application) {
    let (Some(appearance), Some(theme)) = (
        crate::quick_settings::appearance(),
        gtk::Settings::default(),
    ) else {
        return;
    };
    let update = move |settings: &gio::Settings| {
        theme.set_gtk_application_prefer_dark_theme(
            settings.string("color-scheme") == "prefer-dark",
        );
    };
    let handler = appearance.connect_changed(Some("color-scheme"), {
        let update = update.clone();
        move |settings, _| update(settings)
    });
    update(&appearance);
    let handler = RefCell::new(Some(handler));
    app.connect_shutdown(move |_| {
        if let Some(handler) = handler.take() {
            appearance.disconnect(handler);
        }
    });
}

pub fn run() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.Astal.Palobar.RustPreferences")
        .build();
    app.connect_startup(|app| {
        follow_color_scheme(app);
        if let Some(display) = gtk::gdk::Display::default() {
            let provider = gtk::CssProvider::new();
            provider.load_from_data(include_str!("preferences.css"));
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
    app.connect_activate(build);
    app.run_with_args::<&str>(&[])
}

pub fn launch() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    gio::Subprocess::newv(
        &[executable.as_os_str(), OsStr::new("prefs")],
        gio::SubprocessFlags::NONE,
    )
    .map(drop)
    .map_err(|error| error.to_string())
}
