import { Gdk } from "ags/gtk4";
import GdkPixbuf from "gi://GdkPixbuf";
import GLib from "gi://GLib";

const IMAGE_TYPE = 0xfffd0002;

export function readCursor(roots: string[], theme: string, visited = new Set<string>()): Uint8Array | null {
  if (!theme || theme === "." || theme === ".." || theme.includes("/") || visited.has(theme)) return null;
  visited.add(theme);

  const inherited = new Set<string>();
  for (const root of roots) {
    const directory = `${root}/${theme}`;
    for (const name of ["left_ptr", "default"]) {
      const path = `${directory}/cursors/${name}`;
      if (!GLib.file_test(path, GLib.FileTest.IS_REGULAR)) continue;
      try {
        const [, bytes] = GLib.file_get_contents(path);
        if (bytes.byteLength >= 16 && new DataView(bytes.buffer, bytes.byteOffset).getUint32(0, true) === 0x72756358) {
          return bytes;
        }
      } catch {
        // Try the next cursor alias or search directory.
      }
    }

    try {
      const index = new GLib.KeyFile();
      index.load_from_file(`${directory}/index.theme`, GLib.KeyFileFlags.NONE);
      for (const parent of index.get_string("Icon Theme", "Inherits").split(/[,;]/)) {
        if (parent.trim()) inherited.add(parent.trim());
      }
    } catch {
      // Themes can provide cursors without an index.theme file.
    }
  }

  for (const parent of inherited) {
    const cursor = readCursor(roots, parent, visited);
    if (cursor) return cursor;
  }
  return null;
}

export function renderCursor(bytes: Uint8Array, size: number): Gdk.Cursor | null {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const read = (offset: number) => view.getUint32(offset, true);
  const table = read(4);
  const count = read(12);
  if (table < 16 || table + count * 12 > bytes.byteLength) return null;

  let nearest: { position: number; nominal: number } | null = null;
  for (let i = 0; i < count; i++) {
    const entry = table + i * 12;
    if (read(entry) !== IMAGE_TYPE) continue;
    const nominal = read(entry + 4);
    const position = read(entry + 8);
    if (!nominal || position + 36 > bytes.byteLength) continue;
    if (!nearest || Math.abs(nominal - size) < Math.abs(nearest.nominal - size)) {
      nearest = { position, nominal };
    }
  }
  if (!nearest) return null;

  const { position, nominal } = nearest;
  const headerSize = read(position);
  const width = read(position + 16);
  const height = read(position + 20);
  const hotX = read(position + 24);
  const hotY = read(position + 28);
  const start = position + headerSize;
  const end = start + width * height * 4;
  if (headerSize < 36 || read(position + 4) !== IMAGE_TYPE || read(position + 8) !== nominal ||
      !width || !height || hotX >= width || hotY >= height || end > bytes.byteLength) return null;

  let texture: Gdk.Texture = Gdk.MemoryTexture.new(
    width, height, Gdk.MemoryFormat.B8G8R8A8_PREMULTIPLIED,
    new GLib.Bytes(bytes.subarray(start, end)), width * 4,
  );
  if (nominal === size) return Gdk.Cursor.new_from_texture(texture, hotX, hotY, null);

  const scale = size / nominal;
  const scaledWidth = Math.max(1, Math.min(256, Math.round(width * scale)));
  const scaledHeight = Math.max(1, Math.min(256, Math.round(height * scale)));
  const scaled = Gdk.pixbuf_get_from_texture(texture)?.scale_simple(
    scaledWidth, scaledHeight, GdkPixbuf.InterpType.BILINEAR,
  );
  if (!scaled) return null;
  texture = Gdk.Texture.new_for_pixbuf(scaled);
  return Gdk.Cursor.new_from_texture(
    texture,
    Math.min(scaledWidth - 1, Math.round(hotX * scaledWidth / width)),
    Math.min(scaledHeight - 1, Math.round(hotY * scaledHeight / height)),
    null,
  );
}
