import { execAsync } from "ags/process";
import GLib from "gi://GLib";
import { configService } from "./config";

function stripHash(hex: string): string {
  return hex.startsWith("#") ? hex.slice(1) : hex;
}

export async function lockScreen() {
  const cfg = configService.get();
  const imagePath = cfg.lockscreen.image || cfg.wallpaper.path;
  const bgColor = stripHash(cfg.lockscreen.color || "#131316");

  const args = [
    "swaylock",
    "--ignore-empty-password",
    "--show-failed-attempts",
    "--indicator-radius",
    "80",
    "--indicator-thickness",
    "8",
    "--font",
    "Cantarell",
    "--color",
    bgColor,
    "--inside-color",
    "1e1e22cc",
    "--inside-clear-color",
    "1e1e22cc",
    "--inside-ver-color",
    "3584e4cc",
    "--inside-wrong-color",
    "e01b24cc",
    "--ring-color",
    "ffffff44",
    "--ring-clear-color",
    "ffffff88",
    "--ring-ver-color",
    "3584e4ff",
    "--ring-wrong-color",
    "e01b24ff",
    "--key-hl-color",
    "3584e4ff",
    "--bs-hl-color",
    "e01b24ff",
    "--line-color",
    "00000000",
    "--separator-color",
    "00000000",
    "--text-color",
    "f6f5f4ff",
    "--text-ver-color",
    "f6f5f4ff",
    "--text-wrong-color",
    "f6f5f4ff",
  ];

  if (imagePath && GLib.file_test(imagePath, GLib.FileTest.EXISTS)) {
    args.push("--image", imagePath, "--scaling", "fill");
  }

  try {
    await execAsync(args);
  } catch (err) {
    console.error("[lockscreen] failed to launch swaylock:", err);
  }
}
