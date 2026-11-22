import Gio from "gi://Gio";
import GLib from "gi://GLib";

const HISTORY_LIMIT = 500;
const directory = GLib.build_filenamev([GLib.get_user_state_dir(), "palobar"]);
const history = Gio.File.new_for_path(GLib.build_filenamev([directory, "run-command-history.json"]));

export interface RunHistoryEntry {
  command: string;
  inTerminal: boolean;
  count: number;
}

function matches(error: unknown, code: Gio.IOErrorEnum): boolean {
  return error instanceof GLib.Error && error.matches(Gio.io_error_quark(), code);
}

function readHistory(): { entries: RunHistoryEntry[]; etag: string | null } {
  const [, bytes, etag] = history.load_contents(null);
  const text = new TextDecoder().decode(bytes);
  const data: unknown = text.trim() ? JSON.parse(text) : [];
  if (!Array.isArray(data)) throw new Error("Invalid command history");

  const entries = new Map<string, RunHistoryEntry>();
  for (const item of data) {
    if (!item || typeof item.command !== "string" || typeof item.inTerminal !== "boolean" ||
        !Number.isSafeInteger(item.count) || item.count < 1) continue;
    const command = item.command.trim();
    if (command && !entries.has(command)) entries.set(command, { command, inTerminal: item.inTerminal, count: item.count });
    if (entries.size === HISTORY_LIMIT) break;
  }
  return { entries: [...entries.values()], etag };
}

export function loadRunHistory(): RunHistoryEntry[] {
  try {
    return readHistory().entries;
  } catch (error) {
    if (!matches(error, Gio.IOErrorEnum.NOT_FOUND)) console.error("[run-command] could not read history:", error);
    return [];
  }
}

export function recordRun(command: string, inTerminal: boolean): void {
  try {
    GLib.mkdir_with_parents(directory, 0o700);
    try {
      history.create(Gio.FileCreateFlags.PRIVATE, null).close(null);
    } catch (error) {
      if (!matches(error, Gio.IOErrorEnum.EXISTS)) throw error;
    }

    for (let attempt = 0; attempt < 3; attempt++) {
      const { entries, etag } = readHistory();
      const previous = entries.find((entry) => entry.command === command);
      const count = Math.min((previous?.count ?? 0) + 1, Number.MAX_SAFE_INTEGER);
      const newest = [{ command, inTerminal, count }, ...entries.filter((entry) => entry.command !== command)]
        .slice(0, HISTORY_LIMIT);
      try {
        history.replace_contents(
          new TextEncoder().encode(JSON.stringify(newest, null, 2) + "\n"),
          etag, false, Gio.FileCreateFlags.PRIVATE | Gio.FileCreateFlags.REPLACE_DESTINATION, null,
        );
        return;
      } catch (error) {
        if (!matches(error, Gio.IOErrorEnum.WRONG_ETAG)) throw error;
      }
    }
    throw new Error("Command history changed repeatedly while saving");
  } catch (error) {
    console.error("[run-command] could not save history:", error);
  }
}
