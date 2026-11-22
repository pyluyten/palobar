import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { createPoll } from "ags/time";
import { createState, For } from "ags";
import AstalWp from "gi://AstalWp";
import AstalNetwork from "gi://AstalNetwork";
import { niri, type NiriWorkspace } from "../services/niri";
import { popups } from "../services/popups";
import { unreadNotificationCount } from "../services/unread-notifications";
import { SysTray } from "./Tray";

function formatGnomeDate(): string {
  const now = new Date();
  const datePart = new Intl.DateTimeFormat("en-US", {
    weekday: "short",
    month: "short",
    day: "numeric",
  }).format(now);
  const timePart = new Intl.DateTimeFormat("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(now);
  return `${datePart.replace(",", "")}  ${timePart}`;
}

function Workspaces() {
  const [workspaces, setWorkspaces] = createState<NiriWorkspace[]>(
    niri.getWorkspaces(),
  );

  niri.subscribe((list) => setWorkspaces(list));

  return (
    <box class="workspaces" spacing={2} valign={Gtk.Align.CENTER}>
      <For each={workspaces}>
        {(ws) => (
          <button
            class={`workspace-pill ${ws.is_focused ? "focused" : ""} ${ws.is_active ? "active" : ""} ${ws.is_urgent ? "urgent" : ""}`}
            valign={Gtk.Align.CENTER}
            onClicked={() => niri.focusWorkspace(ws.idx)}
            tooltipText={ws.name ?? `Workspace ${ws.idx}`}
          >
            <box
              class="workspace-dot"
              valign={Gtk.Align.CENTER}
              halign={Gtk.Align.CENTER}
            />
          </button>
        )}
      </For>
    </box>
  );
}

function SystemIndicators() {
  const wp = AstalWp.get_default();
  const speaker = wp?.audio.defaultSpeaker;
  const network = AstalNetwork.get_default();

  const [volIcon, setVolIcon] = createState(
    speaker?.volumeIcon ?? "audio-volume-medium-symbolic",
  );
  const [netIcon, setNetIcon] = createState(
    network?.wifi?.iconName ??
      network?.wired?.iconName ??
      "network-offline-symbolic",
  );

  if (speaker) {
    speaker.connect("notify::volume-icon", () => setVolIcon(speaker.volumeIcon));
  }

  const syncNetworkIcon = () => {
    if (!network) return;
    if (network.primary === AstalNetwork.Primary.WIRED && network.wired) {
      setNetIcon(network.wired.iconName);
    } else if (network.wifi) {
      setNetIcon(network.wifi.iconName);
    } else {
      setNetIcon("network-offline-symbolic");
    }
  };

  if (network) {
    syncNetworkIcon();
    network.connect("notify::primary", syncNetworkIcon);
    network.wifi?.connect("notify::icon-name", syncNetworkIcon);
    network.wired?.connect("notify::icon-name", syncNetworkIcon);
  }

  return (
    <box spacing={8}>
      <image iconName={netIcon} />
      <image iconName={volIcon} />
      <image iconName="system-shutdown-symbolic" />
    </box>
  );
}

function NotificationBadge() {
  return (
    <label
      class="notif-badge"
      visible={unreadNotificationCount((count) => count > 0)}
      label={unreadNotificationCount(String)}
    />
  );
}

export function Bar(gdkmonitor: Gdk.Monitor, index: number) {
  const date = createPoll(formatGnomeDate(), 10000, formatGnomeDate);
  const { TOP, LEFT, RIGHT } = Astal.WindowAnchor;

  return (
    <window
      visible
      name={`palobar-panel-${index}`}
      class="panel-window"
      gdkmonitor={gdkmonitor}
      exclusivity={Astal.Exclusivity.EXCLUSIVE}
      anchor={TOP | LEFT | RIGHT}
      application={app}
    >
      <centerbox class="panel">
        {/* Left: Workspaces */}
        <box $type="start" class="panel-left" spacing={8}>
          <Workspaces />
        </box>

        {/* Center: Date & Time */}
        <button
          $type="center"
          class={`panel-button clock-button ${popups.active((p) => (p === "calendar" ? "open" : ""))}`}
          tooltipText="Calendar and Notifications"
          onClicked={() => popups.toggle("calendar")}
        >
          <box spacing={6}>
            <label label={date} />
            <NotificationBadge />
          </box>
        </button>

        {/* Right: SysTray + Clipboard + Network + Volume + Session */}
        <box $type="end" class="panel-right" spacing={4}>
          <SysTray />

          <button
            class={`panel-button clipboard-button ${popups.active((p) => (p === "clipboard" ? "open" : ""))}`}
            tooltipText="Clipboard History"
            onClicked={() => popups.toggle("clipboard")}
          >
            <image iconName="edit-paste-symbolic" />
          </button>

          <button
            class={`panel-button system-menu-button ${popups.active((p) => (p === "quick-settings" ? "open" : ""))}`}
            tooltipText="Quick Settings"
            onClicked={() => popups.toggle("quick-settings")}
          >
            <SystemIndicators />
          </button>
        </box>
      </centerbox>
    </window>
  );
}
