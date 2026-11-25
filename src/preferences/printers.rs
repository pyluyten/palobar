use super::label;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::net::IpAddr;
use std::rc::Rc;

struct Printer<'a> {
    name: &'a str,
    status: &'a str,
    is_default: bool,
}

fn parse_printers(output: &str) -> Vec<Printer<'_>> {
    let default = output
        .lines()
        .find_map(|line| line.strip_prefix("system default destination: "))
        .map(str::trim);
    output
        .lines()
        .filter_map(|line| {
            let (name, status) = line
                .trim()
                .strip_prefix("printer ")?
                .split_once(char::is_whitespace)?;
            let status = status.trim();
            Some(Printer {
                name,
                status: status.strip_prefix("is ").unwrap_or(status),
                is_default: default == Some(name),
            })
        })
        .collect()
}

async fn printer_status() -> Result<String, String> {
    let lpstat =
        glib::find_program_in_path("lpstat").ok_or("CUPS tools (lpstat) not found in PATH")?;
    let launcher = gio::SubprocessLauncher::new(
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    );
    launcher.setenv("LC_ALL", "C", true);
    let process = launcher
        .spawn(&[lpstat.as_os_str(), OsStr::new("-d"), OsStr::new("-p")])
        .map_err(|error| error.to_string())?;
    let (output, error) = process
        .communicate_utf8_future(None)
        .await
        .map_err(|error| error.to_string())?;
    let output = output.map_or_else(String::new, |text| text.to_string());
    let error = error.as_deref().unwrap_or("").trim();
    if process.is_successful()
        || !parse_printers(&output).is_empty()
        || error == "lpstat: No destinations added."
    {
        Ok(output)
    } else if error.is_empty() {
        Err("Could not list CUPS printers.".into())
    } else {
        Err(format!("Could not list CUPS printers. {error}"))
    }
}

fn cups_address(config: &str) -> (String, u16) {
    for line in config.lines() {
        let mut words = line.split_whitespace();
        let Some((directive, value)) = words.next().zip(words.next()) else {
            continue;
        };
        if directive.eq_ignore_ascii_case("port") {
            if let Ok(port) = value.parse::<u16>() {
                if port != 0 {
                    return ("localhost".into(), port);
                }
            }
        }
        if !directive.eq_ignore_ascii_case("listen") {
            continue;
        }
        let Some((host, port)) = value.rsplit_once(':') else {
            continue;
        };
        let Ok(port) = port.parse::<u16>() else {
            continue;
        };
        if port == 0 {
            continue;
        }
        let host = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        if matches!(host, "*" | "" | "0.0.0.0" | "::") {
            return ("localhost".into(), port);
        }
        if host.parse::<IpAddr>().is_ok()
            || host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        {
            return (host.into(), port);
        }
    }
    ("localhost".into(), 631)
}

struct WebStatus {
    url: Option<String>,
    detail: String,
}

fn check_web(host: &str, port: u16) -> WebStatus {
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let url = format!("http://{authority}");
    let error = |detail: String| WebStatus { url: None, detail };
    let client = gio::SocketClient::new();
    client.set_timeout(2);
    client.set_enable_proxy(false);
    let connection = match client.connect_to_host(&authority, port, gio::Cancellable::NONE) {
        Ok(connection) => connection,
        Err(_) => return error(format!("CUPS is not listening on {authority}")),
    };
    let request = format!("GET / HTTP/1.0\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    let mut response = Vec::new();
    if connection
        .output_stream()
        .into_write()
        .write_all(request.as_bytes())
        .is_err()
        || connection
            .input_stream()
            .into_read()
            .take(4096)
            .read_to_end(&mut response)
            .is_err()
    {
        return error("Unable to read the CUPS HTTP response".into());
    }
    let response = String::from_utf8_lossy(&response);
    let code = response.lines().next().and_then(|line| {
        let (protocol, status) = line.split_once(' ')?;
        protocol
            .starts_with("HTTP/")
            .then_some(status.split_whitespace().next()?.parse::<u16>().ok()?)
    });
    match code {
        Some(403 | 404) => error("CUPS web interface is disabled or unavailable".into()),
        Some(_)
            if response
                .to_ascii_lowercase()
                .contains("web interface is disabled") =>
        {
            error("CUPS web interface is disabled on this system".into())
        }
        Some(200..=399 | 401) => WebStatus {
            detail: format!("Available at {url}"),
            url: Some(url),
        },
        Some(code) => error(format!("CUPS web interface returned HTTP {code}")),
        None => error("Unable to read the CUPS HTTP response".into()),
    }
}

fn group() -> gtk::Box {
    let result = gtk::Box::new(gtk::Orientation::Vertical, 0);
    result.add_css_class("prefs-group");
    result
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

fn render_printers(group: &gtk::Box, result: Result<String, String>) {
    while let Some(child) = group.first_child() {
        group.remove(&child);
    }
    let output = match result {
        Ok(output) => output,
        Err(error) => {
            row(group, true).append(&label(&error, "prefs-muted"));
            return;
        }
    };
    let printers = parse_printers(&output);
    if printers.is_empty() {
        row(group, true).append(&label("No printers configured", "prefs-muted"));
    }
    for (index, printer) in printers.iter().enumerate() {
        let row = row(group, index + 1 == printers.len());
        row.append(&gtk::Image::from_icon_name("printer-symbolic"));
        let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let title = if printer.is_default {
            format!("{} (Default)", printer.name)
        } else {
            printer.name.into()
        };
        info.append(&label(&title, "prefs-row-title"));
        info.append(&label(printer.status, "prefs-muted"));
        row.append(&info);
    }
}

struct Page {
    printers: gtk::Box,
    refresh: gtk::Button,
    status: gtk::Label,
    open: gtk::Button,
    url: RefCell<Option<String>>,
}

impl Page {
    fn refresh(self: &Rc<Self>) {
        if !self.refresh.is_sensitive() {
            return;
        }
        self.refresh.set_sensitive(false);
        self.open.set_sensitive(false);
        self.url.replace(None);
        self.status.set_text("Checking CUPS status…");
        let page = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let web = gio::spawn_blocking(|| {
                let config = std::fs::read_to_string("/etc/cups/cupsd.conf").unwrap_or_default();
                let (host, port) = cups_address(&config);
                check_web(&host, port)
            });
            let printers = printer_status().await;
            if let Some(page) = page.upgrade() {
                render_printers(&page.printers, printers);
            }
            let web = web.await.unwrap_or_else(|_| WebStatus {
                url: None,
                detail: "Could not check the CUPS web interface".into(),
            });
            if let Some(page) = page.upgrade() {
                page.open.set_sensitive(web.url.is_some());
                page.status.set_text(&web.detail);
                page.url.replace(web.url);
                page.refresh.set_sensitive(true);
            }
        });
    }
}

pub(super) fn build() -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.add_css_class("prefs-page-body");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = label("Configured Printers", "prefs-group-title");
    title.set_hexpand(true);
    header.append(&title);
    let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh.add_css_class("prefs-btn");
    refresh.set_tooltip_text(Some("Refresh"));
    refresh.update_property(&[gtk::accessible::Property::Label("Refresh")]);
    header.append(&refresh);
    page.append(&header);
    let printers = group();
    row(&printers, true).append(&label("Loading printers…", "prefs-muted"));
    page.append(&printers);

    page.append(&label("CUPS Administration", "prefs-group-title"));
    let cups = group();
    let row = row(&cups, true);
    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.append(&label("CUPS Web Interface", "prefs-row-title"));
    let status = label("Checking CUPS status…", "prefs-muted");
    info.append(&status);
    row.append(&info);
    let open = gtk::Button::with_mnemonic("_Open CUPS");
    open.add_css_class("prefs-btn");
    open.set_sensitive(false);
    open.set_valign(gtk::Align::Center);
    row.append(&open);
    page.append(&cups);

    let state = Rc::new(Page {
        printers,
        refresh,
        status,
        open,
        url: RefCell::default(),
    });
    state.refresh.connect_clicked({
        let state = Rc::downgrade(&state);
        move |_| {
            if let Some(state) = state.upgrade() {
                state.refresh();
            }
        }
    });
    state.open.connect_clicked({
        let state = Rc::downgrade(&state);
        move |button| {
            if let Some(state) = state.upgrade() {
                if let Some(url) = state.url.borrow().as_deref() {
                    let context = button.display().app_launch_context();
                    if let Err(error) = gio::AppInfo::launch_default_for_uri(url, Some(&context)) {
                        state
                            .status
                            .set_text(&format!("Could not open CUPS. {error}"));
                    }
                }
            }
        }
    });
    page.connect_map(move |_| state.refresh());
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}

#[cfg(test)]
mod tests {
    use super::{cups_address, parse_printers};

    #[test]
    fn keeps_printer_status_and_marks_the_default_destination() {
        let printers = parse_printers(
            "system default destination: Office\nprinter Office is idle. enabled since Monday\nprinter Lab disabled since Tuesday\n",
        );
        assert_eq!(printers.len(), 2);
        assert_eq!(printers[0].name, "Office");
        assert_eq!(printers[0].status, "idle. enabled since Monday");
        assert!(printers[0].is_default);
        assert_eq!(printers[1].status, "disabled since Tuesday");
        assert!(!printers[1].is_default);
        assert!(parse_printers("no system default destination\n").is_empty());
    }

    #[test]
    fn skips_unix_sockets_and_handles_wildcard_and_ipv6_listeners() {
        assert_eq!(
            cups_address("Listen /run/cups/cups.sock\nListen [::1]:8631"),
            ("::1".into(), 8631)
        );
        assert_eq!(
            cups_address("Listen 0.0.0.0:731"),
            ("localhost".into(), 731)
        );
        assert_eq!(cups_address("Port 8631"), ("localhost".into(), 8631));
        assert_eq!(
            cups_address("Listen /run/cups/cups.sock"),
            ("localhost".into(), 631)
        );
    }
}
