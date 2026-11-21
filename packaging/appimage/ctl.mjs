import Gio from "gi://Gio";
import GLib from "gi://GLib";
import System from "system";

try {
    const reply = Gio.DBus.session.call_sync(
        "io.Astal.ags", "/io/Astal/Application", "io.Astal.Application", "Request",
        new GLib.Variant("(as)", [ARGV]), new GLib.VariantType("(s)"),
        Gio.DBusCallFlags.NO_AUTO_START, -1, null,
    );
    print(reply.deep_unpack()[0]);
} catch (error) {
    if (ARGV[0] === "quit" && error instanceof GLib.Error &&
        Gio.DBusError.get_remote_error(error) === "org.freedesktop.DBus.Error.NoReply") {
        print("ok");
    } else if (ARGV[0] === "run-command") {
        const process = Gio.Subprocess.new(["palobar-run-command"], Gio.SubprocessFlags.NONE);
        System.exit(process.wait_check(null) ? 0 : 1);
    } else {
        printerr(error.message);
        System.exit(1);
    }
}
