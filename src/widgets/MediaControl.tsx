import { Gtk } from "ags/gtk4";
import { createBinding, createComputed, For } from "ags";
import AstalMpris from "gi://AstalMpris";
import Pango from "gi://Pango";
import { popups } from "../services/popups";

function MediaPlayer({ player }: { player: AstalMpris.Player }) {
  const title = createBinding(player, "title");
  const identity = createBinding(player, "identity");
  const artist = createBinding(player, "artist");
  const status = createBinding(player, "playback_status");
  const canControl = createBinding(player, "can_control");
  const canPlay = createBinding(player, "can_play");
  const canPause = createBinding(player, "can_pause");
  const canPrevious = createBinding(player, "can_go_previous");
  const canNext = createBinding(player, "can_go_next");
  const playing = status((value) => value === AstalMpris.PlaybackStatus.PLAYING);

  return (
    <box class="media-player" orientation={Gtk.Orientation.VERTICAL} spacing={8}>
      <label
        class="media-player-title"
        label={title((value) => value || "No track information")}
        tooltipText={title}
        maxWidthChars={32}
        ellipsize={Pango.EllipsizeMode.END}
        xalign={0}
      />
      <label
        class="media-player-subtitle"
        label={createComputed(() =>
          [artist(), identity() || player.bus_name.slice("org.mpris.MediaPlayer2.".length)]
            .filter(Boolean).join(" · "),
        )}
        maxWidthChars={32}
        ellipsize={Pango.EllipsizeMode.END}
        xalign={0}
      />
      <box halign={Gtk.Align.CENTER} spacing={8}>
        <button
          class="qs-circle-btn"
          tooltipText="Previous Track"
          sensitive={createComputed(() => canControl() && canPrevious())}
          onClicked={() => player.previous()}
        >
          <image iconName="media-skip-backward-symbolic" />
        </button>
        <button
          class="qs-circle-btn"
          tooltipText={playing((value) => value ? "Pause" : "Play")}
          sensitive={createComputed(() => canControl() && (playing() ? canPause() : canPlay()))}
          onClicked={() => {
            if (playing.get()) player.pause();
            else player.play();
          }}
        >
          <image
            iconName={playing((value) =>
              value ? "media-playback-pause-symbolic" : "media-playback-start-symbolic",
            )}
          />
        </button>
        <button
          class="qs-circle-btn"
          tooltipText="Next Track"
          sensitive={createComputed(() => canControl() && canNext())}
          onClicked={() => player.next()}
        >
          <image iconName="media-skip-forward-symbolic" />
        </button>
      </box>
    </box>
  );
}

export function MediaControl() {
  const mpris = AstalMpris.get_default();
  const players = createBinding(mpris, "players")((list) => list ?? []);

  const popover = new Gtk.Popover({
    position: Gtk.PositionType.BOTTOM,
    child: (
      <Gtk.ScrolledWindow
        hscrollbarPolicy={Gtk.PolicyType.NEVER}
        maxContentHeight={380}
        propagateNaturalHeight
      >
        <box class="media-player-list" orientation={Gtk.Orientation.VERTICAL} spacing={8}>
          <For each={players}>
            {(player) => <MediaPlayer player={player} />}
          </For>
        </box>
      </Gtk.ScrolledWindow>
    ) as Gtk.Widget,
  });
  popover.add_css_class("media-popover");

  popover.connect("show", () => popups.close());

  return (
    <Gtk.MenuButton
      class="media-button"
      iconName="multimedia-player-symbolic"
      tooltipText="Media Controls"
      visible={players((list) => list.length > 0)}
      popover={popover}
      $={(button) => {
        button.connect("notify::visible", () => {
          if (!button.get_visible()) button.popdown();
        });
      }}
    />
  );
}
