import { createState } from "ags";

export type PopupName =
  | "calendar"
  | "quick-settings"
  | "notifications"
  | "clipboard"
  | null;

const [activePopup, setActivePopup] = createState<PopupName>(null);

export const popups = {
  active: activePopup,
  toggle(name: Exclude<PopupName, null>) {
    setActivePopup((current) => (current === name ? null : name));
  },
  open(name: Exclude<PopupName, null>) {
    setActivePopup(name);
  },
  close() {
    setActivePopup(null);
  },
};
