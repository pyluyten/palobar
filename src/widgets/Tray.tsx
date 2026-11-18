import { Gtk, Gdk } from "ags/gtk4";
import { createState, For } from "ags";
import AstalTray from "gi://AstalTray";

function TrayItemButton(item: AstalTray.TrayItem) {
  const [gicon, setGicon] = createState(item.gicon);
  const [tooltip, setTooltip] = createState(
    item.tooltipMarkup || item.title || "",
  );

  item.connect("notify::gicon", () => setGicon(item.gicon));
  item.connect("notify::tooltip-markup", () =>
    setTooltip(item.tooltipMarkup || item.title || ""),
  );
  item.connect("notify::title", () =>
    setTooltip(item.tooltipMarkup || item.title || ""),
  );

  let popover: Gtk.PopoverMenu | null = null;

  const syncMenu = (widget: Gtk.Widget) => {
    if (item.actionGroup) {
      widget.insert_action_group("dbusmenu", item.actionGroup);
    }
    if (popover && item.menuModel) {
      popover.set_menu_model(item.menuModel);
    }
  };

  const openMenu = (widget: Gtk.Widget) => {
    if (!item.menuModel) return;
    item.about_to_show();
    if (!popover) {
      popover = new Gtk.PopoverMenu({
        menu_model: item.menuModel,
      });
      popover.set_parent(widget);
    }
    syncMenu(widget);
    popover.popup();
  };

  return (
    <button
      class="panel-button tray-item"
      tooltipMarkup={tooltip}
      onClicked={(self) => {
        if (item.isMenu) {
          openMenu(self);
        } else {
          item.activate(0, 0);
        }
      }}
      $={(self) => {
        syncMenu(self);

        const agId = item.connect("notify::action-group", () => syncMenu(self));
        const mmId = item.connect("notify::menu-model", () => syncMenu(self));

        const rightClick = new Gtk.GestureClick({
          button: Gdk.BUTTON_SECONDARY,
        });
        rightClick.connect("pressed", () => {
          openMenu(self);
        });
        self.add_controller(rightClick);

        const middleClick = new Gtk.GestureClick({
          button: Gdk.BUTTON_MIDDLE,
        });
        middleClick.connect("pressed", () => {
          item.secondary_activate(0, 0);
        });
        self.add_controller(middleClick);

        self.connect("destroy", () => {
          item.disconnect(agId);
          item.disconnect(mmId);
          if (popover) {
            popover.unparent();
            popover = null;
          }
        });
      }}
    >
      <image gicon={gicon} pixelSize={15} />
    </button>
  );
}

export function SysTray() {
  const tray = AstalTray.get_default();
  const [items, setItems] = createState<AstalTray.TrayItem[]>(tray.items);

  const sync = () => {
    setItems([...tray.items]);
  };

  tray.connect("item-added", sync);
  tray.connect("item-removed", sync);

  return (
    <box class="tray" spacing={2} visible={items((list) => list.length > 0)}>
      <For each={items}>{(item) => TrayItemButton(item)}</For>
    </box>
  );
}
