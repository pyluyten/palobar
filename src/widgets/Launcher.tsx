import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import type AstalApps from "gi://AstalApps";
import type GObject from "gi://GObject";
import GLib from "gi://GLib";
import { popups } from "../services/popups";
import { getApps, scheduleGc } from "../services/runtime";
import { createMiniDock } from "./Dock";

const PAGE_COLS = 8;
const PAGE_ROWS = 3;
const APPS_PER_PAGE = PAGE_COLS * PAGE_ROWS;
const APP_ICON_SIZE = 81;

let launcherWindow: Gtk.Window | null = null;
let disposeLauncher: (() => void) | null = null;
const stateListeners = new Set<(open: boolean) => void>();

function notifyLauncherState(open: boolean) {
  for (const listener of stateListeners) {
    listener(open);
  }
}

export function onLauncherStateChange(
  listener: (open: boolean) => void,
): () => void {
  stateListeners.add(listener);
  return () => stateListeners.delete(listener);
}

export function closeLauncher() {
  const window = launcherWindow;
  if (!window) return;

  launcherWindow = null;
  disposeLauncher?.();
  disposeLauncher = null;
  window.set_focus(null);
  window.set_child(null);
  window.destroy();
  notifyLauncherState(false);
  scheduleGc();
}

export function isLauncherOpen(): boolean {
  return launcherWindow !== null;
}

export function openLauncherFromOverview(monitor: Gdk.Monitor, initialQuery?: string) {
  if (launcherWindow) return;
  showLauncher(monitor, initialQuery, true);
}

export function toggleLauncher(monitor?: Gdk.Monitor, initialQuery?: string) {
  if (launcherWindow) {
    closeLauncher();
    return;
  }
  showLauncher(monitor, initialQuery, false);
}

function showLauncher(
  monitor: Gdk.Monitor | undefined,
  initialQuery: string | undefined,
  fromOverview: boolean,
) {
  popups.close();

  const targetMonitor = monitor ?? app.get_monitors()[0];
  if (!targetMonitor) return;

  // Instance partagée : on ne ré-indexe plus les applications à chaque ouverture.
  const appsService = getApps();
  const iconTheme = Gtk.IconTheme.get_for_display(targetMonitor.get_display());
  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;

  let results: AstalApps.Application[] = [];
  let currentPage = 0;
  let allSorted: AstalApps.Application[] | null = null;
  let focusSource = 0;
  const disconnectSignals: (() => void)[] = [];
  const trackSignal = (object: GObject.Object, id: number) => {
    disconnectSignals.push(() => object.disconnect(id));
  };

  const stack = new Gtk.Stack({
    transition_duration: 220,
    hexpand: true,
    vexpand: true,
  });

  const prevBtn = new Gtk.Button({
    valign: Gtk.Align.CENTER,
    tooltip_text: "Previous Page",
    child: new Gtk.Image({ icon_name: "go-previous-symbolic", pixel_size: 22 }),
  });
  prevBtn.add_css_class("launcher-nav-btn");

  const nextBtn = new Gtk.Button({
    valign: Gtk.Align.CENTER,
    tooltip_text: "Next Page",
    child: new Gtk.Image({ icon_name: "go-next-symbolic", pixel_size: 22 }),
  });
  nextBtn.add_css_class("launcher-nav-btn");

  const dotsBox = new Gtk.Box({
    spacing: 8,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.CENTER,
  });
  dotsBox.add_css_class("launcher-dots");

  const launchApp = (application: AstalApps.Application) => {
    closeLauncher();
    application.launch();
  };

  const buildPage = () => {
    const flowBox = new Gtk.FlowBox({
      valign: Gtk.Align.START,
      halign: Gtk.Align.CENTER,
      homogeneous: true,
      max_children_per_line: PAGE_COLS,
      min_children_per_line: PAGE_COLS,
      row_spacing: 18,
      column_spacing: 18,
      selection_mode: Gtk.SelectionMode.SINGLE,
      activate_on_single_click: true,
    });
    flowBox.add_css_class("launcher-grid");

    const tiles = Array.from({ length: APPS_PER_PAGE }, () => {
      const tile = new Gtk.Box({
        orientation: Gtk.Orientation.VERTICAL,
        spacing: 8,
        halign: Gtk.Align.CENTER,
        valign: Gtk.Align.CENTER,
      });
      tile.add_css_class("launcher-tile");

      const icon = new Gtk.Image({ pixel_size: APP_ICON_SIZE });
      icon.add_css_class("launcher-tile-icon");

      const label = new Gtk.Label({
        max_width_chars: 13,
        ellipsize: 3,
        justify: Gtk.Justification.CENTER,
      });
      label.add_css_class("launcher-tile-label");

      tile.append(icon);
      tile.append(label);

      const flowChild = new Gtk.FlowBoxChild({
        child: tile,
        visible: false,
      });
      flowChild.add_css_class("launcher-flow-child");
      const drag = new Gtk.DragSource({ actions: Gdk.DragAction.COPY });
      const slot = {
        child: flowChild,
        icon,
        label,
        application: null as AstalApps.Application | null,
      };
      trackSignal(drag, drag.connect("prepare", () => {
        const application = slot.application;
        if (!application) return null;
        return Gdk.ContentProvider.new_union([
          Gdk.ContentProvider.new_for_value(application),
          Gdk.ContentProvider.new_for_value(application.entry),
        ]);
      }));
      trackSignal(drag, drag.connect("drag-begin", () => {
        drag.set_icon(new Gtk.WidgetPaintable({ widget: icon }), 27, 27);
      }));
      trackSignal(drag, drag.connect("drag-end", () => drag.set_icon(null, 0, 0)));
      flowChild.add_controller(drag);
      flowBox.append(flowChild);
      return slot;
    });

    trackSignal(flowBox, flowBox.connect("child-activated", (_box, child) => {
      const application = tiles[child.get_index()]?.application;
      if (application) launchApp(application);
    }));

    return { flowBox, tiles };
  };

  // Deux grilles suffisent pour animer une transition sans reconstruire les tuiles.
  const grids = [buildPage()];
  stack.add_named(grids[0].flowBox, "grid-0");
  let activeGrid = 0;
  const dots: Gtk.Button[] = [];
  const pageCount = () => Math.ceil(results.length / APPS_PER_PAGE);

  const updatePaginationUi = () => {
    const total = pageCount();
    prevBtn.set_visible(total > 1);
    nextBtn.set_visible(total > 1);
    prevBtn.set_sensitive(currentPage > 0);
    nextBtn.set_sensitive(currentPage < total - 1);
    dotsBox.set_visible(total > 1);

    while (dots.length < total) {
      const page = dots.length;
      const dot = new Gtk.Button({ width_request: 10, height_request: 10 });
      dot.add_css_class("launcher-dot");
      trackSignal(dot, dot.connect("clicked", () => goToPage(page)));
      dots.push(dot);
      dotsBox.append(dot);
    }

    dots.forEach((dot, index) => {
      dot.set_visible(index < total);
      if (index === currentPage) dot.add_css_class("active");
      else dot.remove_css_class("active");
    });
  };

  const fillPage = (grid: ReturnType<typeof buildPage>, page: number) => {
    const offset = page * APPS_PER_PAGE;
    grid.flowBox.unselect_all();
    grid.tiles.forEach((slot, index) => {
      const application = results[offset + index] ?? null;
      if (slot.application === application) return;
      slot.application = application;
      // GTK's background icon loading can race with rapid window teardown.
      const paintable = application ? iconTheme.lookup_icon(
        application.iconName || "application-x-executable",
        null,
        APP_ICON_SIZE,
        targetMonitor.get_scale_factor(),
        slot.icon.get_direction(),
        Gtk.IconLookupFlags.NONE,
      ) : null;
      slot.icon.set_from_paintable(paintable);
      slot.child.set_visible(application !== null);
      slot.label.set_label(application?.name ?? "");
      slot.child.set_tooltip_text(application ? application.description || application.name : null);
    });
    if (grid.tiles[0].application) grid.flowBox.select_child(grid.tiles[0].child);
  };

  const goToPage = (page: number) => {
    if (page < 0 || page >= pageCount() || page === currentPage) return;
    const transition = page > currentPage
      ? Gtk.StackTransitionType.SLIDE_LEFT : Gtk.StackTransitionType.SLIDE_RIGHT;
    activeGrid = 1 - activeGrid;
    if (!grids[activeGrid]) {
      const grid = buildPage();
      grids.push(grid);
      stack.add_named(grid.flowBox, `grid-${activeGrid}`);
    }
    currentPage = page;
    fillPage(grids[activeGrid], page);
    stack.set_visible_child_full(`grid-${activeGrid}`, transition);
    updatePaginationUi();
  };

  trackSignal(prevBtn, prevBtn.connect("clicked", () => goToPage(currentPage - 1)));
  trackSignal(nextBtn, nextBtn.connect("clicked", () => goToPage(currentPage + 1)));

  const renderApps = (query: string) => {
    const trimmed = query.trim();
    if (trimmed) {
      results = appsService.fuzzy_query(trimmed);
    } else {
      allSorted ??= [...appsService.list].sort((a, b) => a.name.localeCompare(b.name));
      results = allSorted;
    }
    currentPage = 0;
    fillPage(grids[activeGrid], currentPage);
    updatePaginationUi();
  };

  // --- Search Entry ---
  const searchEntry = new Gtk.Entry({
    placeholder_text: "Type to search",
    primary_icon_name: "system-search-symbolic",
    halign: Gtk.Align.CENTER,
    width_request: 400,
    text: initialQuery ?? "",
  });
  searchEntry.add_css_class("launcher-search");

  trackSignal(searchEntry, searchEntry.connect("changed", () => renderApps(searchEntry.text)));
  trackSignal(searchEntry, searchEntry.connect("activate", () => {
    if (results[0]) launchApp(results[0]);
  }));

  const searchKeyCtrl = new Gtk.EventControllerKey();
  trackSignal(searchKeyCtrl, searchKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
    const { flowBox, tiles: [first] } = grids[activeGrid];
    if (keyval !== Gdk.KEY_Down || !first.application) return false;
    flowBox.select_child(first.child);
    first.child.grab_focus();
    return true;
  }));
  searchEntry.add_controller(searchKeyCtrl);

  renderApps(initialQuery ?? "");

  // --- Horizontal Grid Row (Prev + Stack + Next) ---
  const gridRow = new Gtk.Box({
    orientation: Gtk.Orientation.HORIZONTAL,
    spacing: 12,
    vexpand: true,
    hexpand: true,
    valign: Gtk.Align.CENTER,
  });
  gridRow.append(prevBtn);
  gridRow.append(stack);
  gridRow.append(nextBtn);

  const scrollCtrl = new Gtk.EventControllerScroll({
    flags: Gtk.EventControllerScrollFlags.VERTICAL |
      Gtk.EventControllerScrollFlags.HORIZONTAL,
  });
  let lastScrollTime = 0;
  trackSignal(scrollCtrl, scrollCtrl.connect("scroll", (_ctrl, dx, dy) => {
    const now = GLib.get_monotonic_time();
    if (now - lastScrollTime < 220_000) return false;
    const delta = Math.abs(dx) > Math.abs(dy) ? dx : dy;
    if (delta > 0.2) {
      lastScrollTime = now;
      goToPage(currentPage + 1);
      return true;
    } else if (delta < -0.2) {
      lastScrollTime = now;
      goToPage(currentPage - 1);
      return true;
    }
    return false;
  }));
  gridRow.add_controller(scrollCtrl);

  // --- Mini Dock (deduplicated open apps from niri) ---
  const dock = createMiniDock({
    appsService,
    onActivateApp: closeLauncher,
    acceptFavorites: true,
    applicationsToggle: fromOverview
      ? { active: true, onToggle: closeLauncher }
      : undefined,
  });

  // --- Main Container ---
  const container = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 20,
    halign: Gtk.Align.CENTER,
    valign: Gtk.Align.FILL,
    margin_top: 42,
    margin_bottom: 24,
    margin_start: 32,
    margin_end: 32,
    width_request: 1020,
  });
  container.add_css_class("launcher-container");
  container.append(searchEntry);
  container.append(gridRow);
  container.append(dotsBox);
  container.append(dock.widget);

  const backdropClick = new Gtk.GestureClick();
  trackSignal(backdropClick, backdropClick.connect("released", (_gesture, _n, x, y) => {
    const picked = launcherWindow?.pick(x, y, Gtk.PickFlags.DEFAULT);
    if (picked === launcherWindow || picked === container) {
      closeLauncher();
    }
  }));

  const win = new Astal.Window({
    visible: false,
    name: "activities-launcher",
    gdkmonitor: targetMonitor,
    layer: Astal.Layer.OVERLAY,
    exclusivity: Astal.Exclusivity.IGNORE,
    keymode: Astal.Keymode.EXCLUSIVE,
    anchor: TOP | BOTTOM | LEFT | RIGHT,
    application: app,
    child: container,
  });
  win.add_css_class("launcher-window");
  trackSignal(win, win.connect("close-request", () => {
    closeLauncher();
    return true;
  }));

  const winKeyCtrl = new Gtk.EventControllerKey();
  trackSignal(winKeyCtrl, winKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
    if (keyval === Gdk.KEY_Escape) {
      closeLauncher();
      return true;
    }
    if (keyval === Gdk.KEY_Page_Down) {
      goToPage(currentPage + 1);
      return true;
    }
    if (keyval === Gdk.KEY_Page_Up) {
      goToPage(currentPage - 1);
      return true;
    }
    return false;
  }));
  win.add_controller(winKeyCtrl);
  win.add_controller(backdropClick);

  launcherWindow = win;
  disposeLauncher = () => {
    dock.dispose();
    if (focusSource) GLib.source_remove(focusSource);
    for (const disconnect of disconnectSignals) disconnect();
    disconnectSignals.length = 0;
  };
  notifyLauncherState(true);
  win.set_visible(true);

  focusSource = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    focusSource = 0;
    searchEntry.grab_focus();
    searchEntry.set_position(-1);
    return GLib.SOURCE_REMOVE;
  });
}
