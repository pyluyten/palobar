import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { createState, For } from "ags";
import { execAsync } from "ags/process";
import GLib from "gi://GLib";
import AstalWp from "gi://AstalWp";
import AstalNetwork from "gi://AstalNetwork";
import AstalNotifd from "gi://AstalNotifd";
import { popups } from "../services/popups";
import { lockScreen } from "../services/lockscreen";
import { canSetAppearance, isDarkStyle, setDarkStyle, subscribeAppearance } from "../services/appearance";

function runSessionCmd(cmd: string[]) {
  popups.close();
  execAsync(cmd).catch((err) =>
    console.error("[session] command failed:", cmd.join(" "), err),
  );
}

function VolumeSection() {
  const wp = AstalWp.get_default();
  const speaker = wp?.audio.defaultSpeaker;

  const [volume, setVolume] = createState(speaker?.volume ?? 0.5);
  const [muted, setMuted] = createState(speaker?.mute ?? false);
  const [icon, setIcon] = createState(
    speaker?.volumeIcon ?? "audio-volume-medium-symbolic",
  );

  if (speaker) {
    speaker.connect("notify::volume", () => setVolume(speaker.volume));
    speaker.connect("notify::mute", () => setMuted(speaker.mute));
    speaker.connect("notify::volume-icon", () => setIcon(speaker.volumeIcon));
  }

  return (
    <box class="qs-row qs-volume" spacing={10}>
      <button
        class={`qs-icon-btn ${muted((m) => (m ? "muted" : ""))}`}
        tooltipText={muted((m) => (m ? "Unmute" : "Mute"))}
        onClicked={() => {
          if (speaker) speaker.mute = !speaker.mute;
        }}
      >
        <image iconName={icon} />
      </button>

      <slider
        class="qs-slider"
        hexpand
        min={0}
        max={1}
        value={volume}
        onChangeValue={({ value }) => {
          if (speaker) {
            speaker.volume = value;
            if (speaker.mute && value > 0) speaker.mute = false;
          }
        }}
      />

      <label
        class="qs-volume-pct"
        label={volume((v) => `${Math.round(v * 100)}%`)}
      />
    </box>
  );
}

function NetworkSection() {
  const network = AstalNetwork.get_default();
  const wifi = network?.wifi;

  const [wifiEnabled, setWifiEnabled] = createState(wifi?.enabled ?? false);
  const [ssid, setSsid] = createState(wifi?.ssid ?? "Disconnected");
  const [netIcon, setNetIcon] = createState(
    wifi?.iconName ?? "network-wireless-offline-symbolic",
  );
  const [showWifiList, setShowWifiList] = createState(false);
  const [aps, setAps] = createState<AstalNetwork.AccessPoint[]>([]);

  const syncWifi = () => {
    if (!wifi) return;
    setWifiEnabled(wifi.enabled);
    setSsid(wifi.ssid || (wifi.enabled ? "Not Connected" : "Wi-Fi Off"));
    setNetIcon(wifi.iconName || "network-wireless-offline-symbolic");
    const unique = new Map<string, AstalNetwork.AccessPoint>();
    for (const ap of wifi.accessPoints) {
      if (!ap.ssid) continue;
      const prev = unique.get(ap.ssid);
      if (!prev || ap.strength > prev.strength) {
        unique.set(ap.ssid, ap);
      }
    }
    setAps(
      [...unique.values()].sort((a, b) => b.strength - a.strength).slice(0, 8),
    );
  };

  if (wifi) {
    syncWifi();
    wifi.connect("notify::enabled", syncWifi);
    wifi.connect("notify::ssid", syncWifi);
    wifi.connect("notify::icon-name", syncWifi);
    wifi.connect("notify::access-points", syncWifi);
  }

  return (
    <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
      <box class="qs-toggles" spacing={8}>
        <button
          class={`qs-toggle-pill ${wifiEnabled((e) => (e ? "active" : ""))}`}
          hexpand
          onClicked={() => {
            if (wifi) wifi.enabled = !wifi.enabled;
          }}
        >
          <box spacing={10} halign={Gtk.Align.START}>
            <image iconName={netIcon} />
            <box orientation={Gtk.Orientation.VERTICAL} spacing={1}>
              <label
                class="qs-toggle-title"
                halign={Gtk.Align.START}
                label="Wi-Fi"
              />
              <label
                class="qs-toggle-subtitle"
                halign={Gtk.Align.START}
                ellipsize={3}
                label={ssid}
              />
            </box>
          </box>
        </button>

        <button
          class={`qs-chevron-btn ${showWifiList((s) => (s ? "active" : ""))}`}
          tooltipText="Select Wi-Fi Network"
          onClicked={() => {
            if (wifi && !showWifiList.get()) {
              wifi.scan();
            }
            setShowWifiList((s) => !s);
          }}
        >
          <image iconName="pan-down-symbolic" />
        </button>
      </box>

      <box
        class="qs-submenu"
        orientation={Gtk.Orientation.VERTICAL}
        spacing={4}
        visible={showWifiList}
      >
        <For each={aps}>
          {(ap) => (
            <button
              class={`qs-ap-item ${ap.ssid === wifi?.ssid ? "connected" : ""}`}
              onClicked={() => {
                execAsync(["nmcli", "device", "wifi", "connect", ap.ssid]).catch(
                  (err) => console.error("[wifi] connect error:", err),
                );
              }}
            >
              <box spacing={8}>
                <image iconName={ap.iconName} />
                <label hexpand halign={Gtk.Align.START} label={ap.ssid} />
                <label
                  class="qs-ap-strength"
                  label={`${ap.strength}%`}
                />
              </box>
            </button>
          )}
        </For>
      </box>
    </box>
  );
}

function QuickTogglesSection() {
  const notifd = AstalNotifd.get_default();
  const [dnd, setDnd] = createState(notifd.dontDisturb);
  notifd.connect("notify::dont-disturb", () => setDnd(notifd.dontDisturb));

  const [darkMode, setDarkMode] = createState(isDarkStyle());
  subscribeAppearance("color-scheme", () => setDarkMode(isDarkStyle()));

  return (
    <box class="qs-mini-toggles" spacing={8} homogeneous>
      <button
        class={darkMode((dark) => dark ? "qs-mini-pill active" : "qs-mini-pill")}
        sensitive={canSetAppearance("color-scheme")}
        onClicked={() => setDarkStyle(!isDarkStyle())}
      >
        <box spacing={8} halign={Gtk.Align.START}>
          <image iconName="weather-clear-night-symbolic" />
          <label label="Dark Style" />
        </box>
      </button>

      <button
        class={`qs-mini-pill ${dnd((d) => (d ? "active" : ""))}`}
        onClicked={() => {
          notifd.dontDisturb = !notifd.dontDisturb;
        }}
      >
        <box spacing={8} halign={Gtk.Align.START}>
          <image
            iconName={dnd((d) =>
              d
                ? "notifications-disabled-symbolic"
                : "preferences-system-notifications-symbolic",
            )}
          />
          <label label="Do Not Disturb" />
        </box>
      </button>
    </box>
  );
}

export function QuickSettings(gdkmonitor: Gdk.Monitor) {
  const { TOP, RIGHT } = Astal.WindowAnchor;
  const [showPowerMenu, setShowPowerMenu] = createState(false);

  return (
    <window
      name="quick-settings-popup"
      class="popup-window"
      gdkmonitor={gdkmonitor}
      anchor={TOP | RIGHT}
      marginTop={6}
      marginRight={10}
      keymode={Astal.Keymode.ON_DEMAND}
      visible={popups.active((p) => p === "quick-settings")}
      application={app}
      $={(self) => {
        const keyCtrl = new Gtk.EventControllerKey();
        keyCtrl.connect("key-pressed", (_ctrl, keyval) => {
          if (keyval === Gdk.KEY_Escape) {
            popups.close();
            return true;
          }
          return false;
        });
        self.add_controller(keyCtrl);
      }}
    >
      <box
        class="popup-card qs-card"
        orientation={Gtk.Orientation.VERTICAL}
        spacing={14}
      >
        {/* Header: Screenshot & Settings on left, Lock & Power on right */}
        <box class="qs-header" spacing={8}>
          <button
            class="qs-circle-btn"
            tooltipText="Screenshot"
            onClicked={() => {
              popups.close();
              GLib.timeout_add(GLib.PRIORITY_DEFAULT, 150, () => {
                execAsync(["niri", "msg", "action", "screenshot"]).catch(
                  (err) => console.error("[screenshot] failed:", err),
                );
                return GLib.SOURCE_REMOVE;
              });
            }}
          >
            <image iconName="applets-screenshooter-symbolic" />
          </button>

          <button
            class="qs-circle-btn"
            tooltipText="Preferences"
            onClicked={() => {
              popups.close();
              const bundledPrefs = GLib.find_program_in_path("palobar-prefs");
              if (bundledPrefs) {
                execAsync([bundledPrefs]).catch((err) =>
                  console.error("[prefs] failed to launch:", err),
                );
                return;
              }
              const prefsPath = GLib.file_test("prefs.ts", GLib.FileTest.EXISTS)
                ? "prefs.ts"
                : "src/prefs.ts";
              execAsync(["ags", "run", prefsPath]).catch((err) =>
                console.error("[prefs] failed to launch:", err),
              );
            }}
          >
            <image iconName="preferences-system-symbolic" />
          </button>

          <box hexpand />

          <button
            class="qs-circle-btn"
            tooltipText="Lock Screen"
            onClicked={() => {
              popups.close();
              lockScreen();
            }}
          >
            <image iconName="system-lock-screen-symbolic" />
          </button>

          <button
            class={`qs-circle-btn ${showPowerMenu((s) => (s ? "active" : ""))}`}
            tooltipText="Power Options"
            onClicked={() => setShowPowerMenu((s) => !s)}
          >
            <image iconName="system-shutdown-symbolic" />
          </button>
        </box>

        {/* Expandable Power Menu */}
        <box
          class="qs-submenu qs-power-menu"
          orientation={Gtk.Orientation.VERTICAL}
          spacing={4}
          visible={showPowerMenu}
        >
          <button
            class="qs-power-item"
            onClicked={() =>
              runSessionCmd(["niri", "msg", "action", "quit", "--skip-confirmation"])
            }
          >
            <box spacing={10}>
              <image iconName="system-log-out-symbolic" />
              <label label="Log Out" />
            </box>
          </button>
          <button
            class="qs-power-item"
            onClicked={() => runSessionCmd(["systemctl", "suspend"])}
          >
            <box spacing={10}>
              <image iconName="media-playback-pause-symbolic" />
              <label label="Suspend" />
            </box>
          </button>
          <button
            class="qs-power-item"
            onClicked={() => runSessionCmd(["systemctl", "reboot"])}
          >
            <box spacing={10}>
              <image iconName="system-reboot-symbolic" />
              <label label="Restart" />
            </box>
          </button>
          <button
            class="qs-power-item danger"
            onClicked={() => runSessionCmd(["systemctl", "poweroff"])}
          >
            <box spacing={10}>
              <image iconName="system-shutdown-symbolic" />
              <label label="Power Off" />
            </box>
          </button>
        </box>

        <VolumeSection />
        <NetworkSection />
        <QuickTogglesSection />
      </box>
    </window>
  );
}
