use gtk::prelude::*;
use gtk::{gio, glib};
use std::collections::HashMap;
use std::time::Duration;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
type Properties = HashMap<String, glib::Variant>;

#[derive(PartialEq, Eq)]
struct Player {
    name: String,
    identity: String,
    title: String,
    artists: String,
    playing: bool,
    previous: bool,
    play: bool,
    next: bool,
}

async fn properties(bus: &gio::DBusConnection, name: &str, interface: &str) -> Option<Properties> {
    bus.call_future(
        Some(name),
        PATH,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        Some(&(interface,).to_variant()),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        750,
    )
    .await
    .ok()?
    .get::<(Properties,)>()
    .map(|(properties,)| properties)
}

fn property<T: glib::variant::FromVariant>(values: &Properties, key: &str) -> Option<T> {
    values.get(key)?.get()
}

async fn read_player(bus: &gio::DBusConnection, name: String) -> Option<Player> {
    let values = properties(bus, &name, PLAYER).await?;
    let metadata = property::<Properties>(&values, "Metadata").unwrap_or_default();
    let root = properties(bus, &name, ROOT).await.unwrap_or_default();
    let playing = property::<String>(&values, "PlaybackStatus").as_deref() == Some("Playing");
    let can = |key| property::<bool>(&values, key).unwrap_or(false);
    let control = can("CanControl");
    Some(Player {
        identity: property::<String>(&root, "Identity")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| name.trim_start_matches(PREFIX).to_owned()),
        name,
        title: property::<String>(&metadata, "xesam:title")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "No track information".into()),
        artists: property::<Vec<String>>(&metadata, "xesam:artist")
            .unwrap_or_default()
            .join(", "),
        playing,
        previous: control && can("CanGoPrevious"),
        play: control && can(if playing { "CanPause" } else { "CanPlay" }),
        next: control && can("CanGoNext"),
    })
}

async fn players(bus: &gio::DBusConnection) -> Option<Vec<Player>> {
    let (mut names,) = bus
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            None,
            gio::DBusCallFlags::NONE,
            750,
        )
        .await
        .ok()?
        .get::<(Vec<String>,)>()?;
    names.retain(|name| name.starts_with(PREFIX));
    names.sort_unstable();
    let mut players = Vec::new();
    for name in names {
        if let Some(player) = read_player(bus, name).await {
            players.push(player);
        }
    }
    Some(players)
}

fn player_widget(bus: &gio::DBusConnection, player: &Player) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 8);
    box_.add_css_class("date-media-card");
    let title = super::label(&player.title, "date-media-title");
    title.set_tooltip_text(Some(&player.title));
    box_.append(&title);
    let subtitle = if player.artists.is_empty() {
        player.identity.clone()
    } else {
        format!("{} · {}", player.artists, player.identity)
    };
    box_.append(&super::label(&subtitle, "date-muted"));
    let controls = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();
    let (action, tooltip, icon) = if player.playing {
        ("Pause", "Pause", "media-playback-pause-symbolic")
    } else {
        ("Play", "Play", "media-playback-start-symbolic")
    };
    for (method, tooltip, icon, enabled) in [
        (
            "Previous",
            "Previous Track",
            "media-skip-backward-symbolic",
            player.previous,
        ),
        (action, tooltip, icon, player.play),
        (
            "Next",
            "Next Track",
            "media-skip-forward-symbolic",
            player.next,
        ),
    ] {
        let button = super::icon_button(icon, tooltip);
        button.set_sensitive(enabled);
        let bus = bus.clone();
        let name = player.name.clone();
        button.connect_clicked(move |button| {
            button.set_sensitive(false);
            let button = button.downgrade();
            let bus = bus.clone();
            let name = name.clone();
            glib::spawn_future_local(async move {
                let result = bus
                    .call_future(
                        Some(&name),
                        PATH,
                        PLAYER,
                        method,
                        None,
                        None,
                        gio::DBusCallFlags::NO_AUTO_START,
                        1000,
                    )
                    .await;
                if let Err(error) = result {
                    eprintln!("[calendar] Media action {method} failed: {error}");
                }
                if let Some(button) = button.upgrade() {
                    button.set_sensitive(true);
                }
            });
        });
        controls.append(&button);
    }
    box_.append(&controls);
    box_
}

pub(super) fn section(feed: &gtk::Box) -> glib::JoinHandle<()> {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.append(&super::label("Media", "date-section-title"));
    let empty = super::label("No Media Playing", "date-muted");
    empty.add_css_class("date-media-card");
    section.append(&empty);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let scroll = super::scroll(&list, 174);
    scroll.set_visible(false);
    section.append(&scroll);
    feed.append(&section);
    glib::spawn_future_local(async move {
        let Ok(bus) = gio::bus_get_future(gio::BusType::Session).await else {
            return;
        };
        let mut rendered = Vec::new();
        loop {
            if let Some(next) = players(&bus).await {
                if next != rendered {
                    super::clear(&list);
                    for player in &next {
                        list.append(&player_widget(&bus, player));
                    }
                    empty.set_visible(next.is_empty());
                    scroll.set_visible(!next.is_empty());
                    rendered = next;
                }
            }
            glib::timeout_future(Duration::from_secs(1)).await;
        }
    })
}
