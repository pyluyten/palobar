#[path = "lockscreen/native.rs"]
mod native;
#[path = "lockscreen/pam.rs"]
mod pam;

use chrono::Local;
use glib::translate::ToGlibPtr;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use native::NativeLock;
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, OsStr};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const FALLBACK_COLOR: &str = "#131316";
const CSS: &str = include_str!("lockscreen.css");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Starting,
    Locked,
    Unlocked,
    Failed,
}

struct View {
    id: u64,
    stack: gtk::Stack,
    background: gtk::Widget,
    time: gtk::Label,
    date: gtk::Label,
    entry: gtk::PasswordEntry,
    status: gtk::Label,
}

struct Session {
    native: NativeLock,
    main_loop: glib::MainLoop,
    outcome: Cell<Outcome>,
    views: RefCell<Vec<View>>,
    next_id: Cell<u64>,
    prompt: Cell<bool>,
    busy: Cell<bool>,
    error: Cell<Option<&'static str>>,
    user: CString,
    display_name: String,
    background: Option<gdk::Texture>,
    avatar: Option<gdk::Texture>,
}

fn texture(path: impl AsRef<std::path::Path>) -> Option<gdk::Texture> {
    let path = path.as_ref();
    if !path.is_file() {
        return None;
    }
    gdk::Texture::from_file(&gio::File::for_path(path)).ok()
}

fn load_background() -> (Option<gdk::Texture>, gdk::RGBA) {
    let path = glib::user_config_dir().join("palobar/config.json");
    let config = std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
        .unwrap_or_default();
    let background = ["/lockscreen/image", "/wallpaper/path"]
        .into_iter()
        .filter_map(|key| config.pointer(key).and_then(serde_json::Value::as_str))
        .filter(|path| !path.is_empty())
        .find_map(texture);
    let color = config
        .pointer("/lockscreen/color")
        .and_then(serde_json::Value::as_str)
        .and_then(|color| gdk::RGBA::parse(color).ok())
        .unwrap_or_else(|| gdk::RGBA::parse(FALLBACK_COLOR).expect("valid fallback color"));
    (background, color)
}

fn styled_label(text: Option<&str>, class: &str) -> gtk::Label {
    let label = gtk::Label::new(text);
    label.add_css_class(class);
    label
}

fn page(spacing: i32) -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(spacing)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build()
}

fn picture(texture: &gdk::Texture) -> gtk::Picture {
    let picture = gtk::Picture::for_paintable(texture);
    if let Some(cover) = picture
        .find_property("content-fit")
        .and_then(|property| glib::EnumClass::with_type(property.value_type()))
        .and_then(|fit| fit.to_value_by_nick("cover"))
    {
        picture.set_property_from_value("content-fit", &cover);
    }
    picture
}

fn update_clock(views: &[View]) {
    let now = Local::now();
    let time = now.format("%H:%M").to_string();
    let date = now.format("%A %-d %B").to_string();
    for view in views {
        if view.time.text() != time {
            view.time.set_text(&time);
        }
        if view.date.text() != date {
            view.date.set_text(&date);
        }
    }
}

impl Session {
    fn add_monitor(self: &Rc<Self>, monitor: &gdk::Monitor) {
        let window = gtk::Window::builder()
            .title("Palobar Lock Screen")
            .decorated(false)
            .build();
        window.add_css_class("lock-window");
        window.connect_close_request(|_| glib::Propagation::Stop);

        let overlay = gtk::Overlay::new();
        let background = self
            .background
            .as_ref()
            .map(|texture| picture(texture).upcast::<gtk::Widget>())
            .unwrap_or_else(|| gtk::Box::new(gtk::Orientation::Vertical, 0).upcast());
        background.add_css_class("lock-bg");
        overlay.set_child(Some(&background));

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(350)
            .build();
        overlay.add_overlay(&stack);
        window.set_child(Some(&overlay));

        let clock = page(0);
        let time = styled_label(None, "lock-time");
        let date = styled_label(None, "lock-date");
        clock.append(&time);
        clock.append(&date);
        stack.add_named(&clock, Some("clock"));

        let prompt = page(14);
        let avatar = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .overflow(gtk::Overflow::Hidden)
            .build();
        avatar.add_css_class("lock-avatar");
        let avatar_content = self
            .avatar
            .as_ref()
            .map(|texture| picture(texture).upcast::<gtk::Widget>())
            .unwrap_or_else(|| {
                gtk::Image::builder()
                    .icon_name("avatar-default-symbolic")
                    .pixel_size(56)
                    .build()
                    .upcast()
            });
        avatar_content.set_size_request(96, 96);
        avatar.append(&avatar_content);
        prompt.append(&avatar);
        prompt.append(&styled_label(Some(&self.display_name), "lock-username"));

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text("Password")
            .show_peek_icon(true)
            .width_request(260)
            .build();
        entry.add_css_class("lock-entry");
        let weak = Rc::downgrade(self);
        entry.connect_activate(move |entry| {
            if let Some(session) = weak.upgrade() {
                session.authenticate(entry);
            }
        });
        prompt.append(&entry);
        let status = styled_label(None, "lock-error");
        prompt.append(&status);
        stack.add_named(&prompt, Some("prompt"));

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let weak_entry = entry.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if let Some(session) = weak.upgrade() {
                if !session.prompt.get() {
                    session.show_prompt(true);
                    if let Some(entry) = weak_entry.upgrade() {
                        entry.grab_focus();
                    }
                    return glib::Propagation::Stop;
                }
                if key == gdk::Key::Escape {
                    session.show_prompt(false);
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        window.add_controller(keys);

        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        let weak_entry = entry.downgrade();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(session) = weak.upgrade() {
                if !session.prompt.get() {
                    session.show_prompt(true);
                    if let Some(entry) = weak_entry.upgrade() {
                        entry.grab_focus();
                    }
                }
            }
        });
        window.add_controller(click);

        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let weak = Rc::downgrade(self);
        window.connect_unrealize(move |_| {
            if let Some(session) = weak.upgrade() {
                session.views.borrow_mut().retain(|view| view.id != id);
            }
        });
        let view = View {
            id,
            stack,
            background,
            time,
            date,
            entry,
            status,
        };
        self.update_view(&view);
        update_clock(std::slice::from_ref(&view));
        self.views.borrow_mut().push(view);
        self.native.assign(&window, monitor);
    }

    fn update_view(&self, view: &View) {
        let prompt = self.prompt.get();
        view.stack
            .set_visible_child_name(if prompt { "prompt" } else { "clock" });
        if prompt {
            view.background.add_css_class("prompt");
        } else {
            view.background.remove_css_class("prompt");
        }
        view.entry
            .set_sensitive(!self.busy.get() && self.outcome.get() == Outcome::Locked);
        let message = self.error.get().unwrap_or(if self.busy.get() {
            "Authenticating…"
        } else {
            ""
        });
        view.status.set_text(message);
        view.status.set_visible(!message.is_empty());
    }

    fn refresh(&self) {
        for view in self.views.borrow().iter() {
            self.update_view(view);
        }
    }

    fn show_prompt(&self, prompt: bool) {
        self.prompt.set(prompt);
        self.error.set(None);
        if !prompt {
            for view in self.views.borrow().iter() {
                view.entry.set_text("");
                view.entry.set_show_peek_icon(false);
                view.entry.set_show_peek_icon(true);
            }
        }
        self.refresh();
    }

    fn authenticate(self: &Rc<Self>, entry: &gtk::PasswordEntry) {
        if self.outcome.get() != Outcome::Locked || self.busy.get() {
            return;
        }
        // Borrow the secure GTK buffer and copy directly into a zeroed-on-drop buffer.
        let editable: &gtk::Editable = entry.upcast_ref();
        let text =
            unsafe { CStr::from_ptr(gtk::ffi::gtk_editable_get_text(editable.to_glib_none().0)) };
        if text.to_bytes().is_empty() {
            return;
        }
        let password = pam::Password::new(text);
        for view in self.views.borrow().iter() {
            view.entry.set_text("");
        }
        self.busy.set(true);
        self.error.set(None);
        self.refresh();

        let user = self.user.clone();
        let weak = Rc::downgrade(self);
        let entry = entry.downgrade();
        glib::spawn_future_local(async move {
            let verified = gio::spawn_blocking(move || pam::verify(&user, &password))
                .await
                .unwrap_or(false);
            if let Some(session) = weak.upgrade() {
                if session.outcome.get() != Outcome::Locked {
                    return;
                }
                if verified {
                    session.native.unlock();
                } else {
                    session.busy.set(false);
                    session.error.set(Some("Authentication failed"));
                    session.refresh();
                    if session.prompt.get() {
                        if let Some(entry) = entry.upgrade() {
                            entry.grab_focus();
                        }
                    }
                }
            }
        });
    }

    fn bind(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.native
            .object
            .connect_local("monitor", false, move |values| {
                if let (Some(session), Ok(monitor)) =
                    (weak.upgrade(), values[1].get::<gdk::Monitor>())
                {
                    session.add_monitor(&monitor);
                }
                None
            });
        let weak = Rc::downgrade(self);
        self.native.object.connect_local("locked", false, move |_| {
            if let Some(session) = weak.upgrade() {
                session.outcome.set(Outcome::Locked);
                session.refresh();
            }
            None
        });
        let weak = Rc::downgrade(self);
        self.native
            .object
            .connect_local("unlocked", false, move |_| {
                if let Some(session) = weak.upgrade() {
                    session
                        .outcome
                        .set(if session.outcome.get() == Outcome::Locked {
                            Outcome::Unlocked
                        } else {
                            Outcome::Failed
                        });
                    session.main_loop.quit();
                }
                None
            });
        let weak = Rc::downgrade(self);
        self.native.object.connect_local("failed", false, move |_| {
            if let Some(session) = weak.upgrade() {
                session.outcome.set(Outcome::Failed);
                session.main_loop.quit();
            }
            None
        });
    }
}

fn run_lock() -> Result<(), String> {
    gtk::init().map_err(|error| error.to_string())?;
    let display =
        gdk::Display::default().ok_or("A Wayland display is required to lock the session.")?;
    if display.monitors().n_items() == 0 {
        return Err("No monitor is available for the lock screen.".into());
    }
    pam::check()?;
    let user = pam::session_user()?;
    let native = NativeLock::new()?;
    let (background, color) = load_background();
    let css = gtk::CssProvider::new();
    css.load_from_data(&format!(
        "{CSS}\nwindow.lock-window {{ background-color: {color}; }}"
    ));
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let name = glib::real_name();
    let display_name = name.to_string_lossy();
    let display_name = if display_name.is_empty() || display_name == "Unknown" {
        user.to_string_lossy().into_owned()
    } else {
        display_name.into_owned()
    };

    let session = Rc::new(Session {
        native,
        user,
        display_name,
        background,
        avatar: texture(glib::home_dir().join(".face")),
        main_loop: glib::MainLoop::new(None, false),
        outcome: Cell::new(Outcome::Starting),
        views: RefCell::default(),
        next_id: Cell::new(0),
        prompt: Cell::new(false),
        busy: Cell::new(false),
        error: Cell::new(None),
    });
    session.bind();
    if session.native.request() && session.outcome.get() != Outcome::Failed {
        let weak = Rc::downgrade(&session);
        let timer = glib::timeout_add_local(Duration::from_secs(1), move || {
            if let Some(session) = weak.upgrade() {
                update_clock(&session.views.borrow());
            }
            glib::ControlFlow::Continue
        });
        session.main_loop.run();
        timer.remove();
    }
    gtk::style_context_remove_provider_for_display(&display, &css);
    if session.outcome.get() == Outcome::Unlocked {
        Ok(())
    } else {
        Err("The compositor did not grant the session lock.".into())
    }
}

pub fn run() -> glib::ExitCode {
    match run_lock() {
        Ok(()) => glib::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[lockscreen] {error}");
            glib::ExitCode::FAILURE
        }
    }
}

pub fn launch() -> Result<(), String> {
    let executable: PathBuf = std::env::current_exe().map_err(|error| error.to_string())?;
    gio::Subprocess::newv(
        &[executable.as_os_str(), OsStr::new("lock")],
        gio::SubprocessFlags::NONE,
    )
    .map(drop)
    .map_err(|error| error.to_string())
}
