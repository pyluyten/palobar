import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";

export interface ClipboardEntry {
  id: string;
  preview: string;
  isImage: boolean;
}

let watchProc: Gio.Subprocess | null = null;
let watchError: string | null = null;

export function getClipboardWatchError(): string | null {
  return watchError;
}

export function initClipboardService() {
  if (watchProc) return;
  watchError = null;
  const wlPaste = GLib.find_program_in_path("wl-paste");
  const cliphist = GLib.find_program_in_path("cliphist");
  if (!wlPaste || !cliphist) {
    const missing = [!wlPaste && "wl-paste", !cliphist && "cliphist"]
      .filter(Boolean)
      .join(", ");
    watchError = `Clipboard history requires ${missing} in Palobar's PATH.`;
    console.error("[clipboard]", watchError);
    return;
  }

  try {
    const proc = Gio.Subprocess.new(
      [wlPaste, "--watch", cliphist, "store"],
      Gio.SubprocessFlags.STDERR_PIPE,
    );
    watchProc = proc;
    proc.communicate_utf8_async(null, null, (_proc, result) => {
      if (watchProc !== proc) return;
      watchProc = null;
      try {
        const [, , stderr] = proc.communicate_utf8_finish(result);
        const detail = stderr?.trim();
        watchError = detail?.includes("data-control")
          ? "Clipboard history requires Wayland data-control. Start Palobar directly in your niri session."
          : "Clipboard history capture stopped. Check Palobar's logs.";
        console.error("[clipboard] wl-paste watcher stopped:", detail);
      } catch (err) {
        watchError = "Clipboard history capture stopped. Check Palobar's logs.";
        console.error("[clipboard] failed to monitor wl-paste:", err);
      }
    });
  } catch (err) {
    watchError = "Could not start clipboard history capture. Check Palobar's logs.";
    console.error("[clipboard] failed to start wl-paste watcher:", err);
    watchProc = null;
  }
}

export function stopClipboardService() {
  if (watchProc) {
    try {
      watchProc.force_exit();
    } catch {
      // ignore
    }
    watchProc = null;
  }
}

export async function listClipboardEntries(
  limit = 25,
): Promise<ClipboardEntry[]> {
  try {
    const out = await execAsync(["cliphist", "list"]);
    const lines = out
      .split("\n")
      .map((l) => l.trimEnd())
      .filter((l) => l.length > 0);

    const entries: ClipboardEntry[] = [];
    for (const line of lines.slice(0, limit)) {
      const tabIdx = line.indexOf("\t");
      if (tabIdx <= 0) continue;
      const id = line.slice(0, tabIdx);
      const preview = line.slice(tabIdx + 1);
      const isImage = /^\[\[ binary data .* (png|jpe?g|webp|bmp|gif)/i.test(
        preview,
      );
      entries.push({ id, preview, isImage });
    }
    return entries;
  } catch {
    return [];
  }
}

export function decodeClipboardBytes(id: string): Promise<GLib.Bytes | null> {
  return new Promise((resolve) => {
    try {
      const proc = Gio.Subprocess.new(
        ["cliphist", "decode", id],
        Gio.SubprocessFlags.STDOUT_PIPE,
      );
      proc.communicate_async(null, null, (_p, res) => {
        try {
          const [ok, stdout] = proc.communicate_finish(res);
          resolve(ok ? stdout : null);
        } catch {
          resolve(null);
        }
      });
    } catch {
      resolve(null);
    }
  });
}

export async function copyClipboardEntry(id: string): Promise<void> {
  try {
    await execAsync(["sh", "-c", `cliphist decode ${id} | wl-copy`]);
  } catch (err) {
    console.error("[clipboard] failed to copy entry:", id, err);
  }
}

export async function deleteClipboardEntry(id: string): Promise<void> {
  try {
    await execAsync(["sh", "-c", `printf '%s\t' '${id}' | cliphist delete`]);
  } catch (err) {
    console.error("[clipboard] failed to delete entry:", id, err);
  }
}

export async function wipeClipboard(): Promise<void> {
  try {
    await execAsync(["cliphist", "wipe"]);
  } catch (err) {
    console.error("[clipboard] failed to wipe history:", err);
  }
}
