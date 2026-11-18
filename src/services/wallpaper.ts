import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { configService, type PalobarConfig } from "./config";

let currentProc: Gio.Subprocess | null = null;
let lastKey = "";

function stopWallpaper() {
  if (currentProc) {
    try {
      currentProc.force_exit();
    } catch {
      // ignore
    }
    currentProc = null;
  }
}

function applyWallpaper(cfg: PalobarConfig) {
  const { path, mode, color } = cfg.wallpaper;
  const key = `${path}|${mode}|${color}`;
  if (key === lastKey && currentProc) return;
  lastKey = key;

  stopWallpaper();

  const args = ["swaybg", "-c", color || "#131316"];
  if (path && GLib.file_test(path, GLib.FileTest.EXISTS)) {
    args.push("-m", mode || "fill", "-i", path);
  }

  try {
    currentProc = Gio.Subprocess.new(args, Gio.SubprocessFlags.NONE);
  } catch (err) {
    console.error("[wallpaper] failed to start swaybg:", err);
    currentProc = null;
  }
}

export function initWallpaperService() {
  configService.subscribe(applyWallpaper);
}
