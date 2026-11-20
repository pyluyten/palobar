import app from "ags/gtk4/app";
import { Astal, Gdk, Gtk } from "ags/gtk4";
import { createState } from "ags";
import {
  copyClipboardEntry,
  decodeClipboardBytes,
  deleteClipboardEntry,
  getClipboardWatchError,
  listClipboardEntries,
  wipeClipboard,
  type ClipboardEntry,
} from "../services/clipboard";
import { popups } from "../services/popups";

export function ClipboardPopup(gdkmonitor: Gdk.Monitor) {
  const { TOP, RIGHT } = Astal.WindowAnchor;

  const [entries, setEntries] = createState<ClipboardEntry[]>([]);
  const [filterText, setFilterText] = createState("");

  const [watchError, setWatchError] = createState<string | null>(null);

  let listContainer!: Gtk.Box;
  let searchEntry!: Gtk.Entry;
  let renderToken = 0;

  const rebuildList = () => {
    if (!listContainer) return;
    const token = ++renderToken;

    let child = listContainer.get_first_child();
    while (child) {
      const next = child.get_next_sibling();
      listContainer.remove(child);
      child = next;
    }

    const q = filterText.get().trim().toLowerCase();
    const filtered = q
      ? entries.get().filter((e) => e.preview.toLowerCase().includes(q))
      : entries.get();

    for (const entry of filtered) {
      const rowBox = new Gtk.Box({
        spacing: 8,
        valign: Gtk.Align.CENTER,
      });
      rowBox.add_css_class("clipboard-item");

      const selectBtn = new Gtk.Button({ hexpand: true });
      selectBtn.add_css_class("clipboard-select-btn");

      if (entry.isImage) {
        const imgBox = new Gtk.Box({
          orientation: Gtk.Orientation.VERTICAL,
          spacing: 6,
          halign: Gtk.Align.START,
        });

        const picture = new Gtk.Picture({
          content_fit: Gtk.ContentFit.CONTAIN,
          halign: Gtk.Align.START,
          height_request: 72,
        });
        picture.add_css_class("clipboard-thumb");

        const meta = new Gtk.Label({
          label: entry.preview.replace(/^\[\[ binary data\s*|\s*\]\]$/g, ""),
          halign: Gtk.Align.START,
          ellipsize: 3,
        });
        meta.add_css_class("clipboard-meta");

        imgBox.append(picture);
        imgBox.append(meta);
        selectBtn.set_child(imgBox);

        decodeClipboardBytes(entry.id).then((bytes) => {
          if (token !== renderToken || !bytes) return;
          try {
            const texture = Gdk.Texture.new_from_bytes(bytes);
            picture.set_paintable(texture);
          } catch {
            // fallback if image cannot be decoded
          }
        });
      } else {
        const lbl = new Gtk.Label({
          label: entry.preview,
          halign: Gtk.Align.START,
          lines: 2,
          max_width_chars: 36,
          ellipsize: 3,
          wrap: true,
        });
        lbl.add_css_class("clipboard-text");
        selectBtn.set_child(lbl);
      }

      selectBtn.connect("clicked", () => {
        popups.close();
        copyClipboardEntry(entry.id);
      });

      const delBtn = new Gtk.Button({
        valign: Gtk.Align.CENTER,
        tooltip_text: "Remove",
        child: new Gtk.Image({ icon_name: "window-close-symbolic" }),
      });
      delBtn.add_css_class("notif-dismiss-btn");
      delBtn.connect("clicked", async () => {
        await deleteClipboardEntry(entry.id);
        refresh();
      });

      rowBox.append(selectBtn);
      rowBox.append(delBtn);
      listContainer.append(rowBox);
    }
  };

  const refresh = async () => {
    const list = await listClipboardEntries(25);
    setWatchError(getClipboardWatchError());
    setEntries(list);
    rebuildList();
  };

  popups.active.subscribe(() => {
    if (popups.active.get() === "clipboard") {
      if (searchEntry) searchEntry.set_text("");
      setFilterText("");
      refresh();
    } else {
      renderToken++;
      if (listContainer) {
        let child = listContainer.get_first_child();
        while (child) {
          const next = child.get_next_sibling();
          listContainer.remove(child);
          child = next;
        }
      }
    }
  });

  return (
    <window
      name="clipboard-popup"
      class="popup-window"
      gdkmonitor={gdkmonitor}
      anchor={TOP | RIGHT}
      marginTop={6}
      marginRight={10}
      keymode={Astal.Keymode.ON_DEMAND}
      visible={popups.active((p) => p === "clipboard")}
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
        class="popup-card clipboard-drawer"
        orientation={Gtk.Orientation.VERTICAL}
        spacing={12}
      >
        <box class="notif-header" spacing={8}>
          <label
            class="qs-title"
            hexpand
            halign={Gtk.Align.START}
            label="Clipboard"
          />

          <button
            class="notif-clear-btn"
            visible={entries((list) => list.length > 0)}
            onClicked={async () => {
              await wipeClipboard();
              refresh();
            }}
          >
            <label label="Clear" />
          </button>
        </box>

        <Gtk.Entry
          placeholderText="Filter history..."
          primaryIconName="system-search-symbolic"
          $={(self) => {
            searchEntry = self;
            self.add_css_class("clipboard-search");
            self.connect("changed", () => {
              setFilterText(self.text);
              rebuildList();
            });
          }}
        />

        <box
          class="notif-empty"
          orientation={Gtk.Orientation.VERTICAL}
          spacing={8}
          visible={entries((list) => list.length === 0)}
        >
          <image iconName="edit-paste-symbolic" />
          <label
            label={watchError((error) => error ?? "Clipboard is empty")}
            maxWidthChars={38}
            wrap
          />
        </box>

        <Gtk.ScrolledWindow
          maxContentHeight={420}
          propagateNaturalHeight
          visible={entries((list) => list.length > 0)}
        >
          <box
            orientation={Gtk.Orientation.VERTICAL}
            spacing={6}
            $={(self) => {
              listContainer = self;
            }}
          />
        </Gtk.ScrolledWindow>
      </box>
    </window>
  );
}
