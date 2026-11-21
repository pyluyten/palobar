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
// Gestion mémoire :
//   - un scope gnim (createRoot) par fenêtre, libéré à la destruction
//   - une seule texture de fond partagée entre tous les écrans
//   - GC forcé peu après le déverrouillage (la mémoire native des textures
//     n'exerce aucune pression sur le GC de SpiderMonkey)
//
// Utilisation : appeler lockScreen() depuis ton requestHandler.

import app from "ags/gtk4/app"
import { Gtk, Gdk } from "ags/gtk4"
import { createState, createRoot } from "ags"
import { createPoll } from "ags/time"
import GLib from "gi://GLib"
import Gio from "gi://Gio"
import type Auth from "gi://AstalAuth"
import System from "system"
import type SessionLock from "gi://Gtk4SessionLock"

import { lockCss } from "./lock-style"
import { checkLockSupport } from "./lock-support"
import { DEFAULT_CONFIG, loadConfig, type PalobarConfig } from "./config"

// ---------------------------------------------------------------- config ---

const AVATAR = `${GLib.get_home_dir()}/.face` // optionnel
const USER_NAME = GLib.get_real_name() === "Unknown" ? GLib.get_user_name() : GLib.get_real_name()

// ----------------------------------------------------------------- état ----

type Stage = "clock" | "prompt"

type LockWin = {
    win: Gtk.Window
    picture: Gtk.Picture | null
    background: Gtk.CssProvider
    dispose: () => void
}

const [stage, setStage] = createState<Stage>("clock")
const [busy, setBusy] = createState(false)
const [error, setError] = createState("")

const time = createPoll("", 1000, () => GLib.DateTime.new_now_local().format("%H:%M")!)
const date = createPoll("", 30_000, () => {
    const s = GLib.DateTime.new_now_local().format("%A %-d %B")!
    return s.charAt(0).toUpperCase() + s.slice(1)
})

let lock: SessionLock.Instance | null = null
let lockAuth: typeof Auth | null = null
let monitorsChangedId: number | null = null
let bgTexture: Gdk.Texture | null = null // partagée entre tous les écrans
const windows = new Map<Gdk.Monitor, LockWin>()
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
    const auth = lockAuth
    if (!auth || busy.get() || password.length === 0) return
    setBusy(true)
    setError("")

    // Utilise le service PAM "astal-auth" (cf. /etc/pam.d/astal-auth)
    auth.Pam.authenticate(password, (_, task) => {
        try {
            auth.Pam.authenticate_finish(task)
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

function LockWindow(monitor: Gdk.Monitor): LockWin {
    // Tout ce qui est créé ici (bindings, abonnements aux états globaux)
    // est rattaché à ce scope et coupé par dispose().
    return createRoot((dispose) => {
        let entry!: Gtk.PasswordEntry
        let picture: Gtk.Picture | null = null

        const cfg = loadConfig()
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
                    {bgTexture ? (
                        <Gtk.Picture
                            $={(self) => (picture = self)}
                            class={stage((s) => (s === "prompt" ? "lock-bg prompt" : "lock-bg"))}
                            contentFit={Gtk.ContentFit.COVER}
                            paintable={bgTexture}
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

        return { win, picture, background, dispose }
    })
}

function destroyWindow(w: LockWin) {
    w.picture?.set_paintable(null) // lâche la référence à la texture
    w.win.get_style_context().remove_provider(w.background)
    w.dispose() // coupe les abonnements gnim (états globaux, polls)
    w.win.destroy()
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
            destroyWindow(w)
            windows.delete(m)
        }
    }
}

// ------------------------------------------------------------ API publique -

let css_applied = false

let lockStarting = false

function fallbackColor(cfg: PalobarConfig): Gdk.RGBA {
    const color = new Gdk.RGBA()
    if (!color.parse(cfg.lockscreen.color)) color.parse(DEFAULT_CONFIG.lockscreen.color)
    return color
}

export async function lockScreen(): Promise<string | null> {
    if (lock || lockStarting) return null

    lockStarting = true
    try {
        const support = await checkLockSupport()
        if (!support.available) return support.reason
        lockAuth = support.auth
        startNativeLock(support.sessionLock)
        return null
    } catch (err) {
        console.error("[lockscreen] failed to start:", err)
        return "Could not start the lock screen."
    } finally {
        lockStarting = false
    }
}

// Charge le fond UNE fois, partagé par tous les écrans.
function loadBackground(): Gdk.Texture | null {
    const cfg = loadConfig()
    const path = cfg.lockscreen.image || cfg.wallpaper.path
    if (!path || !GLib.file_test(path, GLib.FileTest.EXISTS)) return null
    try {
        return Gdk.Texture.new_from_filename(path)
    } catch (err) {
        console.error("[lockscreen] failed to load background:", err)
        return null
    }
}

function startNativeLock(sessionLock: typeof SessionLock) {
    if (!css_applied) {
        app.apply_css(lockCss)
        css_applied = true
    }

    setStage("clock")
    setError("")
    bgTexture = loadBackground()

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

function gcSoon() {
    GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1000, () => {
        System.gc()
        return GLib.SOURCE_REMOVE
    })
}

function cleanup() {
    if (!lock) return // déjà nettoyé (unlock() puis signal "unlocked")

    if (monitorsChangedId !== null) {
        Gdk.Display.get_default()?.get_monitors().disconnect(monitorsChangedId)
        monitorsChangedId = null
    }
    windows.forEach(destroyWindow)
    windows.clear()
    focusers.clear()
    bgTexture = null
    lock = null
    lockAuth = null
    setStage("clock")
    gcSoon()
}

