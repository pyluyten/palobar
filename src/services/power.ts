import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { configService, type PalobarConfig } from "./config";

let idleProc: Gio.Subprocess | null = null;
let lastKey = "";

export function stopPowerService() {
  if (idleProc) {
    try {
      idleProc.force_exit();
    } catch {
      // ignore
    }
    idleProc = null;
  }
}

function applyPowerConfig(cfg: PalobarConfig) {
  const {
    screenBlankEnabled,
    screenBlankMinutes,
    autoSuspendEnabled,
    autoSuspendMinutes,
  } = cfg.power;

  const key = `${screenBlankEnabled}|${screenBlankMinutes}|${autoSuspendEnabled}|${autoSuspendMinutes}`;
  if (key === lastKey && (idleProc !== null || (!screenBlankEnabled && !autoSuspendEnabled))) {
    return;
  }
  lastKey = key;

  stopPowerService();

  if (!screenBlankEnabled && !autoSuspendEnabled) {
    return;
  }

  const swayidleBin = GLib.find_program_in_path("swayidle");
  if (!swayidleBin) {
    console.error("[power] swayidle not found in PATH");
    return;
  }

  const args = [swayidleBin, "-w"];

  if (screenBlankEnabled && screenBlankMinutes > 0) {
    const secs = String(Math.round(screenBlankMinutes * 60));
    args.push(
      "timeout",
      secs,
      "niri msg action power-off-monitors",
      "resume",
      "niri msg action power-on-monitors",
    );
  }

  if (autoSuspendEnabled && autoSuspendMinutes > 0) {
    const secs = String(Math.round(autoSuspendMinutes * 60));
    args.push("timeout", secs, "systemctl suspend");
  }

  try {
    idleProc = Gio.Subprocess.new(args, Gio.SubprocessFlags.NONE);
  } catch (err) {
    console.error("[power] failed to start swayidle:", err);
    idleProc = null;
  }
}

export function initPowerService() {
  configService.subscribe(applyPowerConfig);
}
