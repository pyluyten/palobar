use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::rc::Rc;

pub const ACTION: &str = "toggle-clipboard";
const CLASS: &str = "clipboard-window";
const PREVIEW_LIMIT: usize = 25;
const THUMBNAIL_BYTES: usize = 2 * 1024 * 1024;

thread_local! {
    static WATCH_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn missing_tools() -> Option<String> {
    let missing: Vec<_> = ["wl-paste", "wl-copy", "cliphist"]
        .into_iter()
        .filter(|tool| glib::find_program_in_path(tool).is_none())
        .collect();
    (!missing.is_empty()).then(|| {
        format!(
            "Clipboard history requires {} in Palobar's PATH.",
            missing.join(", ")
        )
    })
}

pub fn start_watcher(app: &gtk::Application) {
    let result = missing_tools().map_or_else(
        || {
            gio::Subprocess::newv(
                &["wl-paste", "--watch", "cliphist", "store"].map(OsStr::new),
                gio::SubprocessFlags::STDOUT_SILENCE,
            )
            .map_err(|error| error.to_string())
        },
        Err,
    );
    let process = match result {
        Ok(process) => process,
        Err(error) => {
            eprintln!("[clipboard] {error}");
            WATCH_ERROR.replace(Some(error));
            return;
        }
    };
    WATCH_ERROR.replace(None);
    let wait = process.clone();
    let task = glib::spawn_future_local(async move {
        let _ = wait.wait_future().await;
        WATCH_ERROR.replace(Some(
            "Clipboard capture stopped. Check that your Wayland session supports data-control."
                .into(),
        ));
        eprintln!("[clipboard] wl-paste watcher stopped");
    });
    app.connect_shutdown(move |_| {
        task.abort();
        process.force_exit();
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    id: u64,
    preview: String,
    mime: Option<&'static str>,
}

fn parse_entry(line: &str) -> Option<Entry> {
    let (id, preview) = line.split_once('\t')?;
    let id = id.parse().ok()?;
    let mime = if preview.starts_with("[[ binary data ") {
        preview
            .split_ascii_whitespace()
            .find_map(|word| match word.to_ascii_lowercase().as_str() {
                "png" => Some("image/png"),
                "jpeg" | "jpg" => Some("image/jpeg"),
                "webp" => Some("image/webp"),
                "bmp" => Some("image/bmp"),
                "gif" => Some("image/gif"),
                _ => None,
            })
    } else {
        None
    };
    Some(Entry {
        id,
        preview: preview.to_owned(),
        mime,
    })
}

struct Child(gio::Subprocess);

impl Child {
    fn spawn(args: &[&str], flags: gio::SubprocessFlags) -> Result<Self, glib::Error> {
        let args: Vec<_> = args.iter().map(OsStr::new).collect();
        gio::Subprocess::newv(&args, flags).map(Self)
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        self.0.force_exit();
    }
}

async fn history() -> Result<Vec<Entry>, glib::Error> {
    let process = Child::spawn(
        &["cliphist", "list"],
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    )?;
    let stream =
        gio::DataInputStream::new(&process.0.stdout_pipe().expect("stdout pipe requested"));
    let mut items = Vec::new();
    while items.len() < PREVIEW_LIMIT {
        let Some(line) = stream.read_line_future(glib::Priority::DEFAULT).await? else {
            if let Err(error) = process.0.wait_check_future().await {
                let stderr = process.0.stderr_pipe().expect("stderr pipe requested");
                let detail = stderr
                    .read_bytes_future(4096, glib::Priority::DEFAULT)
                    .await?;
                if !String::from_utf8_lossy(&detail).contains("please store something first") {
                    return Err(error);
                }
            }
            break;
        };
        if let Some(entry) = parse_entry(&String::from_utf8_lossy(&line)) {
            items.push(entry);
        }
    }
    Ok(items)
}

async fn image_preview(id: u64) -> Option<gdk::Texture> {
    let process = Child::spawn(
        &["cliphist", "decode", &id.to_string()],
        gio::SubprocessFlags::STDOUT_PIPE,
    )
    .ok()?;
    let stream = process.0.stdout_pipe()?;
    let mut bytes = Vec::new();
    loop {
        let part = stream
            .read_bytes_future(32 * 1024, glib::Priority::DEFAULT)
            .await
            .ok()?;
        if part.is_empty() {
            break;
        }
        if bytes.len() + part.len() > THUMBNAIL_BYTES {
            return None;
        }
        bytes.extend_from_slice(part.as_ref());
    }
    process.0.wait_check_future().await.ok()?;
    let input = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
    let pixbuf = gdk_pixbuf::Pixbuf::from_stream_at_scale_future(&input, 240, 72, true)
        .await
        .ok()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}

async fn copy(entry: Entry) -> Result<(), glib::Error> {
    let decode = Child::spawn(
        &["cliphist", "decode", &entry.id.to_string()],
        gio::SubprocessFlags::STDOUT_PIPE,
    )?;
    let mut args = vec!["wl-copy"];
    if let Some(mime) = entry.mime {
        args.extend(["--type", mime]);
    }
    let write = Child::spawn(
        &args,
        gio::SubprocessFlags::STDIN_PIPE | gio::SubprocessFlags::STDOUT_SILENCE,
    )?;
    let input = decode.0.stdout_pipe().expect("stdout pipe requested");
    let output = write.0.stdin_pipe().expect("stdin pipe requested");
    output
        .splice_future(
            &input,
            gio::OutputStreamSpliceFlags::CLOSE_SOURCE,
            glib::Priority::DEFAULT,
        )
        .await?;
    decode.0.wait_check_future().await?;
    output.close_future(glib::Priority::DEFAULT).await?;
    write.0.wait_check_future().await
}

enum Action {
    Load,
    Copy(Entry),
    Remove(u64),
    Clear,
}

async fn change_history(action: Action) -> Result<Vec<Entry>, glib::Error> {
    match action {
        Action::Remove(id) => {
            let process = Child::spawn(&["cliphist", "delete"], gio::SubprocessFlags::STDIN_PIPE)?;
            process
                .0
                .communicate_future(Some(&glib::Bytes::from_owned(
                    format!("{id}\t\n").into_bytes(),
                )))
                .await?;
            process.0.wait_check_future().await?;
        }
        Action::Clear => {
            let process = Child::spawn(&["cliphist", "wipe"], gio::SubprocessFlags::NONE)?;
            process.0.wait_check_future().await?;
        }
        Action::Load | Action::Copy(_) => {}
    }
    history().await
}

struct Row {
    entry: Entry,
    query_text: String,
    widget: gtk::Box,
}

struct Drawer {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    list: gtk::Box,
    search: gtk::Entry,
    clear: gtk::Button,
    scroll: gtk::ScrolledWindow,
    status: gtk::Box,
    message: gtk::Label,
    rows: RefCell<Vec<Row>>,
    pending: RefCell<Option<glib::JoinHandle<()>>>,
    previews: RefCell<Option<glib::JoinHandle<()>>>,
    busy: Cell<bool>,
    error: RefCell<Option<&'static str>>,
}

impl Drawer {
    fn filter(&self) {
        let query = self.search.text().trim().to_lowercase();
        let rows = self.rows.borrow();
        let mut matches = false;
        for row in &*rows {
            let show = row.query_text.contains(&query);
            matches |= show;
            row.widget.set_visible(show);
        }
        self.clear.set_visible(!rows.is_empty());
        self.scroll.set_visible(matches);
        let watch_error =
            missing_tools().or_else(|| WATCH_ERROR.with(|error| error.borrow().clone()));
        let error = self.error.borrow();
        self.status.set_visible(error.is_some() || !matches);
        self.status.set_vexpand(!matches);
        self.message.set_text(if let Some(error) = *error {
            error
        } else if self.busy.get() {
            "Loading clipboard history…"
        } else if let Some(error) = watch_error.as_deref() {
            error
        } else if rows.is_empty() {
            "Clipboard is empty"
        } else {
            "No matching entries"
        });
    }

    fn active(self: &Rc<Self>, action: Action) {
        if self.busy.replace(true) {
            return;
        }
        self.error.replace(None);
        self.list.set_sensitive(false);
        self.clear.set_sensitive(false);
        self.filter();
        let weak = Rc::downgrade(self);
        let task = glib::spawn_future_local(async move {
            let result = match action {
                Action::Copy(entry) => copy(entry).await.map(|()| None),
                other => change_history(other).await.map(Some),
            };
            let Some(drawer) = weak.upgrade() else { return };
            drawer.busy.set(false);
            drawer.list.set_sensitive(true);
            drawer.clear.set_sensitive(true);
            match result {
                Ok(Some(entries)) => drawer.render(entries),
                Ok(None) => {
                    if let Some(window) = drawer.window.upgrade() {
                        window.close();
                    }
                }
                Err(error) => {
                    eprintln!("[clipboard] {error}");
                    drawer.error.replace(Some(
                        "The clipboard operation failed. Check Palobar's logs.",
                    ));
                    drawer.filter();
                }
            }
        });
        self.pending.replace(Some(task));
    }

    fn render(self: &Rc<Self>, entries: Vec<Entry>) {
        if let Some(task) = self.previews.take() {
            task.abort();
        }
        self.rows.borrow_mut().clear();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut thumbnails = Vec::new();
        for entry in entries {
            let row = gtk::Box::builder()
                .spacing(8)
                .valign(gtk::Align::Center)
                .build();
            row.add_css_class("clipboard-item");
            let label = gtk::Label::builder()
                .label(&entry.preview)
                .xalign(0.0)
                .lines(2)
                .max_width_chars(36)
                .wrap(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
            content.set_halign(gtk::Align::Start);
            if entry.mime.is_some() {
                let picture = gtk::Picture::builder()
                    .halign(gtk::Align::Start)
                    .height_request(72)
                    .visible(false)
                    .build();
                picture.add_css_class("clipboard-thumb");
                picture.set_overflow(gtk::Overflow::Hidden);
                if let Some(metadata) = entry
                    .preview
                    .strip_prefix("[[ binary data")
                    .and_then(|text| text.strip_suffix("]]"))
                {
                    label.set_text(metadata.trim());
                }
                thumbnails.push((entry.id, picture.downgrade()));
                content.append(&picture);
                label.add_css_class("clipboard-meta");
            } else {
                label.add_css_class("clipboard-text");
            }
            content.append(&label);
            let choose = gtk::Button::builder().hexpand(true).child(&content).build();
            choose.add_css_class("clipboard-select-btn");
            let weak = Rc::downgrade(self);
            let selected = entry.clone();
            choose.connect_clicked(move |_| {
                if let Some(drawer) = weak.upgrade() {
                    drawer.active(Action::Copy(selected.clone()));
                }
            });
            let remove = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text("Remove")
                .valign(gtk::Align::Center)
                .build();
            let weak = Rc::downgrade(self);
            remove.add_css_class("clipboard-remove-btn");
            let id = entry.id;
            remove.connect_clicked(move |_| {
                if let Some(drawer) = weak.upgrade() {
                    drawer.active(Action::Remove(id));
                }
            });
            row.append(&choose);
            row.append(&remove);
            self.list.append(&row);
            let query_text = entry.preview.to_lowercase();
            self.rows.borrow_mut().push(Row {
                entry,
                query_text,
                widget: row,
            });
        }
        self.filter();
        if !thumbnails.is_empty() {
            self.previews
                .replace(Some(glib::spawn_future_local(async move {
                    for (id, picture) in thumbnails {
                        if let Some(texture) = image_preview(id).await {
                            if let Some(picture) = picture.upgrade() {
                                picture.set_paintable(Some(&texture));
                                picture.set_visible(true);
                            }
                        }
                    }
                })));
        }
    }

    fn dispose(&self) {
        for task in [&self.pending, &self.previews] {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}

pub fn is_open(app: &gtk::Application) -> bool {
    app.windows()
        .iter()
        .any(|window| window.has_css_class(CLASS))
}

pub fn close(app: &gtk::Application) -> bool {
    crate::panel_popup::close(app, CLASS)
}

pub fn install_style(display: &gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("clipboard.css"));
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn toggle(app: &gtk::Application, monitor: &gdk::Monitor, indicator: Option<&gtk::Button>) {
    if close(app) {
        return;
    }
    crate::calendar::close(app);
    crate::quick_settings::close(app);
    crate::workspace_dialog::close(app);
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Clipboard")
        .build();
    window.add_css_class(CLASS);
    window.init_layer_shell();
    window.set_namespace(Some("clipboard-popup"));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, 6);
    window.set_margin(Edge::Right, 10);
    window.set_keyboard_mode(KeyboardMode::OnDemand);

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .height_request(480.min(monitor.geometry().height() * 4 / 5))
        .build();
    card.add_css_class("clipboard-drawer");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::builder()
        .label("Clipboard")
        .xalign(0.0)
        .hexpand(true)
        .build();
    title.add_css_class("clipboard-title");
    header.append(&title);
    let clear = gtk::Button::with_label("Clear");
    clear.add_css_class("clipboard-clear-btn");
    header.append(&clear);
    card.append(&header);
    let search = gtk::Entry::builder()
        .placeholder_text("Filter history…")
        .primary_icon_name("system-search-symbolic")
        .build();
    search.add_css_class("clipboard-search");
    card.append(&search);
    let message = gtk::Label::builder().wrap(true).max_width_chars(38).build();
    let status = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .valign(gtk::Align::Center)
        .spacing(8)
        .build();
    status.add_css_class("clipboard-status");
    status.append(&gtk::Image::from_icon_name("edit-paste-symbolic"));
    status.append(&message);
    card.append(&status);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    card.append(&scroll);
    window.set_child(Some(&card));

    let drawer = Rc::new(Drawer {
        window: window.downgrade(),
        list,
        search,
        clear,
        scroll,
        status,
        message,
        rows: RefCell::default(),
        pending: RefCell::default(),
        previews: RefCell::default(),
        busy: Cell::default(),
        error: RefCell::default(),
    });
    let weak = Rc::downgrade(&drawer);
    drawer.search.connect_changed(move |_| {
        if let Some(drawer) = weak.upgrade() {
            drawer.filter();
        }
    });
    let weak = Rc::downgrade(&drawer);
    drawer.search.connect_activate(move |_| {
        if let Some(drawer) = weak.upgrade() {
            let first = drawer
                .rows
                .borrow()
                .iter()
                .find(|row| row.widget.is_visible())
                .map(|row| row.entry.clone());
            if let Some(entry) = first {
                drawer.active(Action::Copy(entry));
            }
        }
    });
    let weak = Rc::downgrade(&drawer);
    drawer.clear.connect_clicked(move |_| {
        if let Some(drawer) = weak.upgrade() {
            drawer.active(Action::Clear);
        }
    });
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak_window = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key != gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(window) = weak_window.upgrade() {
            window.close();
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
    let indicator = indicator.map(|button| {
        button.add_css_class("open");
        button.downgrade()
    });
    drawer.active(Action::Load);
    let search = drawer.search.clone();
    window.connect_unrealize(move |_| {
        drawer.dispose();
        if let Some(button) = indicator.as_ref().and_then(glib::WeakRef::upgrade) {
            button.remove_css_class("open");
        }
    });
    glib::spawn_future_local(async {
        let _ = crate::quick_settings::command(&["palobar-ctl", "close-popup"]).await;
    });
    window.present();
    search.grab_focus();
}

pub fn toggle_first_monitor(app: &gtk::Application) {
    if let Some(monitor) = gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gdk::Monitor>()
    {
        toggle(app, &monitor, None);
    }
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

#[cfg(test)]
mod tests {
    use super::parse_entry;

    #[test]
    fn parses_cliphist_images_and_preserves_tabs_and_spaces_in_text() {
        let image = parse_entry("123\t[[ binary data 678 B png 12x10 ]]").unwrap();
        assert_eq!(image.id, 123);
        assert_eq!(image.mime, Some("image/png"));
        let text = parse_entry("4\t hello\tworld ").unwrap();
        assert_eq!(text.preview, " hello\tworld ");
        assert_eq!(text.mime, None);
        assert!(parse_entry("not-an-id\tpreview").is_none());
    }
}
