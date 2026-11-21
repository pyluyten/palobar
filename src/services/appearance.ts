import Gio from "gi://Gio";

export const ACCENT_COLORS = [
  { id: "blue", name: "Blue", hex: "#3584e4" },
  { id: "teal", name: "Teal", hex: "#2190a4" },
  { id: "green", name: "Green", hex: "#3a944a" },
  { id: "yellow", name: "Yellow", hex: "#c88800" },
  { id: "orange", name: "Orange", hex: "#ed5b00" },
  { id: "red", name: "Red", hex: "#e62d42" },
  { id: "pink", name: "Pink", hex: "#d56199" },
  { id: "purple", name: "Purple", hex: "#9141ac" },
  { id: "slate", name: "Slate", hex: "#6f8396" },
] as const;

type AccentColor = typeof ACCENT_COLORS[number];
type AppearanceKey = "color-scheme" | "accent-color";

const schema = Gio.SettingsSchemaSource.get_default()?.lookup(
  "org.gnome.desktop.interface",
  true,
);
const settings = schema ? new Gio.Settings({ settings_schema: schema }) : null;

export function canSetAppearance(key: AppearanceKey): boolean {
  return Boolean(schema?.has_key(key) && settings?.is_writable(key));
}

export function isDarkStyle(): boolean {
  return Boolean(schema?.has_key("color-scheme") &&
    settings?.get_string("color-scheme") === "prefer-dark");
}

export function setDarkStyle(dark: boolean): void {
  if (canSetAppearance("color-scheme")) {
    settings?.set_string("color-scheme", dark ? "prefer-dark" : "default");
  }
}

export function getAccentColor(): AccentColor {
  const id = schema?.has_key("accent-color") ? settings?.get_string("accent-color") : undefined;
  return ACCENT_COLORS.find((color) => color.id === id) ?? ACCENT_COLORS[0];
}

export function setAccentColor(id: AccentColor["id"]): void {
  if (canSetAppearance("accent-color")) settings?.set_string("accent-color", id);
}

export function subscribeAppearance(key: AppearanceKey, listener: () => void): () => void {
  // GSettings emits changed only for keys that have been read with a handler connected.
  const id = schema?.has_key(key) ? settings?.connect(`changed::${key}`, listener) : undefined;
  listener();
  return () => {
    if (id !== undefined) settings?.disconnect(id);
  };
}
