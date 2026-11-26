use super::label;
use glib::variant::ObjectPath;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const AGENT_PATH: &str = "/io/Astal/Palobar/BluetoothAgent";
const AGENT_IFACE: &str = "org.bluez.Agent1";
const AGENT_XML: &str = r#"<node><interface name="org.bluez.Agent1">
    <method name="Release"/>
    <method name="RequestPinCode"><arg type="o" direction="in"/><arg type="s" direction="out"/></method>
    <method name="DisplayPinCode"><arg type="o" direction="in"/><arg type="s" direction="in"/></method>
    <method name="RequestPasskey"><arg type="o" direction="in"/><arg type="u" direction="out"/></method>
    <method name="DisplayPasskey"><arg type="o" direction="in"/><arg type="u" direction="in"/><arg type="q" direction="in"/></method>
    <method name="RequestConfirmation"><arg type="o" direction="in"/><arg type="u" direction="in"/></method>
    <method name="RequestAuthorization"><arg type="o" direction="in"/></method>
    <method name="AuthorizeService"><arg type="o" direction="in"/><arg type="s" direction="in"/></method>
    <method name="Cancel"/>
</interface></node>"#;

fn agent_path() -> ObjectPath {
    ObjectPath::try_from(AGENT_PATH).expect("valid Bluetooth agent object path")
}

fn register_agent_parameters() -> glib::Variant {
    (agent_path(), "KeyboardDisplay").to_variant()
}

fn agent_path_parameters() -> glib::Variant {
    (agent_path(),).to_variant()
}

type Properties = HashMap<String, glib::Variant>;
type Interfaces = HashMap<String, Properties>;
type ManagedObjects = HashMap<ObjectPath, Interfaces>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AdapterInfo {
    path: String,
    alias: String,
    powered: bool,
    discovering: bool,
    discoverable: bool,
    discoverable_timeout: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DeviceInfo {
    path: String,
    name: String,
    address: String,
    icon: String,
    appearance: u16,
    paired: bool,
    connected: bool,
}

#[derive(Default)]
struct Snapshot {
    adapters: Vec<AdapterInfo>,
    devices: Vec<DeviceInfo>,
}

fn property<T: glib::variant::FromVariant>(properties: &Properties, key: &str) -> Option<T> {
    properties.get(key)?.get()
}

fn snapshot(objects: ManagedObjects) -> Snapshot {
    let mut result = Snapshot::default();
    for (path, interfaces) in objects {
        if let Some(properties) = interfaces.get(ADAPTER) {
            let alias = property::<String>(properties, "Alias")
                .or_else(|| property(properties, "Name"))
                .unwrap_or_default();
            result.adapters.push(AdapterInfo {
                path: path.as_str().to_owned(),
                alias,
                powered: property(properties, "Powered").unwrap_or(false),
                discovering: property(properties, "Discovering").unwrap_or(false),
                discoverable: property(properties, "Discoverable").unwrap_or(false),
                discoverable_timeout: property(properties, "DiscoverableTimeout").unwrap_or(0),
            });
        }
        if let Some(properties) = interfaces.get(DEVICE) {
            let address = property::<String>(properties, "Address").unwrap_or_default();
            let name = property::<String>(properties, "Alias")
                .or_else(|| property(properties, "Name"))
                .filter(|name: &String| !name.is_empty())
                .unwrap_or_else(|| address.clone());
            result.devices.push(DeviceInfo {
                path: path.as_str().to_owned(),
                name,
                address,
                icon: property(properties, "Icon").unwrap_or_default(),
                appearance: property(properties, "Appearance").unwrap_or_default(),
                paired: property(properties, "Paired").unwrap_or(false),
                connected: property(properties, "Connected").unwrap_or(false),
            });
        }
    }
    result.adapters.sort_by(|a, b| a.path.cmp(&b.path));
    result.devices.sort_by(|a, b| {
        b.paired
            .cmp(&a.paired)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    result
}

fn device_type(device: &DeviceInfo) -> &'static str {
    match device.icon.as_str() {
        "audio-card" | "audio-headphones" | "audio-headset" | "audio-speakers" => "Audio",
        "input-mouse" => "Mouse",
        "input-keyboard" => "Keyboard",
        "phone" => "Phone",
        "computer" => "Computer",
        "printer" => "Printer",
        _ => match device.appearance {
            0x0040..=0x007f => "Phone",
            0x0080..=0x00bf => "Computer",
            0x00c0..=0x00ff => "Watch",
            0x0140..=0x017f => "Display",
            0x01c0..=0x01df => "Glasses",
            0x0200..=0x021f => "Tag",
            0x0240..=0x025f => "Keyring",
            0x0400..=0x04bf => "Audio",
            0x03c0..=0x03ff => "Input device",
            _ => "Bluetooth device",
        },
    }
}

async fn call(
    connection: &gio::DBusConnection,
    path: &str,
    interface: &str,
    method: &str,
    parameters: Option<&glib::Variant>,
) -> Result<glib::Variant, String> {
    connection
        .call_future(
            Some(BLUEZ),
            path,
            interface,
            method,
            parameters,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            5000,
        )
        .await
        .map_err(|error| error.to_string())
}

async fn read_snapshot(connection: &gio::DBusConnection) -> Result<Snapshot, String> {
    let reply = call(
        connection,
        "/",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        None,
    )
    .await?;
    decode_snapshot(reply)
}

fn decode_snapshot(reply: glib::Variant) -> Result<Snapshot, String> {
    let (objects,) = reply
        .get::<(ManagedObjects,)>()
        .ok_or_else(|| "BlueZ returned an invalid object list".to_owned())?;
    Ok(snapshot(objects))
}

async fn set_property(
    connection: &gio::DBusConnection,
    path: &str,
    interface: &str,
    property: &str,
    value: glib::Variant,
) -> Result<(), String> {
    call(
        connection,
        path,
        "org.freedesktop.DBus.Properties",
        "Set",
        Some(&(interface, property, value).to_variant()),
    )
    .await
    .map(drop)
}

struct Page {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    status: gtk::Label,
    powered: gtk::Switch,
    alias: gtk::Entry,
    alias_dirty: Cell<bool>,
    save_alias: gtk::Button,
    devices: gtk::ListBox,
    syncing: Cell<bool>,
    busy: Cell<bool>,
    connection: RefCell<Option<gio::DBusConnection>>,
    adapters: RefCell<Vec<AdapterInfo>>,
    rendered: RefCell<Vec<DeviceInfo>>,
    refresh: RefCell<Option<glib::JoinHandle<()>>>,
    agent: RefCell<Option<(gio::DBusConnection, gio::RegistrationId)>>,
    agent_busy: Cell<bool>,
    agent_error: RefCell<Option<String>>,
    visibility: RefCell<HashMap<String, (bool, u32)>>,
    visibility_busy: Cell<bool>,
    discovery_started: Cell<bool>,
}

impl Page {
    fn new(
        window: &gtk::ApplicationWindow,
        status: gtk::Label,
        powered: gtk::Switch,
        alias: gtk::Entry,
        save_alias: gtk::Button,
        devices: gtk::ListBox,
    ) -> Rc<Self> {
        Rc::new(Self {
            window: window.downgrade(),
            status,
            powered,
            alias,
            alias_dirty: Cell::new(false),
            save_alias,
            devices,
            syncing: Cell::new(false),
            busy: Cell::new(false),
            connection: RefCell::default(),
            adapters: RefCell::default(),
            rendered: RefCell::default(),
            refresh: RefCell::default(),
            agent: RefCell::default(),
            agent_busy: Cell::default(),
            agent_error: RefCell::default(),
            visibility: RefCell::default(),
            visibility_busy: Cell::default(),
            discovery_started: Cell::default(),
        })
    }

    fn start(self: &Rc<Self>) {
        if self.refresh.borrow().is_some() {
            return;
        }
        let page = self.clone();
        self.refresh
            .replace(Some(glib::spawn_future_local(async move {
                loop {
                    page.refresh_once().await;
                    glib::timeout_future(Duration::from_secs(3)).await;
                }
            })));
    }

    async fn refresh_once(self: &Rc<Self>) {
        if self.connection.borrow().is_none() {
            match gio::bus_get_future(gio::BusType::System).await {
                Ok(connection) => {
                    self.connection.replace(Some(connection.clone()));
                }
                Err(error) => {
                    self.report(Some(&format!(
                        "Could not connect to the system bus. {error}"
                    )));
                    return;
                }
            }
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        match read_snapshot(&connection).await {
            Ok(snapshot) => {
                self.update(snapshot);
                self.register_agent(&connection);
            }
            Err(error) => {
                self.report(Some(&format!("Could not read Bluetooth devices. {error}")));
                self.powered.set_sensitive(false);
                self.alias.set_sensitive(false);
                self.save_alias.set_sensitive(false);
            }
        }
    }

    fn report(&self, message: Option<&str>) {
        self.status.set_text(message.unwrap_or_default());
        self.status.set_visible(message.is_some());
    }

    fn update(self: &Rc<Self>, mut snapshot: Snapshot) {
        self.report(self.agent_error.borrow().as_deref());
        self.powered
            .set_sensitive(!snapshot.adapters.is_empty() && !self.busy.get());
        let powered = !snapshot.adapters.is_empty() && snapshot.adapters.iter().all(|a| a.powered);
        self.syncing.set(true);
        self.powered.set_active(powered);
        if !self.alias_dirty.get() && !self.alias.has_focus() {
            self.alias.set_text(
                snapshot
                    .adapters
                    .first()
                    .map_or("", |adapter| &adapter.alias),
            );
        }
        self.alias.set_sensitive(powered && !self.busy.get());
        self.save_alias.set_sensitive(powered && !self.busy.get());
        self.syncing.set(false);

        if snapshot.adapters.is_empty() {
            self.report(Some("No Bluetooth adapter found."));
        } else {
            for adapter in &snapshot.adapters {
                self.visibility
                    .borrow_mut()
                    .entry(adapter.path.clone())
                    .or_insert((adapter.discoverable, adapter.discoverable_timeout));
            }
        }
        if powered
            && !self.discovery_started.get()
            && !snapshot.adapters.iter().any(|adapter| adapter.discovering)
        {
            let connection = self.connection.borrow().clone();
            let adapters: Vec<_> = snapshot
                .adapters
                .iter()
                .filter(|adapter| adapter.powered)
                .map(|adapter| adapter.path.clone())
                .collect();
            if let Some(connection) = connection {
                self.discovery_started.set(true);
                let page = self.clone();
                glib::spawn_future_local(async move {
                    for adapter in adapters {
                        if call(&connection, &adapter, ADAPTER, "StartDiscovery", None)
                            .await
                            .is_err()
                        {
                            page.discovery_started.set(false);
                        }
                    }
                });
            }
        }
        if powered
            && !self.visibility_busy.get()
            && snapshot
                .adapters
                .iter()
                .any(|adapter| !adapter.discoverable)
        {
            if let Some(connection) = self.connection.borrow().clone() {
                self.visibility_busy.set(true);
                let adapters: Vec<_> = snapshot
                    .adapters
                    .iter()
                    .filter(|adapter| adapter.powered && !adapter.discoverable)
                    .map(|adapter| adapter.path.clone())
                    .collect();
                let page = self.clone();
                glib::spawn_future_local(async move {
                    for path in adapters {
                        let _ = set_property(
                            &connection,
                            &path,
                            ADAPTER,
                            "DiscoverableTimeout",
                            0_u32.to_variant(),
                        )
                        .await;
                        let _ = set_property(
                            &connection,
                            &path,
                            ADAPTER,
                            "Discoverable",
                            true.to_variant(),
                        )
                        .await;
                    }
                    page.visibility_busy.set(false);
                });
            }
        }
        snapshot.devices.retain(|device| {
            snapshot
                .adapters
                .iter()
                .any(|adapter| device.path.starts_with(&format!("{}/", adapter.path)))
        });
        if !powered {
            snapshot.devices.retain(|device| device.paired);
        }
        self.render_devices(snapshot.devices);
        self.adapters.replace(snapshot.adapters);
    }

    fn render_devices(self: &Rc<Self>, devices: Vec<DeviceInfo>) {
        if *self.rendered.borrow() == devices {
            return;
        }
        while let Some(child) = self.devices.first_child() {
            self.devices.remove(&child);
        }
        if devices.is_empty() {
            let empty = gtk::ListBoxRow::new();
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            content.add_css_class("prefs-row");
            content.add_css_class("last");
            content.append(&label("No devices found", "prefs-muted"));
            empty.set_child(Some(&content));
            self.devices.append(&empty);
        }
        for (index, device) in devices.iter().enumerate() {
            let row = gtk::ListBoxRow::new();
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            content.add_css_class("prefs-row");
            if index + 1 == devices.len() {
                content.add_css_class("last");
            }
            content.append(&gtk::Image::from_icon_name(device_icon(device)));
            let name = label(&device.name, "prefs-row-title");
            name.set_hexpand(true);
            content.append(&name);
            content.append(&label(device_state(device), "prefs-muted"));
            row.set_child(Some(&content));
            self.devices.append(&row);
        }
        self.rendered.replace(devices);
    }

    fn show_device(self: &Rc<Self>, device: &DeviceInfo) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let dialog = gtk::Dialog::builder()
            .title(&device.name)
            .transient_for(&window)
            .modal(true)
            .default_width(420)
            .build();
        dialog.add_button("Close", gtk::ResponseType::Close);
        if device.paired {
            if device.connected {
                dialog.add_button("Disconnect", gtk::ResponseType::Reject);
            } else {
                dialog.add_button("Connect", gtk::ResponseType::Accept);
            }
            dialog.add_button("Forget Device", gtk::ResponseType::Yes);
        } else {
            dialog.add_button("Pair", gtk::ResponseType::Apply);
        }
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("prefs-group");
        let rows = [
            ("Paired", if device.paired { "Yes" } else { "No" }),
            ("Type", device_type(device)),
            ("Address", device.address.as_str()),
            (
                "Connection",
                if device.connected {
                    "Connected"
                } else {
                    "Disconnected"
                },
            ),
        ];
        for (index, (title, value)) in rows.into_iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
            row.add_css_class("prefs-row");
            if index + 1 == rows.len() {
                row.add_css_class("last");
            }
            row.append(&label(title, "prefs-muted"));
            row.append(&label(value, "prefs-row-title"));
            body.append(&row);
        }
        dialog.content_area().append(&body);
        let page = self.clone();
        let device = device.clone();
        dialog.connect_response(move |dialog, response| {
            dialog.close();
            match response {
                gtk::ResponseType::Accept => page.device_action(&device, "Connect"),
                gtk::ResponseType::Reject => page.device_action(&device, "Disconnect"),
                gtk::ResponseType::Apply => page.device_action(&device, "Pair"),
                gtk::ResponseType::Yes => page.device_action(&device, "Forget"),
                _ => {}
            }
        });
        dialog.present();
    }

    fn device_action(self: &Rc<Self>, device: &DeviceInfo, action: &'static str) {
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let path = device.path.clone();
        let connected = device.connected;
        let page = self.clone();
        glib::spawn_future_local(async move {
            page.busy.set(true);
            let result = match action {
                "Connect" | "Disconnect" | "Pair" => call(&connection, &path, DEVICE, action, None)
                    .await
                    .map(drop),
                "Forget" => {
                    let adapter = page
                        .adapters
                        .borrow()
                        .iter()
                        .find(|adapter| path.starts_with(&format!("{}/", adapter.path)))
                        .map(|adapter| adapter.path.clone());
                    match adapter {
                        Some(adapter) => {
                            if connected {
                                let _ = call(&connection, &path, DEVICE, "Disconnect", None).await;
                            }
                            call(
                                &connection,
                                &adapter,
                                ADAPTER,
                                "RemoveDevice",
                                Some(
                                    &(ObjectPath::try_from(path.as_str())
                                        .expect("valid BlueZ device path"),)
                                        .to_variant(),
                                ),
                            )
                            .await
                            .map(drop)
                        }
                        None => Err("Bluetooth adapter not found".into()),
                    }
                }
                _ => return,
            };
            page.busy.set(false);
            page.refresh_once().await;
            match result {
                Ok(()) => page.report(None),
                Err(error) => {
                    let action = action.to_lowercase();
                    page.report(Some(&format!("Could not {action} device. {error}")));
                }
            }
        });
    }

    fn set_power(self: &Rc<Self>, powered: bool) {
        if self.syncing.get() || self.busy.replace(true) {
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            self.busy.set(false);
            return;
        };
        let adapters: Vec<_> = self
            .adapters
            .borrow()
            .iter()
            .map(|a| a.path.clone())
            .collect();
        let page = self.clone();
        glib::spawn_future_local(async move {
            let mut last_error = None;
            for path in adapters {
                if let Err(error) =
                    set_property(&connection, &path, ADAPTER, "Powered", powered.to_variant()).await
                {
                    last_error = Some(error);
                }
            }
            page.busy.set(false);
            page.refresh_once().await;
            if let Some(error) = last_error {
                page.report(Some(&format!("Could not change Bluetooth power. {error}")));
            }
        });
    }

    fn save_alias(self: &Rc<Self>) {
        let Some(adapter) = self.adapters.borrow().first().cloned() else {
            return;
        };
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let alias = self.alias.text().to_string().trim().to_owned();
        if alias.is_empty() {
            self.report(Some("The Bluetooth device name cannot be empty."));
            return;
        }
        let page = self.clone();
        glib::spawn_future_local(async move {
            page.busy.set(true);
            let result = set_property(
                &connection,
                &adapter.path,
                ADAPTER,
                "Alias",
                alias.to_variant(),
            )
            .await;
            page.busy.set(false);
            page.refresh_once().await;
            if let Err(error) = result {
                page.report(Some(&format!(
                    "Could not update the Bluetooth name. {error}"
                )));
            } else {
                page.alias_dirty.set(false);
                page.report(None);
            }
        });
    }

    fn register_agent(self: &Rc<Self>, connection: &gio::DBusConnection) {
        if self.agent.borrow().is_some() || self.agent_busy.replace(true) {
            return;
        }
        let Ok(node) = gio::DBusNodeInfo::for_xml(AGENT_XML) else {
            self.agent_busy.set(false);
            return;
        };
        let Some(interface) = node.lookup_interface(AGENT_IFACE) else {
            self.agent_busy.set(false);
            return;
        };
        let page = Rc::downgrade(self);
        let Ok(id) = connection
            .register_object(AGENT_PATH, &interface)
            .method_call(move |_, _, _, _, method, parameters, invocation| {
                let Some(page) = page.upgrade() else {
                    invocation.return_dbus_error("org.bluez.Error.Canceled", "Settings closed");
                    return;
                };
                page.agent_request(method, &parameters, invocation);
            })
            .build()
        else {
            self.agent_busy.set(false);
            return;
        };
        let connection = connection.clone();
        let page = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let registration = call(
                &connection,
                "/org/bluez",
                "org.bluez.AgentManager1",
                "RegisterAgent",
                Some(&register_agent_parameters()),
            )
            .await;
            let registered = registration.is_ok();
            let agent_error = registration.err().map(|error| error.to_string());
            if registered {
                if let Err(error) = call(
                    &connection,
                    "/org/bluez",
                    "org.bluez.AgentManager1",
                    "RequestDefaultAgent",
                    Some(&agent_path_parameters()),
                )
                .await
                {
                    if let Some(page) = page.upgrade() {
                        let message = format!(
                            "Bluetooth pairing agent could not become the default: {error}"
                        );
                        page.agent_error.replace(Some(message.clone()));
                        page.report(Some(&message));
                    }
                }
            }
            if let Some(page) = page.upgrade() {
                page.agent_busy.set(false);
                if registered {
                    page.agent.replace(Some((connection, id)));
                } else {
                    let _ = connection.unregister_object(id);
                    let error = agent_error.unwrap_or_else(|| "unknown BlueZ error".into());
                    let message = format!("Could not register a Bluetooth pairing agent: {error}");
                    page.agent_error.replace(Some(message.clone()));
                    page.report(Some(&message));
                }
            } else {
                if registered {
                    let _ = call(
                        &connection,
                        "/org/bluez",
                        "org.bluez.AgentManager1",
                        "UnregisterAgent",
                        Some(&agent_path_parameters()),
                    )
                    .await;
                }
                let _ = connection.unregister_object(id);
            }
        });
    }

    fn agent_request(
        self: &Rc<Self>,
        method: &str,
        parameters: &glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        match method {
            "Release" | "Cancel" => invocation.return_value(None),
            "DisplayPinCode" => {
                let pin = parameters.get::<(ObjectPath, String)>().map(|(_, pin)| pin);
                if let Some(pin) = pin {
                    self.report(Some(&format!("Enter Bluetooth PIN {pin} on the device.")));
                }
                invocation.return_value(None);
            }
            "DisplayPasskey" => {
                let passkey = parameters
                    .get::<(ObjectPath, u32, u16)>()
                    .map(|(_, pin, _)| pin);
                if let Some(passkey) = passkey {
                    self.report(Some(&format!("Confirm Bluetooth code {passkey:06}.")));
                }
                invocation.return_value(None);
            }
            "RequestConfirmation" => {
                let passkey = parameters
                    .get::<(ObjectPath, u32)>()
                    .map(|(_, passkey)| passkey);
                self.confirm_agent(
                    &format!(
                        "Confirm that the code {} matches the device.",
                        passkey.unwrap_or(0)
                    ),
                    invocation,
                );
            }
            "RequestAuthorization" | "AuthorizeService" => {
                self.confirm_agent("Allow this Bluetooth device to connect?", invocation);
            }
            "RequestPinCode" => self.prompt_agent(
                "Enter the Bluetooth PIN requested by the device.",
                invocation,
                false,
            ),
            "RequestPasskey" => self.prompt_agent(
                "Enter the six-digit passkey requested by the device.",
                invocation,
                true,
            ),
            _ => invocation.return_dbus_error("org.bluez.Error.Rejected", "Unsupported request"),
        }
    }

    fn confirm_agent(&self, message: &str, invocation: gio::DBusMethodInvocation) {
        let Some(window) = self.window.upgrade() else {
            invocation.return_dbus_error("org.bluez.Error.Canceled", "Settings closed");
            return;
        };
        let dialog = gtk::MessageDialog::builder()
            .transient_for(&window)
            .modal(true)
            .text(message)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Confirm", gtk::ResponseType::Accept);
        let invocation = RefCell::new(Some(invocation));
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(invocation) = invocation.take() {
                    invocation.return_value(None);
                }
            } else {
                if let Some(invocation) = invocation.take() {
                    invocation.return_dbus_error("org.bluez.Error.Rejected", "Pairing rejected");
                }
            }
            dialog.close();
        });
        dialog.present();
    }

    fn prompt_agent(&self, message: &str, invocation: gio::DBusMethodInvocation, numeric: bool) {
        let Some(window) = self.window.upgrade() else {
            invocation.return_dbus_error("org.bluez.Error.Canceled", "Settings closed");
            return;
        };
        let dialog = gtk::Dialog::builder()
            .transient_for(&window)
            .modal(true)
            .title("Bluetooth Pairing")
            .build();
        let entry = gtk::Entry::new();
        if numeric {
            entry.set_input_purpose(gtk::InputPurpose::Digits);
            entry.set_max_length(6);
        }
        dialog
            .content_area()
            .append(&label(message, "prefs-row-title"));
        dialog.content_area().append(&entry);
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Continue", gtk::ResponseType::Accept);
        let invocation = RefCell::new(Some(invocation));
        dialog.connect_response(move |dialog, response| {
            let text = entry.text();
            if response == gtk::ResponseType::Accept
                && !text.is_empty()
                && (!numeric || text.parse::<u32>().is_ok())
            {
                let Some(invocation) = invocation.take() else {
                    dialog.close();
                    return;
                };
                if numeric {
                    invocation.return_value(Some(
                        &(text.parse::<u32>().unwrap_or_default(),).to_variant(),
                    ));
                } else {
                    invocation.return_value(Some(&(text.as_str(),).to_variant()));
                }
            } else {
                if let Some(invocation) = invocation.take() {
                    invocation.return_dbus_error("org.bluez.Error.Rejected", "Pairing canceled");
                }
            }
            dialog.close();
        });
        dialog.present();
    }

    fn stop(self: &Rc<Self>) {
        if let Some(task) = self.refresh.take() {
            task.abort();
        }
        if let Some((connection, id)) = self.agent.take() {
            glib::spawn_future_local(async move {
                let _ = call(
                    &connection,
                    "/org/bluez",
                    "org.bluez.AgentManager1",
                    "UnregisterAgent",
                    Some(&agent_path_parameters()),
                )
                .await;
                let _ = connection.unregister_object(id);
            });
        }
        if let Some(connection) = self.connection.borrow().clone() {
            if self.discovery_started.replace(false) {
                for adapter in self
                    .adapters
                    .borrow()
                    .iter()
                    .filter(|adapter| adapter.powered)
                {
                    let connection = connection.clone();
                    let path = adapter.path.clone();
                    glib::spawn_future_local(async move {
                        let _ = call(&connection, &path, ADAPTER, "StopDiscovery", None).await;
                    });
                }
            }
            for (path, (discoverable, timeout)) in self.visibility.take() {
                let connection = connection.clone();
                glib::spawn_future_local(async move {
                    let _ = set_property(
                        &connection,
                        &path,
                        ADAPTER,
                        "DiscoverableTimeout",
                        timeout.to_variant(),
                    )
                    .await;
                    let _ = set_property(
                        &connection,
                        &path,
                        ADAPTER,
                        "Discoverable",
                        discoverable.to_variant(),
                    )
                    .await;
                });
            }
        }
    }
}

fn device_icon(device: &DeviceInfo) -> &'static str {
    match device_type(device) {
        "Audio" => "audio-card-symbolic",
        "Mouse" => "input-mouse-symbolic",
        "Keyboard" => "input-keyboard-symbolic",
        "Phone" => "phone-symbolic",
        "Computer" => "computer-symbolic",
        _ => "bluetooth-active-symbolic",
    }
}

fn device_state(device: &DeviceInfo) -> &'static str {
    if device.connected {
        "Connected"
    } else if device.paired {
        "Disconnected"
    } else {
        "Not paired"
    }
}

fn group() -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("prefs-group");
    group
}

pub(super) fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.add_css_class("prefs-page-body");
    page.append(&label("Bluetooth", "prefs-group-title"));

    let status = label("", "error");
    status.set_visible(false);
    page.append(&status);

    let adapter = group();
    let power_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    power_row.add_css_class("prefs-row");
    let power_info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    power_info.set_hexpand(true);
    power_info.append(&label("Bluetooth", "prefs-row-title"));
    power_info.append(&label(
        "Enable or disable Bluetooth for this computer",
        "prefs-muted",
    ));
    power_row.append(&power_info);
    let powered = gtk::Switch::builder().valign(gtk::Align::Center).build();
    powered.set_sensitive(false);
    power_row.append(&powered);
    adapter.append(&power_row);

    let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    name_row.add_css_class("prefs-row");
    name_row.add_css_class("last");
    let name_info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    name_info.set_hexpand(true);
    name_info.append(&label("Visible as", "prefs-row-title"));
    name_info.append(&label(
        "Visible to nearby devices while this page is open",
        "prefs-muted",
    ));
    name_row.append(&name_info);
    let alias = gtk::Entry::builder()
        .width_chars(18)
        .sensitive(false)
        .build();
    name_row.append(&alias);
    let save_alias = gtk::Button::with_label("Save");
    save_alias.add_css_class("prefs-btn");
    save_alias.set_sensitive(false);
    name_row.append(&save_alias);
    adapter.append(&name_row);
    page.append(&adapter);

    page.append(&label("Devices", "prefs-group-title"));
    let devices = gtk::ListBox::new();
    devices.add_css_class("prefs-group");
    devices.add_css_class("bluetooth-device-list");
    devices.set_selection_mode(gtk::SelectionMode::None);
    devices.set_activate_on_single_click(true);
    let loading = gtk::ListBoxRow::new();
    let loading_content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    loading_content.add_css_class("prefs-row");
    loading_content.add_css_class("last");
    loading_content.append(&label("Looking for Bluetooth devices…", "prefs-muted"));
    loading.set_child(Some(&loading_content));
    devices.append(&loading);
    page.append(&devices);

    let state = Page::new(
        window,
        status,
        powered.clone(),
        alias.clone(),
        save_alias.clone(),
        devices.clone(),
    );
    powered.connect_active_notify({
        let state = Rc::downgrade(&state);
        move |switch| {
            if let Some(state) = state.upgrade() {
                state.set_power(switch.is_active());
            }
        }
    });
    save_alias.connect_clicked({
        let state = Rc::downgrade(&state);
        move |_| {
            if let Some(state) = state.upgrade() {
                state.save_alias();
            }
        }
    });
    alias.connect_changed({
        let state = Rc::downgrade(&state);
        move |_| {
            if let Some(state) = state.upgrade() {
                if !state.syncing.get() {
                    state.alias_dirty.set(true);
                }
            }
        }
    });
    devices.connect_row_activated({
        let state = Rc::downgrade(&state);
        move |_, row| {
            if let Some(state) = state.upgrade() {
                if let Some(device) = state.rendered.borrow().get(row.index() as usize).cloned() {
                    state.show_device(&device);
                }
            }
        }
    });
    page.connect_map({
        let state = Rc::downgrade(&state);
        move |_| {
            if let Some(state) = state.upgrade() {
                state.start();
            }
        }
    });
    state.start();
    let state = Rc::downgrade(&state);
    page.connect_unmap(move |_| {
        if let Some(state) = state.upgrade() {
            state.stop();
        }
    });

    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build()
}

#[cfg(test)]
mod tests {
    use super::{
        agent_path_parameters, decode_snapshot, device_state, device_type,
        register_agent_parameters, AdapterInfo, DeviceInfo, Snapshot, ADAPTER, AGENT_IFACE,
        AGENT_XML, DEVICE,
    };
    use gtk::gio;
    use gtk::glib::variant::{ObjectPath, ToVariant};
    use std::collections::HashMap;

    fn device(paired: bool, connected: bool) -> DeviceInfo {
        DeviceInfo {
            path: "/org/bluez/hci0/dev_00_11_22_33_44_55".into(),
            name: "Headphones".into(),
            address: "00:11:22:33:44:55".into(),
            icon: "audio-card".into(),
            appearance: 0,
            paired,
            connected,
        }
    }

    #[test]
    fn device_details_report_pairing_connection_and_type() {
        let connected = device(true, true);
        assert_eq!(device_state(&connected), "Connected");
        assert_eq!(device_type(&connected), "Audio");
        assert_eq!(device_state(&device(true, false)), "Disconnected");
        assert_eq!(device_state(&device(false, false)), "Not paired");
    }

    #[test]
    fn snapshot_keeps_adapter_and_device_metadata() {
        let snapshot = Snapshot {
            adapters: vec![AdapterInfo {
                path: "/org/bluez/hci0".into(),
                alias: "Palobar Host".into(),
                powered: true,
                discovering: true,
                discoverable: false,
                discoverable_timeout: 180,
            }],
            devices: vec![device(false, false)],
        };
        assert_eq!(snapshot.adapters[0].alias, "Palobar Host");
        assert!(snapshot.adapters[0].powered);
        assert_eq!(snapshot.devices[0].address, "00:11:22:33:44:55");
    }

    #[test]
    fn decodes_bluez_managed_objects() {
        let adapter_path = ObjectPath::try_from("/org/bluez/hci0").unwrap();
        let device_path = ObjectPath::try_from("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
        let adapter = HashMap::from([
            ("Alias".to_owned(), "Palobar Host".to_variant()),
            ("Powered".to_owned(), true.to_variant()),
            ("Discovering".to_owned(), true.to_variant()),
        ]);
        let device = HashMap::from([
            ("Alias".to_owned(), "Headphones".to_variant()),
            ("Address".to_owned(), "00:11:22:33:44:55".to_variant()),
            ("Icon".to_owned(), "audio-card".to_variant()),
            ("Paired".to_owned(), true.to_variant()),
            ("Connected".to_owned(), false.to_variant()),
        ]);
        let objects = HashMap::from([
            (adapter_path, HashMap::from([(ADAPTER.to_owned(), adapter)])),
            (device_path, HashMap::from([(DEVICE.to_owned(), device)])),
        ]);
        let snapshot = decode_snapshot((objects,).to_variant()).unwrap();
        assert_eq!(snapshot.adapters[0].alias, "Palobar Host");
        assert!(snapshot.adapters[0].powered);
        assert_eq!(snapshot.devices[0].name, "Headphones");
        assert!(snapshot.devices[0].paired);
        assert!(!snapshot.devices[0].connected);
        assert_eq!(device_type(&snapshot.devices[0]), "Audio");
    }

    #[test]
    fn bluetooth_agent_interface_is_valid() {
        let node = gio::DBusNodeInfo::for_xml(AGENT_XML).unwrap();
        assert!(node.lookup_interface(AGENT_IFACE).is_some());
    }

    #[test]
    fn agent_manager_arguments_use_object_path_signatures() {
        assert_eq!(register_agent_parameters().type_().as_str(), "(os)");
        assert_eq!(agent_path_parameters().type_().as_str(), "(o)");
    }
}
