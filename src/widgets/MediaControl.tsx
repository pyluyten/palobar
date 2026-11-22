import { Gtk } from "ags/gtk4";
import { createBinding, createComputed, For } from "ags";
import AstalMpris from "gi://AstalMpris";
import Pango from "gi://Pango";

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

  return (
    <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
      <label class="qs-title" xalign={0} label="Media" />
      <label
        class="media-player media-player-subtitle"
        xalign={0}
        label="No Media Playing"
        visible={players((list) => list.length === 0)}
      />
      <Gtk.ScrolledWindow
        hscrollbarPolicy={Gtk.PolicyType.NEVER}
        maxContentHeight={174}
        propagateNaturalHeight
        visible={players((list) => list.length > 0)}
      >
        <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
          <For each={players}>
            {(player) => <MediaPlayer player={player} />}
          </For>
        </box>
      </Gtk.ScrolledWindow>
    </box>
  );
}
