import AstalApps from "gi://AstalApps"
import GLib from "gi://GLib"
import System from "system"

let apps: AstalApps.Apps | null = null
export const getApps = () => (apps ??= new AstalApps.Apps())

let gcTimer = 0
export function scheduleGc() {
    if (gcTimer) GLib.source_remove(gcTimer)
    gcTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1500, () => {
        gcTimer = 0
        System.gc()
        return GLib.SOURCE_REMOVE
    })
}
