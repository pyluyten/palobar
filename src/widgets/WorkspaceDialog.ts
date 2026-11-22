import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import GioUnix from "gi://GioUnix";
import Graphene from "gi://Graphene";
import { niri, type NiriWindow, type NiriWorkspace } from "../services/niri";
import { popups } from "../services/popups";
import { scheduleGc } from "../services/runtime";

let dialog: Astal.Window | undefined;
let primaryMonitor: Gdk.Monitor | undefined;

export function displayWorkspaces(monitor = primaryMonitor) {
  if (!dialog || !monitor) return;
  if (dialog.get_gdkmonitor() !== monitor) dialog.set_gdkmonitor(monitor);
  popups.open("workspaces");
}

function windowIcon(window: NiriWindow, theme: Gtk.IconTheme) {
  const appId = window.app_id;
  const desktop = appId
    ? GioUnix.DesktopAppInfo.new(appId.endsWith(".desktop") ? appId : appId + ".desktop")
    : null;
  const icon = desktop?.get_icon();
  return icon
    ? new Gtk.Image({ gicon: icon, pixel_size: 36 })
    : new Gtk.Image({
        icon_name: appId && theme.has_icon(appId) ? appId : "application-x-executable",
        pixel_size: 36,
      });
}

export function WorkspaceDialog(monitor: Gdk.Monitor) {
  primaryMonitor = monitor;
  const rows = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 12 });
  const status = new Gtk.Label({ wrap: true });
  status.add_css_class("workspace-dialog-status");
  const list = new Gtk.ScrolledWindow({
    child: rows,
    hscrollbar_policy: Gtk.PolicyType.NEVER,
    vscrollbar_policy: Gtk.PolicyType.AUTOMATIC,
    vexpand: true,
  });
  const card = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 10 });
  card.add_css_class("popup-card");
  card.add_css_class("workspace-dialog");
  const heading = new Gtk.Box({ spacing: 8 });
  heading.append(new Gtk.Label({
    label: "Workspaces",
    xalign: 0,
    hexpand: true,
    css_classes: ["qs-title"],
  }));
  const dismiss = new Gtk.Button({
    icon_name: "window-close-symbolic",
    tooltip_text: "Close",
    css_classes: ["workspace-dialog-button", "workspace-dialog-close"],
  });
  dismiss.connect("clicked", () => popups.close());
  heading.append(dismiss);
  card.append(heading);
  card.append(list);
  card.append(status);

  const dragLayer = new Gtk.Fixed({ can_target: false });
  const overlay = new Gtk.Overlay({ child: card });
  overlay.add_overlay(dragLayer);

  const win = new Astal.Window({
    name: "workspaces-dialog",
    namespace: "palobar-workspaces",
    application: app,
    gdkmonitor: monitor,
    layer: Astal.Layer.OVERLAY,
    keymode: Astal.Keymode.EXCLUSIVE,
    default_width: 580,
    default_height: 500,
    visible: false,
    child: overlay,
  });
  win.add_css_class("popup-window");
  dialog = win;

  const theme = Gtk.IconTheme.get_for_display(monitor.get_display());
  const tiles = new Map<number, () => void>();
  const workspaceCards = new Map<Gtk.Widget, number>();
  const rowCleanup: (() => void)[] = [];
  let generation = 0;
  let loading = false;
  let moving = false;
  let unsubscribeClosed: (() => void) | undefined;

  function showStatus(message: string) {
    status.set_label(message);
    status.set_visible(Boolean(message));
  }

  async function closeWindow(id: number) {
    const current = generation;
    try {
      await niri.closeWindow(id);
    } catch (error) {
      console.error("[workspace dialog] close failed:", error);
      if (current === generation) showStatus("Could not close the window.");
    }
  }

  async function moveWindow(windowId: number, workspaceId: number) {
    moving = true;
    const current = generation;
    try {
      await niri.moveWindowToWorkspace(windowId, workspaceId);
      if (current === generation) await load();
    } catch (error) {
      console.error("[workspace dialog] move failed:", error);
      if (current === generation) showStatus("Could not move the window.");
    } finally {
      moving = false;
      if (current !== generation && win.get_visible()) void load();
    }
  }

  function appendWindow(column: Gtk.Box, window: NiriWindow) {
    const icon = windowIcon(window, theme);
    const tile = new Gtk.Box({
      tooltip_text: window.title || window.app_id || "Window",
      css_classes: ["workspace-window-tile"],
    });
    tile.append(icon);
    const drag = new Gtk.GestureDrag({ button: Gdk.BUTTON_PRIMARY });
    let ghost: Gtk.Image | undefined;
    const clearGhost = () => {
      if (!ghost) return;
      dragLayer.remove(ghost);
      ghost = undefined;
      tile.set_opacity(1);
    };
    const position = (x: number, y: number) =>
      tile.compute_point(overlay, new Graphene.Point({ x, y }));
    const update = drag.connect("drag-update", (gesture, dx, dy) => {
      const [started, x, y] = gesture.get_start_point();
      if (!started || moving || loading) return;
      const [valid, point] = position(x + dx, y + dy);
      if (!valid) return;
      const left = point.x - 18;
      const top = point.y - 18;
      if (ghost) {
        dragLayer.move(ghost, left, top);
      } else if (tile.drag_check_threshold(x, y, x + dx, y + dy)) {
        gesture.set_state(Gtk.EventSequenceState.CLAIMED);
        ghost = windowIcon(window, theme);
        dragLayer.put(ghost, left, top);
        tile.set_opacity(0.45);
      }
    });
    const end = drag.connect("drag-end", (gesture, dx, dy) => {
      const dragged = Boolean(ghost);
      clearGhost();
      const event = gesture.get_current_event()?.get_event_type();
      if (!win.get_visible() || moving || loading ||
          (event !== Gdk.EventType.BUTTON_RELEASE && event !== Gdk.EventType.TOUCH_END)) return;
      const [started, x, y] = gesture.get_start_point();
      if (!started) return;
      const releaseX = x + dx;
      const releaseY = y + dy;
      if (!dragged) {
        if (!tile.contains(releaseX, releaseY) ||
            tile.drag_check_threshold(x, y, releaseX, releaseY)) return;
        popups.close();
        void niri.focusWindow(window.id);
        return;
      }

      const [valid, point] = position(releaseX, releaseY);
      if (!valid) return;
      let widget = overlay.pick(point.x, point.y, Gtk.PickFlags.DEFAULT);
      while (widget && !workspaceCards.has(widget)) widget = widget.get_parent();
      const destination = widget ? workspaceCards.get(widget) : undefined;
      if (destination === undefined) {
        void closeWindow(window.id);
      } else if (destination !== window.workspace_id) {
        void moveWindow(window.id, destination);
      }
    });
    const cancel = drag.connect("cancel", clearGhost);
    tile.set_cursor_from_name("grab");
    tile.add_controller(drag);
    column.append(tile);
    tiles.set(window.id, () => {
      clearGhost();
      for (const handler of [update, end, cancel]) drag.disconnect(handler);
      tile.remove_controller(drag);
      column.remove(tile);
      if (!column.get_first_child()) {
        const columns = column.get_parent();
        if (columns instanceof Gtk.Box) columns.remove(column);
      }
    });
  }

  function appendWorkspace(workspace: NiriWorkspace, windows: NiriWindow[], showOutput: boolean) {
    const section = new Gtk.Box({
      orientation: Gtk.Orientation.VERTICAL,
      css_classes: ["workspace-dialog-row"],
    });
    const header = new Gtk.Box({ spacing: 8, css_classes: ["workspace-dialog-header"] });
    const label = new Gtk.Label({ hexpand: true, ellipsize: 3, xalign: 0 });
    const output = showOutput && workspace.output ? " · " + workspace.output : "";
    const title = (name: string | null) => (name ?? "Workspace " + workspace.idx) + output;
    label.set_label(title(workspace.name));

    const entry = new Gtk.Entry({ hexpand: true, visible: false, placeholder_text: "Workspace name" });
    const rename = new Gtk.Button({ label: "Rename", css_classes: ["workspace-dialog-button"] });
    const cancel = new Gtk.Button({ label: "Cancel", visible: false, css_classes: ["workspace-dialog-button"] });
    let name = workspace.name;
    let saving = false;
    const stopEditing = () => {
      label.set_visible(true);
      entry.set_visible(false);
      cancel.set_visible(false);
      rename.set_label("Rename");
    };
    const save = async () => {
      const next = entry.get_text().trim();
      if (!next || saving) return;
      if (next === name) {
        stopEditing();
        return;
      }
      saving = true;
      header.set_sensitive(false);
      const current = generation;
      try {
        await niri.renameWorkspace(workspace.id, next);
        if (current !== generation) return;
        name = next;
        label.set_label(title(name));
        showStatus("");
        stopEditing();
      } catch (error) {
        console.error("[workspace dialog] rename failed:", error);
        if (current === generation) showStatus("Could not rename the workspace.");
      } finally {
        if (current === generation) {
          saving = false;
          header.set_sensitive(true);
        }
      }
    };

    const renameHandler = rename.connect("clicked", () => {
      if (entry.get_visible()) {
        void save();
        return;
      }
      entry.set_text(name ?? "");
      label.set_visible(false);
      entry.set_visible(true);
      cancel.set_visible(true);
      rename.set_label("Save");
      entry.grab_focus();
      entry.select_region(0, -1);
    });
    const entryHandler = entry.connect("activate", () => void save());
    const cancelHandler = cancel.connect("clicked", stopEditing);
    rowCleanup.push(() => {
      rename.disconnect(renameHandler);
      entry.disconnect(entryHandler);
      cancel.disconnect(cancelHandler);
    });
    header.append(label);
    header.append(entry);
    header.append(rename);
    header.append(cancel);
    section.append(header);

    const columns = new Gtk.Box({
      spacing: 12,
      valign: Gtk.Align.START,
      css_classes: ["workspace-dialog-columns"],
    });
    let column: Gtk.Box | undefined;
    let previousIndex: number | undefined;
    for (const window of windows) {
      const index = window.layout?.pos_in_scrolling_layout?.[0];
      if (!column || index === undefined || index !== previousIndex) {
        column = new Gtk.Box({ orientation: Gtk.Orientation.VERTICAL, spacing: 8 });
        columns.append(column);
      }
      appendWindow(column, window);
      previousIndex = index;
    }
    section.append(new Gtk.ScrolledWindow({
      child: columns,
      hscrollbar_policy: Gtk.PolicyType.AUTOMATIC,
      vscrollbar_policy: Gtk.PolicyType.NEVER,
      propagate_natural_height: true,
    }));
    workspaceCards.set(section, workspace.id);
    rows.append(section);
  }

  const key = new Gtk.EventControllerKey({ propagation_phase: Gtk.PropagationPhase.CAPTURE });
  key.connect("key-pressed", (_controller, keyval) => {
    if (keyval !== Gdk.KEY_Escape) return false;
    popups.close();
    return true;
  });
  win.add_controller(key);
  win.connect("close-request", () => {
    popups.close();
    return true;
  });

  function clearRows() {
    for (const remove of tiles.values()) remove();
    tiles.clear();
    workspaceCards.clear();
    for (const cleanup of rowCleanup.splice(0)) cleanup();
    for (let row = rows.get_first_child(); row; row = rows.get_first_child()) rows.remove(row);
    scheduleGc();
  }

  async function load() {
    if (loading) return;
    loading = true;
    const current = generation;
    try {
      const [workspaces, windows] = await Promise.all([niri.queryWorkspaces(), niri.queryWindows()]);
      if (current !== generation) return;
      if (rows.get_first_child()) clearRows();
      const byWorkspace = new Map<number, NiriWindow[]>();
      for (const window of windows) {
        if (window.workspace_id === null) continue;
        const group = byWorkspace.get(window.workspace_id) ?? [];
        group.push(window);
        byWorkspace.set(window.workspace_id, group);
      }
      const showOutput = new Set(workspaces.map((workspace) => workspace.output)).size > 1;
      for (const workspace of workspaces) {
        const group = byWorkspace.get(workspace.id) ?? [];
        group.sort((a, b) => {
          const [ac, ar] = a.layout?.pos_in_scrolling_layout ?? [Infinity, 0];
          const [bc, br] = b.layout?.pos_in_scrolling_layout ?? [Infinity, 0];
          return (ac - bc) || (ar - br) || (a.id - b.id);
        });
        appendWorkspace(workspace, group, showOutput);
      }
      showStatus(workspaces.length ? "" : "No workspaces available.");
    } catch (error) {
      console.error("[workspace dialog] loading failed:", error);
      if (current === generation) showStatus("Could not load workspaces.");
    } finally {
      loading = false;
      if (current !== generation && win.get_visible() && !moving) void load();
    }
  }

  popups.active.subscribe(() => {
    const visible = popups.active() === "workspaces";
    if (visible === win.get_visible()) return;
    generation += 1;
    win.set_visible(visible);
    if (visible) {
      showStatus("Loading workspaces…");
      unsubscribeClosed = niri.subscribeWindowClosed((id) => {
        const remove = tiles.get(id);
        if (!remove) return;
        remove();
        tiles.delete(id);
        scheduleGc();
      });
      if (!moving) void load();
    } else {
      unsubscribeClosed?.();
      unsubscribeClosed = undefined;
      clearRows();
    }
  });
}
