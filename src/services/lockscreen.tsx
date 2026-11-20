// lockscreen.tsx — lock screen "à la GNOME" pour AGS v3 (gnim) + GTK4
//
// Dépendances système :
//   - gtk4-session-lock  (typelib Gtk4SessionLock) -> protocole ext-session-lock-v1
//   - astal-auth         (typelib AstalAuth)       -> authentification PAM
//
// Deux étapes, comme GNOME :
//   "clock"  : fond net, grosse horloge + date
//   "prompt" : fond flouté/assombri, avatar + nom + champ de mot de passe
//
// Utilisation : appeler lock() depuis ton requestHandler (voir app.tsx).

import app from "ags/gtk4/app"
import { Gtk, Gdk } from "ags/gtk4"
import { createState } from "ags"
import { createPoll } from "ags/time"
import GLib from "gi://GLib"
import Gio from "gi://Gio"
import Auth from "gi://AstalAuth"
import type SessionLock from "gi://Gtk4SessionLock"

import { lockCss } from "./lock-style"
import { DEFAULT_CONFIG, loadConfig, type PalobarConfig } from "./config"

// ---------------------------------------------------------------- config ---

const AVATAR = `${GLib.get_home_dir()}/.face` // optionnel
const USER_NAME = GLib.get_real_name() === "Unknown" ? GLib.get_user_name() : GLib.get_real_name()

// ----------------------------------------------------------------- état ----

type Stage = "clock" | "prompt"

const [stage, setStage] = createState<Stage>("clock")
const [busy, setBusy] = createState(false)
const [error, setError] = createState("")

const time = createPoll("", 1000, () => GLib.DateTime.new_now_local().format("%H:%M")!)
const date = createPoll("", 30_000, () => {
    const s = GLib.DateTime.new_now_local().format("%A %-d %B")!
    return s.charAt(0).toUpperCase() + s.slice(1)
})

let lock: SessionLock.Instance | null = null
let monitorsChangedId: number | null = null
const windows = new Map<Gdk.Monitor, Gtk.Window>()
// callbacks qui donnent le focus aux champs de saisie quand on passe en "prompt"
const focusers = new Set<() => void>()

stage.subscribe(() => {
    if (stage.get() === "prompt") GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
        focusers.forEach((f) => f())
        return GLib.SOURCE_REMOVE
    })
})

// ------------------------------------------------------------------ auth ---

function tryUnlock(password: string, entry: Gtk.PasswordEntry) {
    if (busy.get() || password.length === 0) return
    setBusy(true)
    setError("")

    // Utilise le service PAM "astal-auth" (cf. /etc/pam.d/astal-auth)
    Auth.Pam.authenticate(password, (_, task) => {
        try {
            Auth.Pam.authenticate_finish(task)
            unlock()
        } catch (_e) {
            setError("Incorrect password")
            entry.set_text("")
            entry.grab_focus()
        } finally {
            setBusy(false)
        }
    })
}

// ----------------------------------------------------------- une fenêtre ---

function LockWindow(monitor: Gdk.Monitor): Gtk.Window {
    let entry!: Gtk.PasswordEntry

    const cfg = loadConfig()
    const bgPath = cfg.lockscreen.image || cfg.wallpaper.path
    const hasBgImage = Boolean(bgPath && GLib.file_test(bgPath, GLib.FileTest.EXISTS))
    const avatar = GLib.file_test(AVATAR, GLib.FileTest.EXISTS)

    const win = (
        <Gtk.Window
            class="lock-window"
            $={(self) => {
                // N'importe quelle touche / clic sur l'horloge -> prompt ; Échap -> retour
                const keys = new Gtk.EventControllerKey()
                keys.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
                keys.connect("key-pressed", (_c, keyval) => {
                    if (stage.get() === "clock") {
                        setStage("prompt")
                        return true
                    }
                    if (keyval === Gdk.KEY_Escape) {
                        setStage("clock")
                        return true
                    }
                    return false
                })
                self.add_controller(keys)

                const click = new Gtk.GestureClick()
                click.connect("pressed", () => {
                    if (stage.get() === "clock") setStage("prompt")
                })
                self.add_controller(click)
            }}
        >
            <Gtk.Overlay>
                {/* Fond : le flou est piloté par la classe CSS "prompt" */}
                {hasBgImage ? (
                    <Gtk.Picture
                        class={stage((s) => (s === "prompt" ? "lock-bg prompt" : "lock-bg"))}
                        contentFit={Gtk.ContentFit.COVER}
                        file={Gio.File.new_for_path(bgPath)}
                        hexpand
                        vexpand
                    />
                ) : (
                    <Gtk.Box
                        class={stage((s) => (s === "prompt" ? "lock-bg prompt" : "lock-bg"))}
                        hexpand
                        vexpand
                    />
                )}

                <Gtk.Stack
                    $type="overlay"
                    class="lock-stack"
                    transitionType={Gtk.StackTransitionType.CROSSFADE}
                    transitionDuration={350}
                    visibleChildName={stage}
                >
                    {/* ---- Étape 1 : horloge ---- */}
                    <Gtk.Box
                        $type="named"
                        name="clock"
                        orientation={Gtk.Orientation.VERTICAL}
                        valign={Gtk.Align.CENTER}
                        halign={Gtk.Align.CENTER}
                        class="clock-page"
                    >
                        <Gtk.Label class="lock-time" label={time} />
                        <Gtk.Label class="lock-date" label={date} />
                    </Gtk.Box>

                    {/* ---- Étape 2 : mot de passe ---- */}
                    <Gtk.Box
                        $type="named"
                        name="prompt"
                        orientation={Gtk.Orientation.VERTICAL}
                        valign={Gtk.Align.CENTER}
                        halign={Gtk.Align.CENTER}
                        spacing={14}
                        class="prompt-page"
                    >
                        <Gtk.Box class="lock-avatar" halign={Gtk.Align.CENTER} overflow={Gtk.Overflow.HIDDEN}>
                            {avatar ? (
                                <Gtk.Picture
                                    file={Gio.File.new_for_path(AVATAR)}
                                    contentFit={Gtk.ContentFit.COVER}
                                    widthRequest={96}
                                    heightRequest={96}
                                />
                            ) : (
                                <Gtk.Image iconName="avatar-default-symbolic" pixelSize={56} widthRequest={96} heightRequest={96} />
                            )}
                        </Gtk.Box>

                        <Gtk.Label class="lock-username" label={USER_NAME} />

                        <Gtk.PasswordEntry
                            $={(self) => {
                                entry = self
                                focusers.add(() => self.grab_focus())
                            }}
                            class="lock-entry"
                            placeholderText="Password"
                            showPeekIcon
                            widthRequest={260}
                            sensitive={busy((b) => !b)}
                            onActivate={(self) => tryUnlock(self.get_text(), self)}
                        />

                        <Gtk.Label class="lock-error" label={error} visible={error((e) => e !== "")} />
                    </Gtk.Box>
                </Gtk.Stack>
            </Gtk.Overlay>
        </Gtk.Window>
    ) as Gtk.Window

    const color = fallbackColor(cfg)
    const background = new Gtk.CssProvider()
    background.load_from_string(`window.lock-window { background-color: ${color.to_string()}; }`)
    win.get_style_context().add_provider(background, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION + 1)

    // Le plus important : on associe la fenêtre à l'écran via ext-session-lock
    lock!.assign_window_to_monitor(win, monitor)
    win.present()
    return win
}

function syncWindows() {
    if (!lock) return

    // nouveaux écrans
    const monitors = Gdk.Display.get_default()!.get_monitors()
    for (let i = 0; i < monitors.get_n_items(); i++) {
        const m = monitors.get_item(i) as Gdk.Monitor
        if (!windows.has(m)) windows.set(m, LockWindow(m))
    }

    // écrans débranchés
    for (const [m, w] of windows) {
        if (!m.is_valid()) {
            w.destroy()
            windows.delete(m)
        }
    }
}

// ------------------------------------------------------------ API publique -

let css_applied = false

let lockStarting = false
let swaylockProcess: Gio.Subprocess | null = null

function fallbackColor(cfg: PalobarConfig): Gdk.RGBA {
    const color = new Gdk.RGBA()
    if (!color.parse(cfg.lockscreen.color)) color.parse(DEFAULT_CONFIG.lockscreen.color)
    return color
}

function startSwaylock() {
    const cfg = loadConfig()
    const { red, green, blue } = fallbackColor(cfg)
    const color = [red, green, blue]
        .map((value) => Math.round(value * 255).toString(16).padStart(2, "0"))
        .join("")
    const args = ["swaylock", "-c", color]
    const image = cfg.lockscreen.image || cfg.wallpaper.path
    if (image && GLib.file_test(image, GLib.FileTest.IS_REGULAR)) {
        args.push("-i", image, "-s", "fill")
    }

    const process = Gio.Subprocess.new(args, Gio.SubprocessFlags.NONE)
    swaylockProcess = process
    process.wait_check_async(null, (_process, result) => {
        try {
            process.wait_check_finish(result)
        } catch (err) {
            console.error("[lockscreen] swaylock failed:", err)
        } finally {
            swaylockProcess = null
        }
    })
}

export async function lockScreen() {
    if (lock || lockStarting || swaylockProcess) return

    lockStarting = true
    try {
        const native = await import("gi://Gtk4SessionLock").catch(() => null)
        if (native?.default.is_supported()) {
            startNativeLock(native.default)
        } else {
            startSwaylock()
        }
    } catch (err) {
        console.error("[lockscreen] failed to start:", err)
    } finally {
        lockStarting = false
    }
}

function startNativeLock(sessionLock: typeof SessionLock) {
    if (!css_applied) {
        app.apply_css(lockCss)
        css_applied = true
    }

    setStage("clock")
    setError("")

    lock = sessionLock.Instance.new()

    lock.connect("locked", () => {
        // Le compositeur a bien masqué la session : on peut cacher le reste.
        console.log("session verrouillée")
    })
    // "failed" : un autre locker tourne déjà ; "unlocked" : session déverrouillée / relâchée
    lock.connect("failed", () => cleanup())
    lock.connect("unlocked", () => cleanup())

    lock.lock()

    // Fenêtres créées APRÈS lock(), puis suivi du hotplug
    syncWindows()
    const monitors = Gdk.Display.get_default()!.get_monitors()
    monitorsChangedId = monitors.connect("items-changed", syncWindows)
}

function unlock() {
    lock?.unlock()
    cleanup()
}

function cleanup() {
    if (monitorsChangedId !== null) {
        Gdk.Display.get_default()?.get_monitors().disconnect(monitorsChangedId)
        monitorsChangedId = null
    }
    windows.forEach((w) => w.destroy())
    windows.clear()
    focusers.clear()
    lock = null
    setStage("clock")
}
