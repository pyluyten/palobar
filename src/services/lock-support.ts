import GLib from "gi://GLib";
import type Auth from "gi://AstalAuth";
import type SessionLock from "gi://Gtk4SessionLock";

type LockSupport =
  | { available: true; auth: typeof Auth; sessionLock: typeof SessionLock }
  | { available: false; reason: string };

export async function checkLockSupport(): Promise<LockSupport> {
  const auth = await import("gi://AstalAuth").catch(() => null);
  if (!auth?.default.Pam) {
    return {
      available: false,
      reason: "Install astal-auth and configure its PAM service to enable the lock screen.",
    };
  }

  const pamPaths = ["/etc/pam.d/astal-auth", "/usr/lib/pam.d/astal-auth"];
  if (!pamPaths.some((path) => GLib.file_test(path, GLib.FileTest.IS_REGULAR))) {
    return {
      available: false,
      reason: "Configure the astal-auth PAM service (for example, /etc/pam.d/astal-auth) to enable the lock screen.",
    };
  }

  const sessionLock = await import("gi://Gtk4SessionLock").catch(() => null);
  if (!sessionLock?.default.is_supported()) {
    return {
      available: false,
      reason: "The lock screen requires GTK4 Session Lock and a Wayland compositor that supports session locking.",
    };
  }

  return { available: true, auth: auth.default, sessionLock: sessionLock.default };
}
