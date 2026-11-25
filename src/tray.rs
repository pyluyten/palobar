use gtk::glib::variant::{ObjectPath, ToVariant};
use gtk::prelude::*;
use gtk::{gdk, gio, glib, Box as GtkBox, Button, Image, Orientation, PopoverMenu};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const ITEM_NAME_PREFIX: &str = "org.kde.StatusNotifierItem-";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const WATCHER_XML: &str = r#"
<node><interface name="org.kde.StatusNotifierWatcher">
  <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
  <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
  <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
  <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
  <property name="ProtocolVersion" type="i" access="read"/>
  <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
  <signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
  <signal name="StatusNotifierHostRegistered"/>
</interface></node>"#;

type Properties = HashMap<String, glib::Variant>;

#[derive(Clone)]
struct Item {
    bus: gio::DBusConnection,
    name: String,
    path: String,
}

impl Item {
    fn new(bus: &gio::DBusConnection, address: &str) -> Option<Self> {
        let (name, path) = address
            .split_once('/')
            .map_or((address, ITEM_PATH), |(name, _)| {
                (name, &address[name.len()..])
            });
        (gio::dbus_is_name(name) && glib::Variant::is_object_path(path)).then(|| Self {
            bus: bus.clone(),
            name: name.into(),
            path: path.into(),
        })
    }

    fn send(&self, path: &str, interface: &str, method: &str, args: &glib::Variant) {
        self.bus.call(
            Some(&self.name),
            path,
            interface,
            method,
            Some(args),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
            gio::Cancellable::NONE,
            |_| {},
        );
    }

    fn activate(&self, method: &str) {
        self.send(&self.path, ITEM, method, &(0i32, 0i32).to_variant());
    }
}

struct ItemView {
    button: Button,
    image: Image,
    menu: Rc<RefCell<Option<PopoverMenu>>>,
    properties: Properties,
    target: Rc<RefCell<Option<ObjectPath>>>,
    menu_only: Rc<Cell<bool>>,
}

impl Drop for ItemView {
    fn drop(&mut self) {
        if let Some(menu) = self.menu.borrow_mut().take() {
            menu.unparent();
        }
        if let Some(parent) = self.button.parent().and_downcast::<GtkBox>() {
            parent.remove(&self.button);
        }
    }
}

impl ItemView {
    fn new(item: Item) -> Self {
        let image = Image::new();
        image.set_pixel_size(15);
        let button = Button::builder().child(&image).build();
        button.add_css_class("panel-button");
        button.add_css_class("tray-item");
        let menu = Rc::new(RefCell::new(None::<PopoverMenu>));
        let target = Rc::new(RefCell::new(None::<ObjectPath>));
        let menu_only = Rc::new(Cell::new(false));
        let pending = Rc::new(Cell::new(false));
        let open_menu = {
            let item = item.clone();
            let button = button.downgrade();
            let menu = Rc::downgrade(&menu);
            let target = target.clone();
            move || {
                let Some(path) = target.borrow().clone() else {
                    item.activate("ContextMenu");
                    return;
                };
                if pending.replace(true) {
                    return;
                }
                let (item, button, menu, pending, target) = (
                    item.clone(),
                    button.clone(),
                    menu.clone(),
                    pending.clone(),
                    target.clone(),
                );
                glib::spawn_future_local(async move {
                    let model = load_menu(&item, &path).await;
                    pending.set(false);
                    if target.borrow().as_ref() != Some(&path) {
                        return;
                    }
                    if let (Some(button), Some(menu), Some(model)) =
                        (button.upgrade(), menu.upgrade(), model)
                    {
                        let mut menu = menu.borrow_mut();
                        if let Some(old) = menu.take() {
                            old.unparent();
                        }
                        let popover = PopoverMenu::from_model(Some(&model));
                        let actions = gio::SimpleActionGroup::new();
                        let click = gio::SimpleAction::new("click", Some(glib::VariantTy::INT32));
                        click.connect_activate(move |_, id| {
                            if let Some(id) = id.and_then(glib::Variant::get::<i32>) {
                                item.send(
                                    &path,
                                    MENU,
                                    "Event",
                                    &(id, "clicked", 0i32.to_variant(), 0u32).to_variant(),
                                );
                            }
                        });
                        actions.add_action(&click);
                        popover.insert_action_group("tray", Some(&actions));
                        popover.set_position(gtk::PositionType::Bottom);
                        popover.set_parent(&button);
                        popover.popup();
                        *menu = Some(popover);
                    }
                });
            }
        };
        button.connect_clicked({
            let item = item.clone();
            let open_menu = open_menu.clone();
            let menu_only = menu_only.clone();
            move |_| {
                if menu_only.get() {
                    open_menu();
                } else {
                    item.activate("Activate");
                }
            }
        });
        let right = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        right.connect_pressed(move |_, _, _, _| open_menu());
        button.add_controller(right);
        let middle = gtk::GestureClick::builder()
            .button(gdk::BUTTON_MIDDLE)
            .build();
        middle.connect_pressed(move |_, _, _, _| item.activate("SecondaryActivate"));
        button.add_controller(middle);
        Self {
            button,
            image,
            menu,
            properties: Properties::new(),
            target,
            menu_only,
        }
    }

    fn update(&mut self, properties: Properties) {
        if self.properties == properties {
            return;
        }
        self.button.set_tooltip_text(
            properties
                .get("ToolTip")
                .filter(|value| value.type_().as_str() == "(sa(iiay)ss)")
                .and_then(|value| value.child_value(2).get::<String>())
                .filter(|title| !title.is_empty())
                .as_deref()
                .or_else(|| text(&properties, "Title"))
                .or(Some("Tray Item")),
        );
        let attention = text(&properties, "Status") == Some("NeedsAttention");
        if !attention
            || !set_icon(
                &self.image,
                &properties,
                "AttentionIconName",
                "AttentionIconPixmap",
            )
        {
            if !set_icon(&self.image, &properties, "IconName", "IconPixmap") {
                self.image
                    .set_icon_name(Some("application-x-executable-symbolic"));
            }
        }
        self.menu_only.set(
            properties
                .get("ItemIsMenu")
                .and_then(glib::Variant::get)
                .unwrap_or(false),
        );
        let path = properties
            .get("Menu")
            .and_then(glib::Variant::get::<ObjectPath>)
            .filter(|path| path.as_str() != "/NO_DBUSMENU" && path.as_str() != "/");
        if *self.target.borrow() != path {
            if let Some(menu) = self.menu.borrow_mut().take() {
                menu.unparent();
            }
            *self.target.borrow_mut() = path;
        }
        self.properties = properties;
    }
}

fn text<'a>(properties: &'a Properties, key: &str) -> Option<&'a str> {
    properties
        .get(key)
        .and_then(glib::Variant::str)
        .filter(|value| !value.is_empty())
}

fn set_icon(image: &Image, properties: &Properties, name: &str, pixmaps: &str) -> bool {
    if let Some(name) = text(properties, name) {
        let theme = gtk::IconTheme::for_display(&image.display());
        if theme.has_icon(name) {
            image.set_icon_name(Some(name));
            return true;
        }
        if let Some(path) = text(properties, "IconThemePath") {
            let theme = gtk::IconTheme::new();
            theme.set_search_path(&[std::path::Path::new(path)]);
            theme.set_theme_name(Some("hicolor"));
            if theme.has_icon(name) {
                let icon = theme.lookup_icon(
                    name,
                    &[],
                    15,
                    image.scale_factor(),
                    gtk::TextDirection::None,
                    gtk::IconLookupFlags::empty(),
                );
                image.set_paintable(Some(&icon));
                return true;
            }
        }
    }
    let Some(pixmaps) = properties
        .get(pixmaps)
        .filter(|value| value.type_().as_str() == "a(iiay)")
    else {
        return false;
    };
    let Some((width, height, mut bytes)) = pixmaps
        .iter()
        .filter_map(|entry| {
            let width = entry.child_get::<i32>(0);
            let height = entry.child_get::<i32>(1);
            let data = entry.child_value(2);
            (width > 0
                && width <= 512
                && height > 0
                && height <= 512
                && data.n_children() == (width * height * 4) as usize)
                .then_some((width, height, data))
        })
        .min_by_key(|(width, height, _)| (width.max(height) - 30).abs())
        .and_then(|(width, height, data)| {
            Some((width, height, data.fixed_array::<u8>().ok()?.to_vec()))
        })
    else {
        return false;
    };
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.rotate_left(1);
    }
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_bytes(
        &glib::Bytes::from_owned(bytes),
        gtk::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        width,
        height,
        width * 4,
    );
    let size = 15;
    let longest = width.max(height);
    let Some(pixbuf) = pixbuf.scale_simple(
        (width * size / longest).max(1),
        (height * size / longest).max(1),
        gtk::gdk_pixbuf::InterpType::Bilinear,
    ) else {
        return false;
    };
    image.set_from_pixbuf(Some(&pixbuf));
    true
}

async fn load_menu(item: &Item, path: &str) -> Option<gio::Menu> {
    let _ = item
        .bus
        .call_future(
            Some(&item.name),
            path,
            MENU,
            "AboutToShow",
            Some(&(0i32,).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
        )
        .await;
    let value = item
        .bus
        .call_future(
            Some(&item.name),
            path,
            MENU,
            "GetLayout",
            Some(&(0i32, 8i32, vec!["label", "enabled", "visible", "type"]).to_variant()),
            Some(glib::VariantTy::new("(u(ia{sv}av))").unwrap()),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
        )
        .await
        .ok()?;
    let model = gio::Menu::new();
    append_menu(&model, &value.child_value(1), 0, &mut 512);
    Some(model)
}

fn append_menu(menu: &gio::Menu, node: &glib::Variant, depth: usize, budget: &mut usize) {
    if depth > 8 {
        return;
    }
    let mut section = gio::Menu::new();
    for child in node.child_value(2).iter() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Some(child) = child
            .as_variant()
            .filter(|value| value.type_().as_str() == "(ia{sv}av)")
        else {
            continue;
        };
        let id = child.child_get::<i32>(0);
        let properties = child.child_get::<Properties>(1);
        if properties.get("visible").and_then(glib::Variant::get) == Some(false) {
            continue;
        }
        if text(&properties, "type") == Some("separator") {
            if section.n_items() != 0 {
                menu.append_section(None, &section);
                section = gio::Menu::new();
            }
        } else if let Some(label) = text(&properties, "label") {
            if child.child_value(2).n_children() > 0 {
                let submenu = gio::Menu::new();
                append_menu(&submenu, &child, depth + 1, budget);
                section.append_submenu(Some(label), &submenu);
            } else {
                let entry = gio::MenuItem::new(Some(label), None);
                if properties.get("enabled").and_then(glib::Variant::get) != Some(false) {
                    entry.set_action_and_target_value(Some("tray.click"), Some(&id.to_variant()));
                }
                section.append_item(&entry);
            }
        }
    }
    if section.n_items() != 0 {
        menu.append_section(None, &section);
    }
}

struct View {
    container: glib::WeakRef<GtkBox>,
    items: HashMap<String, ItemView>,
}

pub struct Tray {
    views: Rc<RefCell<Vec<View>>>,
}

impl Tray {
    pub fn widget(&self) -> GtkBox {
        let container = GtkBox::new(Orientation::Horizontal, 2);
        container.set_visible(false);
        self.views.borrow_mut().push(View {
            container: container.downgrade(),
            items: HashMap::new(),
        });
        container
    }
}

pub fn start(app: &gtk::Application) -> Result<Tray, glib::Error> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    let node = gio::DBusNodeInfo::for_xml(WATCHER_XML)?;
    let items = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let object = bus
        .register_object(WATCHER_PATH, &node.lookup_interface(WATCHER).unwrap())
        .property({
            let items = items.clone();
            move |_, _, _, _, property| match property {
                "RegisteredStatusNotifierItems" => items
                    .borrow()
                    .iter()
                    .map(|(address, _)| address.as_str())
                    .collect::<Vec<_>>()
                    .to_variant(),
                "IsStatusNotifierHostRegistered" => true.to_variant(),
                _ => 0i32.to_variant(),
            }
        })
        .method_call({
            let items = items.clone();
            move |bus, sender, _, _, method, parameters, call| {
                if method == "RegisterStatusNotifierHost" {
                    let _ = bus.emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER,
                        "StatusNotifierHostRegistered",
                        None,
                    );
                    call.return_value(None);
                    return;
                }
                let service = parameters.child_get::<String>(0);
                let address = if service.starts_with('/') {
                    format!("{}{}", sender.unwrap_or_default(), service)
                } else {
                    service
                };
                let Some(item) = Item::new(&bus, &address) else {
                    call.return_dbus_error(
                        "org.freedesktop.DBus.Error.InvalidArgs",
                        "Invalid tray item address",
                    );
                    return;
                };
                if !items
                    .borrow()
                    .iter()
                    .any(|(existing, _)| existing == &address)
                {
                    let owner = bus
                        .call_sync(
                            Some(BUS),
                            BUS_PATH,
                            BUS,
                            "GetNameOwner",
                            Some(&(item.name.as_str(),).to_variant()),
                            None,
                            gio::DBusCallFlags::NONE,
                            1000,
                            gio::Cancellable::NONE,
                        )
                        .ok()
                        .and_then(|value| value.get::<(String,)>());
                    let Some((owner,)) = owner else {
                        call.return_dbus_error(
                            "org.freedesktop.DBus.Error.NameHasNoOwner",
                            "Tray item is unavailable",
                        );
                        return;
                    };
                    items.borrow_mut().push((address.clone(), owner));
                    let _ = bus.emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER,
                        "StatusNotifierItemRegistered",
                        Some(&(address,).to_variant()),
                    );
                }
                call.return_value(None);
            }
        })
        .build()?;
    let named_items = Rc::new(RefCell::new(HashSet::new()));
    let changes = bus.subscribe_to_signal(
        Some(BUS),
        Some(BUS),
        Some("NameOwnerChanged"),
        Some(BUS_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        {
            let items = items.clone();
            let named_items = named_items.clone();
            move |signal| {
                let (name, old, new): (String, String, String) = signal.parameters.get().unwrap();
                if name.starts_with(ITEM_NAME_PREFIX) {
                    let mut named_items = named_items.borrow_mut();
                    if new.is_empty() {
                        named_items.remove(&name);
                        named_items.shrink_to_fit();
                    } else {
                        named_items.insert(name.clone());
                    }
                }
                if !old.is_empty() {
                    let mut items = items.borrow_mut();
                    let count = items.len();
                    items.retain(|(address, owner)| {
                        let service = address.split('/').next().unwrap_or_default();
                        if name == *owner || name == service {
                            let _ = signal.connection.emit_signal(
                                None,
                                WATCHER_PATH,
                                WATCHER,
                                "StatusNotifierItemUnregistered",
                                Some(&(address,).to_variant()),
                            );
                            false
                        } else {
                            true
                        }
                    });
                    if items.len() != count {
                        items.shrink_to_fit();
                    }
                }
            }
        },
    );
    // Qt applications can export their item before a watcher is available.
    if let Ok(names) = bus.call_sync(
        Some(BUS),
        BUS_PATH,
        BUS,
        "ListNames",
        None,
        None,
        gio::DBusCallFlags::NONE,
        1000,
        gio::Cancellable::NONE,
    ) {
        if let Some((names,)) = names.get::<(Vec<String>,)>() {
            named_items.borrow_mut().extend(
                names
                    .into_iter()
                    .filter(|name| name.starts_with(ITEM_NAME_PREFIX)),
            );
        }
    }
    let watcher = gio::bus_own_name_on_connection(
        &bus,
        WATCHER,
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, _| {},
    );
    let host_name = format!("org.kde.StatusNotifierHost-{}", std::process::id());
    let host = gio::bus_own_name_on_connection(
        &bus,
        &host_name,
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, _| {},
    );
    let views = Rc::new(RefCell::new(Vec::<View>::new()));
    let refresh = glib::spawn_future_local(refresh_views(
        bus.clone(),
        host_name,
        views.clone(),
        named_items,
    ));
    let resources = RefCell::new(Some((object, changes, watcher, host, refresh)));
    app.connect_shutdown(move |_| {
        if let Some((object, changes, watcher, host, refresh)) = resources.borrow_mut().take() {
            refresh.abort();
            gio::bus_unown_name(host);
            gio::bus_unown_name(watcher);
            drop(changes);
            let _ = bus.unregister_object(object);
        }
    });
    Ok(Tray { views })
}

async fn refresh_views(
    bus: gio::DBusConnection,
    host_name: String,
    views: Rc<RefCell<Vec<View>>>,
    named_items: Rc<RefCell<HashSet<String>>>,
) {
    let local_owner = bus.unique_name();
    let mut previous_owner = None;
    let mut host_registered = false;
    loop {
        let owner = bus
            .call_future(
                Some(BUS),
                BUS_PATH,
                BUS,
                "GetNameOwner",
                Some(&(WATCHER,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                1000,
            )
            .await
            .ok()
            .and_then(|value| value.get::<(String,)>())
            .map(|(owner,)| owner);
        if owner != previous_owner {
            host_registered = false;
            for view in views.borrow_mut().iter_mut() {
                view.items.clear();
            }
            previous_owner = owner.clone();
        }
        if !host_registered {
            if let Some(owner) = &owner {
                host_registered = bus
                    .call_future(
                        Some(owner),
                        WATCHER_PATH,
                        WATCHER,
                        "RegisterStatusNotifierHost",
                        Some(&(host_name.as_str(),).to_variant()),
                        None,
                        gio::DBusCallFlags::NO_AUTO_START,
                        1000,
                    )
                    .await
                    .is_ok();
            }
        }
        let mut addresses = if let Some(owner) = &owner {
            bus.call_future(
                Some(owner),
                WATCHER_PATH,
                PROPERTIES,
                "Get",
                Some(&(WATCHER, "RegisteredStatusNotifierItems").to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1000,
            )
            .await
            .ok()
            .and_then(|value| value.get::<(glib::Variant,)>())
            .and_then(|(value,)| value.get::<Vec<String>>())
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        if owner.is_some() && owner.as_deref() == local_owner.as_deref() {
            addresses.extend(named_items.borrow().iter().cloned());
        }
        let mut seen = HashSet::new();
        for address in &addresses {
            let Some(item) = Item::new(&bus, address) else {
                continue;
            };
            let address = format!("{}{}", item.name, item.path);
            if !seen.insert(address.clone()) {
                continue;
            }
            let properties = bus
                .call_future(
                    Some(&item.name),
                    &item.path,
                    PROPERTIES,
                    "GetAll",
                    Some(&(ITEM,).to_variant()),
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    1000,
                )
                .await
                .ok()
                .and_then(|value| value.get::<(Properties,)>());
            let Some((properties,)) = properties.filter(|(properties,)| !properties.is_empty())
            else {
                seen.remove(address.as_str());
                continue;
            };
            for view in views.borrow_mut().iter_mut() {
                let Some(container) = view.container.upgrade() else {
                    continue;
                };
                let entry = view.items.entry(address.clone()).or_insert_with(|| {
                    let entry = ItemView::new(item.clone());
                    container.append(&entry.button);
                    entry
                });
                entry.update(properties.clone());
            }
        }
        views.borrow_mut().retain_mut(|view| {
            let Some(container) = view.container.upgrade() else {
                return false;
            };
            view.items
                .retain(|address, _| seen.contains(address.as_str()));
            view.items.shrink_to_fit();
            container.set_visible(!view.items.is_empty());
            true
        });
        glib::timeout_future(Duration::from_secs(2)).await;
    }
}
