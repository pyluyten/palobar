import app from "ags/gtk4/app";
import { Astal, Gdk } from "ags/gtk4";
import { createPoll } from "ags/time";
import { createState, For } from "ags";
import { niri, type NiriWorkspace } from "../services/niri";

function formatGnomeDate(): string {
  const now = new Date();
  const formatted = new Intl.DateTimeFormat("fr-FR", {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(now);
  return formatted.replace(",", "");
}

function Workspaces() {
  const [workspaces, setWorkspaces] = createState<NiriWorkspace[]>(
    niri.getWorkspaces(),
  );

  niri.subscribe((list) => setWorkspaces(list));

  return (
    <box class="workspaces" spacing={4}>
      <For each={workspaces}>
        {(ws) => (
          <button
            class={`workspace-pill ${ws.is_focused ? "focused" : ""} ${ws.is_active ? "active" : ""} ${ws.is_urgent ? "urgent" : ""}`}
            onClicked={() => niri.focusWorkspace(ws.idx)}
            tooltipText={ws.name ?? `Workspace ${ws.idx}`}
          >
            <box class="workspace-dot" />
          </button>
        )}
      </For>
    </box>
  );
}

export function Bar(gdkmonitor: Gdk.Monitor) {
  const date = createPoll(formatGnomeDate(), 10000, formatGnomeDate);
  const { TOP, LEFT, RIGHT } = Astal.WindowAnchor;

  return (
    <window
      visible
      name="palobar-panel"
      class="panel-window"
      gdkmonitor={gdkmonitor}
      exclusivity={Astal.Exclusivity.EXCLUSIVE}
      anchor={TOP | LEFT | RIGHT}
      application={app}
    >
      <centerbox class="panel">
        {/* Gauche : Activités + Workspaces */}
        <box $type="start" class="panel-left" spacing={8}>
          <button
            class="panel-button activities-button"
            onClicked={() => {
              // TODO Jalon 3 : ouvrir le launcher plein écran Activités
              niri.toggleOverview();
            }}
          >
            <label label="Activités" />
          </button>
          <Workspaces />
        </box>

        {/* Centre : Date & Heure */}
        <box $type="center" class="panel-center">
          <button
            class="panel-button clock-button"
            onClicked={() => {
              // TODO Jalon 2 : ouvrir le popover Calendrier
              console.log("Toggle calendar popover");
            }}
          >
            <label label={date} />
          </button>
        </box>

        {/* Droite : Notification tray + Réseau + Volume + Session */}
        <box $type="end" class="panel-right" spacing={4}>
          <button
            class="panel-button notifications-button"
            tooltipText="Notifications"
            onClicked={() => {
              // TODO Jalon 2 : ouvrir le tiroir de notifications
              console.log("Toggle notification tray");
            }}
          >
            <image iconName="preferences-system-notifications-symbolic" />
          </button>

          <button
            class="panel-button system-menu-button"
            tooltipText="Réseau, Volume, Session"
            onClicked={() => {
              // TODO Jalon 2 : ouvrir Quick Settings (réseau, volume, session)
              console.log("Toggle quick settings");
            }}
          >
            <box spacing={8}>
              <image iconName="network-wireless-signal-excellent-symbolic" />
              <image iconName="audio-volume-high-symbolic" />
              <image iconName="system-shutdown-symbolic" />
            </box>
          </button>
        </box>
      </centerbox>
    </window>
  );
}
