import { Gdk, Gtk } from "ags/gtk4";
import { popups } from "../services/popups";

export function PopupDismissArea({ children }: { children: Gtk.Widget }) {
  return (
    <box
      hexpand
      vexpand
      $={(area) => {
        const click = new Gtk.GestureClick({ button: Gdk.BUTTON_PRIMARY });
        click.set_propagation_phase(Gtk.PropagationPhase.CAPTURE);
        click.connect("pressed", (gesture, _count, x, y) => {
          if (area.pick(x, y, Gtk.PickFlags.DEFAULT) === area) {
            gesture.set_state(Gtk.EventSequenceState.CLAIMED);
            popups.close();
          }
        });
        area.add_controller(click);
      }}
    >
      {children}
    </box>
  );
}
