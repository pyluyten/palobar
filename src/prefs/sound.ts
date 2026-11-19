import { Gtk } from "ags/gtk4";
import AstalWp from "gi://AstalWp";
import { makeGroupBox, makeRow } from "./ui";

export function buildSoundPage(): Gtk.Widget {
  const soundPage = new Gtk.Box({
    orientation: Gtk.Orientation.VERTICAL,
    spacing: 14,
  });
  soundPage.add_css_class("prefs-page-body");

  const wp = AstalWp.get_default();

  // --- Output Section ---
  const outTitle = new Gtk.Label({
    label: "Output",
    halign: Gtk.Align.START,
  });
  outTitle.add_css_class("prefs-group-title");
  soundPage.append(outTitle);

  const outGroup = makeGroupBox();

  const outDevRow = makeRow(false);
  const outDevLabel = new Gtk.Label({
    label: "Output Device",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  outDevLabel.add_css_class("prefs-row-title");
  const outDropdown = Gtk.DropDown.new_from_strings(["No Output Device"]);
  outDevRow.append(outDevLabel);
  outDevRow.append(outDropdown);

  const outVolRow = makeRow(true);
  const outVolLabel = new Gtk.Label({
    label: "Output Volume",
    halign: Gtk.Align.START,
  });
  outVolLabel.add_css_class("prefs-row-title");

  const outMuteIcon = new Gtk.Image({
    icon_name: "audio-volume-medium-symbolic",
  });
  const outMuteBtn = new Gtk.Button({ child: outMuteIcon });
  outMuteBtn.add_css_class("prefs-btn");

  const outScale = Gtk.Scale.new_with_range(
    Gtk.Orientation.HORIZONTAL,
    0,
    1,
    0.01,
  );
  outScale.set_hexpand(true);
  outScale.set_draw_value(false);

  const outPctLabel = new Gtk.Label({
    label: "0%",
    width_chars: 4,
    halign: Gtk.Align.END,
  });
  outPctLabel.add_css_class("prefs-muted");

  outVolRow.append(outVolLabel);
  outVolRow.append(outMuteBtn);
  outVolRow.append(outScale);
  outVolRow.append(outPctLabel);

  outGroup.append(outDevRow);
  outGroup.append(outVolRow);
  soundPage.append(outGroup);

  // --- Input Section ---
  const inTitle = new Gtk.Label({
    label: "Input",
    halign: Gtk.Align.START,
  });
  inTitle.add_css_class("prefs-group-title");
  soundPage.append(inTitle);

  const inGroup = makeGroupBox();

  const inDevRow = makeRow(false);
  const inDevLabel = new Gtk.Label({
    label: "Input Device",
    hexpand: true,
    halign: Gtk.Align.START,
  });
  inDevLabel.add_css_class("prefs-row-title");
  const inDropdown = Gtk.DropDown.new_from_strings(["No Input Device"]);
  inDevRow.append(inDevLabel);
  inDevRow.append(inDropdown);

  const inVolRow = makeRow(true);
  const inVolLabel = new Gtk.Label({
    label: "Input Volume",
    halign: Gtk.Align.START,
  });
  inVolLabel.add_css_class("prefs-row-title");

  const inMuteIcon = new Gtk.Image({
    icon_name: "microphone-sensitivity-medium-symbolic",
  });
  const inMuteBtn = new Gtk.Button({ child: inMuteIcon });
  inMuteBtn.add_css_class("prefs-btn");

  const inScale = Gtk.Scale.new_with_range(
    Gtk.Orientation.HORIZONTAL,
    0,
    1,
    0.01,
  );
  inScale.set_hexpand(true);
  inScale.set_draw_value(false);

  const inPctLabel = new Gtk.Label({
    label: "0%",
    width_chars: 4,
    halign: Gtk.Align.END,
  });
  inPctLabel.add_css_class("prefs-muted");

  inVolRow.append(inVolLabel);
  inVolRow.append(inMuteBtn);
  inVolRow.append(inScale);
  inVolRow.append(inPctLabel);

  inGroup.append(inDevRow);
  inGroup.append(inVolRow);
  soundPage.append(inGroup);

  if (wp) {
    const audio = wp.audio;
    let speakers: AstalWp.Endpoint[] = [];
    let mics: AstalWp.Endpoint[] = [];
    let syncingOut = false;
    let syncingIn = false;
    let currentSpeaker: AstalWp.Endpoint | null = null;
    let speakerSignals: number[] = [];
    let currentMic: AstalWp.Endpoint | null = null;
    let micSignals: number[] = [];

    const syncSpeakerValues = () => {
      syncingOut = true;
      if (currentSpeaker) {
        outScale.set_value(currentSpeaker.volume);
        outPctLabel.label = `${Math.round(currentSpeaker.volume * 100)}%`;
        outMuteIcon.icon_name =
          currentSpeaker.volumeIcon || "audio-volume-medium-symbolic";
        outVolRow.sensitive = true;
      } else {
        outScale.set_value(0);
        outPctLabel.label = "0%";
        outVolRow.sensitive = false;
      }
      syncingOut = false;
    };

    const bindDefaultSpeaker = () => {
      if (currentSpeaker) {
        for (const id of speakerSignals) currentSpeaker.disconnect(id);
        speakerSignals = [];
      }
      currentSpeaker = audio.defaultSpeaker;
      if (currentSpeaker) {
        speakerSignals.push(
          currentSpeaker.connect("notify::volume", syncSpeakerValues),
          currentSpeaker.connect("notify::mute", syncSpeakerValues),
          currentSpeaker.connect("notify::volume-icon", syncSpeakerValues),
        );
      }
      syncSpeakerValues();
    };

    const syncSpeakersList = () => {
      syncingOut = true;
      speakers = [...audio.speakers];
      if (speakers.length === 0) {
        outDropdown.model = Gtk.StringList.new(["No Output Device"]);
        outDropdown.sensitive = false;
      } else {
        outDropdown.model = Gtk.StringList.new(
          speakers.map((s) => s.description || s.name || `Device ${s.id}`),
        );
        const defIdx = speakers.findIndex((s) => s.isDefault);
        outDropdown.selected = defIdx >= 0 ? defIdx : 0;
        outDropdown.sensitive = true;
      }
      syncingOut = false;
    };

    outDropdown.connect("notify::selected", () => {
      if (syncingOut) return;
      const target = speakers[outDropdown.selected];
      if (target) target.isDefault = true;
    });

    outScale.connect("value-changed", () => {
      if (syncingOut || !currentSpeaker) return;
      const val = outScale.get_value();
      currentSpeaker.volume = val;
      if (currentSpeaker.mute && val > 0) currentSpeaker.mute = false;
    });

    outMuteBtn.connect("clicked", () => {
      if (currentSpeaker) currentSpeaker.mute = !currentSpeaker.mute;
    });

    const syncMicValues = () => {
      syncingIn = true;
      if (currentMic) {
        inScale.set_value(currentMic.volume);
        inPctLabel.label = `${Math.round(currentMic.volume * 100)}%`;
        inMuteIcon.icon_name = currentMic.mute
          ? "microphone-sensitivity-muted-symbolic"
          : "microphone-sensitivity-medium-symbolic";
        inVolRow.sensitive = true;
      } else {
        inScale.set_value(0);
        inPctLabel.label = "0%";
        inVolRow.sensitive = false;
      }
      syncingIn = false;
    };

    const bindDefaultMic = () => {
      if (currentMic) {
        for (const id of micSignals) currentMic.disconnect(id);
        micSignals = [];
      }
      currentMic = audio.defaultMicrophone;
      if (currentMic) {
        micSignals.push(
          currentMic.connect("notify::volume", syncMicValues),
          currentMic.connect("notify::mute", syncMicValues),
        );
      }
      syncMicValues();
    };

    const syncMicsList = () => {
      syncingIn = true;
      mics = [...audio.microphones];
      if (mics.length === 0) {
        inDropdown.model = Gtk.StringList.new(["No Input Device"]);
        inDropdown.sensitive = false;
      } else {
        inDropdown.model = Gtk.StringList.new(
          mics.map((m) => m.description || m.name || `Device ${m.id}`),
        );
        const defIdx = mics.findIndex((m) => m.isDefault);
        inDropdown.selected = defIdx >= 0 ? defIdx : 0;
        inDropdown.sensitive = true;
      }
      syncingIn = false;
    };

    inDropdown.connect("notify::selected", () => {
      if (syncingIn) return;
      const target = mics[inDropdown.selected];
      if (target) target.isDefault = true;
    });

    inScale.connect("value-changed", () => {
      if (syncingIn || !currentMic) return;
      const val = inScale.get_value();
      currentMic.volume = val;
      if (currentMic.mute && val > 0) currentMic.mute = false;
    });

    inMuteBtn.connect("clicked", () => {
      if (currentMic) currentMic.mute = !currentMic.mute;
    });

    syncSpeakersList();
    bindDefaultSpeaker();
    syncMicsList();
    bindDefaultMic();

    audio.connect("speaker-added", syncSpeakersList);
    audio.connect("speaker-removed", syncSpeakersList);
    audio.connect("notify::default-speaker", () => {
      syncSpeakersList();
      bindDefaultSpeaker();
    });
    audio.connect("microphone-added", syncMicsList);
    audio.connect("microphone-removed", syncMicsList);
    audio.connect("notify::default-microphone", () => {
      syncMicsList();
      bindDefaultMic();
    });
  }

  return soundPage;
}
