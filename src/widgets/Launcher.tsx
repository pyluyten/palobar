import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import type AstalApps from "gi://AstalApps";
import GLib from "gi://GLib";
import { popups } from "../services/popups";
import { getApps, scheduleGc } from "../services/runtime";
import { createMiniDock } from "./Dock";

const PAGE_COLS = 8;
const PAGE_ROWS = 3;
const APPS_PER_PAGE = PAGE_COLS * PAGE_ROWS;

let launcherWindow: Gtk.Window | null = null;
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
  if (launcherWindow) {
    launcherWindow.close();
    launcherWindow.destroy();
    launcherWindow = null;
    notifyLauncherState(false);
    scheduleGc(); // les widgets natifs ne sont libérés qu'au passage du GC
  }
}

export function isLauncherOpen(): boolean {
  return launcherWindow !== null;
}

export function openLauncher(monitor?: Gdk.Monitor, initialQuery?: string) {
  if (launcherWindow) return;
  toggleLauncher(monitor, initialQuery);
}

export function toggleLauncher(monitor?: Gdk.Monitor, initialQuery?: string) {
  if (launcherWindow) {
    closeLauncher();
    return;
  }

  popups.close();

  const targetMonitor = monitor ?? app.get_monitors()[0];
  if (!targetMonitor) return;

  // Instance partagée : on ne ré-indexe plus les applications à chaque ouverture.
  const appsService = getApps();
  const { TOP, BOTTOM, LEFT, RIGHT } = Astal.WindowAnchor;

  let pages: AstalApps.Application[][] = [];
  let pageGrids: Gtk.FlowBox[] = [];
  let built: boolean[] = []; // pages réellement construites (création paresseuse)
  let currentPage = 0;
  let allSorted: AstalApps.Application[] | null = null; // trié une seule fois

  const stack = new Gtk.Stack({
    transition_type: Gtk.StackTransitionType.SLIDE_LEFT_RIGHT,
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

  const updatePaginationUi = () => {
    const total = pages.length;
    prevBtn.set_visible(total > 1);
    nextBtn.set_visible(total > 1);
    prevBtn.set_sensitive(currentPage > 0);
    nextBtn.set_sensitive(currentPage < total - 1);
    dotsBox.set_visible(total > 1);

    let dot = dotsBox.get_first_child();
    let idx = 0;
    while (dot) {
      if (idx === currentPage) {
        dot.add_css_class("active");
      } else {
        dot.remove_css_class("active");
      }
      dot = dot.get_next_sibling();
      idx++;
    }
  };

  // Construit UNE page (au plus APPS_PER_PAGE tuiles).
  const buildPage = (pageApps: AstalApps.Application[]): Gtk.FlowBox => {
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

    for (const item of pageApps) {
      const tile = new Gtk.Box({
        orientation: Gtk.Orientation.VERTICAL,
        spacing: 8,
        halign: Gtk.Align.CENTER,
        valign: Gtk.Align.CENTER,
      });
      tile.add_css_class("launcher-tile");

      const icon = new Gtk.Image({
        icon_name: item.iconName || "application-x-executable",
        pixel_size: 54,
      });
      icon.add_css_class("launcher-tile-icon");

      const label = new Gtk.Label({
        label: item.name,
        max_width_chars: 13,
        ellipsize: 3,
        justify: Gtk.Justification.CENTER,
      });
      label.add_css_class("launcher-tile-label");

      tile.append(icon);
      tile.append(label);

      const flowChild = new Gtk.FlowBoxChild({
        child: tile,
        tooltip_text: item.description || item.name,
      });
      flowChild.add_css_class("launcher-flow-child");
      const drag = new Gtk.DragSource({ actions: Gdk.DragAction.COPY });
      drag.connect("prepare", () => Gdk.ContentProvider.new_for_value(item));
      drag.connect("drag-begin", () => {
        drag.set_icon(new Gtk.WidgetPaintable({ widget: icon }), 27, 27);
      });
      flowChild.add_controller(drag);
      flowBox.append(flowChild);
    }

    flowBox.connect("child-activated", (_box, child) => {
      const idx = child.get_index();
      const selected = pageApps[idx];
      if (selected) launchApp(selected);
    });

    const first = flowBox.get_child_at_index(0);
    if (first) flowBox.select_child(first);

    return flowBox;
  };

  // Construit la page seulement si elle ne l'est pas déjà.
  const ensurePage = (idx: number) => {
    if (idx < 0 || idx >= pages.length || built[idx]) return;
    const flowBox = buildPage(pages[idx]);
    pageGrids[idx] = flowBox;
    built[idx] = true;
    stack.add_named(flowBox, `page-${idx}`);
  };

  const goToPage = (pageIdx: number) => {
    if (pageIdx < 0 || pageIdx >= pages.length) return;
    ensurePage(pageIdx);
    currentPage = pageIdx;
    stack.set_visible_child_name(`page-${pageIdx}`);
    updatePaginationUi();
  };

  prevBtn.connect("clicked", () => goToPage(currentPage - 1));
  nextBtn.connect("clicked", () => goToPage(currentPage + 1));

  const renderApps = (query: string) => {
    let oldChild = stack.get_first_child();
    while (oldChild) {
      const next = oldChild.get_next_sibling();
      stack.remove(oldChild);
      oldChild = next;
    }

    let oldDot = dotsBox.get_first_child();
    while (oldDot) {
      const next = oldDot.get_next_sibling();
      dotsBox.remove(oldDot);
      oldDot = next;
    }

    const trimmed = query.trim();
    let list: AstalApps.Application[];
    if (trimmed) {
      list = appsService.fuzzy_query(trimmed);
    } else {
      allSorted ??= [...appsService.list].sort((a, b) =>
        a.name.localeCompare(b.name),
      );
      list = allSorted;
    }

    pages = [];
    pageGrids = [];
    built = [];
    for (let i = 0; i < list.length; i += APPS_PER_PAGE) {
      pages.push(list.slice(i, i + APPS_PER_PAGE));
    }
    if (pages.length === 0) {
      pages.push([]);
    }

    // Les points de pagination sont légers : on les crée tous.
    pages.forEach((_pageApps, pIdx) => {
      const dotBtn = new Gtk.Button({
        width_request: 10,
        height_request: 10,
      });
      dotBtn.add_css_class("launcher-dot");
      dotBtn.connect("clicked", () => goToPage(pIdx));
      dotsBox.append(dotBtn);
    });

    // Mais seule la première page est construite maintenant.
    currentPage = 0;
    ensurePage(0);
    stack.set_visible_child_name("page-0");
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

  searchEntry.connect("changed", () => {
    renderApps(searchEntry.text);
  });

  searchEntry.connect("activate", () => {
    const firstPage = pages[0];
    if (firstPage && firstPage.length > 0) {
      launchApp(firstPage[0]);
    }
  });

  const searchKeyCtrl = new Gtk.EventControllerKey();
  searchKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
    if (keyval === Gdk.KEY_Down) {
      const grid = pageGrids[currentPage];
      const first = grid?.get_child_at_index(0);
      if (first) {
        grid.select_child(first);
        first.grab_focus();
        return true;
      }
    }
    return false;
  });
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
  scrollCtrl.connect("scroll", (_ctrl, dx, dy) => {
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
  });
  gridRow.add_controller(scrollCtrl);

  // --- Mini Dock (deduplicated open apps from niri) ---
  const dockBox = createMiniDock({
    appsService,
    isAlive: () => launcherWindow === win,
    onActivateApp: () => closeLauncher(),
    acceptFavorites: true,
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
  container.append(dockBox);

  const backdropClick = new Gtk.GestureClick();
  backdropClick.connect("released", (_gesture, _n, x, y) => {
    const picked = launcherWindow?.pick(x, y, Gtk.PickFlags.DEFAULT);
    if (picked === launcherWindow || picked === container) {
      closeLauncher();
    }
  });

  const win = (
    <window
      visible={false}
      name="activities-launcher"
      class="launcher-window"
      gdkmonitor={targetMonitor}
      layer={Astal.Layer.OVERLAY}
      exclusivity={Astal.Exclusivity.IGNORE}
      keymode={Astal.Keymode.EXCLUSIVE}
      anchor={TOP | BOTTOM | LEFT | RIGHT}
      application={app}
    >
      {container}
    </window>
  ) as Gtk.Window;

  const winKeyCtrl = new Gtk.EventControllerKey();
  winKeyCtrl.connect("key-pressed", (_ctrl, keyval) => {
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
  });
  win.add_controller(winKeyCtrl);
  win.add_controller(backdropClick);

  launcherWindow = win;
  notifyLauncherState(true);
  win.set_visible(true);

  GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    if (launcherWindow === win) {
      searchEntry.grab_focus();
      searchEntry.set_position(-1);
    }
    return GLib.SOURCE_REMOVE;
  });
}
