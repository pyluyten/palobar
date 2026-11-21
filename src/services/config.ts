import Gio from "gi://Gio";
import GLib from "gi://GLib";

export type WallpaperMode = "fill" | "fit" | "stretch" | "center" | "tile";

export interface PalobarConfig {
  favoriteApps: string[];
  wallpaper: {
    path: string;
    mode: WallpaperMode;
    color: string;
  };
  lockscreen: {
    image: string;
    color: string;
    autoLockDelaySeconds: number | null;
  };
  power: {
    screenBlankEnabled: boolean;
    screenBlankMinutes: number;
    autoSuspendEnabled: boolean;
    autoSuspendMinutes: number;
  };
}

export const DEFAULT_CONFIG: PalobarConfig = {
  favoriteApps: [],
  wallpaper: {
    path: "",
    mode: "fill",
    color: "#131316",
  },
  lockscreen: {
    image: "",
    color: "#131316",
    autoLockDelaySeconds: null,
  },
  power: {
    screenBlankEnabled: true,
    screenBlankMinutes: 10,
    autoSuspendEnabled: false,
    autoSuspendMinutes: 120,
  },
};

const CONFIG_DIR = GLib.build_filenamev([
  GLib.get_user_config_dir(),
  "palobar",
]);
export const CONFIG_PATH = GLib.build_filenamev([CONFIG_DIR, "config.json"]);

function mergeConfig(raw: Partial<PalobarConfig>): PalobarConfig {
  return {
    favoriteApps: Array.isArray(raw.favoriteApps)
      ? [...new Set(raw.favoriteApps.filter((id) => typeof id === "string" && id.endsWith(".desktop")))]
      : [],
    wallpaper: {
      ...DEFAULT_CONFIG.wallpaper,
      ...(raw.wallpaper ?? {}),
    },
    lockscreen: {
      ...DEFAULT_CONFIG.lockscreen,
      ...(raw.lockscreen ?? {}),
    },
    power: {
      ...DEFAULT_CONFIG.power,
      ...(raw.power ?? {}),
    },
  };
}

export function loadConfig(): PalobarConfig {
  try {
    const file = Gio.File.new_for_path(CONFIG_PATH);
    if (!file.query_exists(null)) {
      saveConfig(DEFAULT_CONFIG);
      return DEFAULT_CONFIG;
    }
    const [ok, bytes] = file.load_contents(null);
    if (!ok) return DEFAULT_CONFIG;
    const text = new TextDecoder().decode(bytes);
    return mergeConfig(JSON.parse(text) as Partial<PalobarConfig>);
  } catch (err) {
    console.error("[config] failed to load config:", err);
    return DEFAULT_CONFIG;
  }
}

function updateConfig(update: (current: PalobarConfig) => PalobarConfig): boolean {
  try {
    GLib.mkdir_with_parents(CONFIG_DIR, 0o755);
    const file = Gio.File.new_for_path(CONFIG_PATH);
    for (let attempt = 0; attempt < 3; attempt++) {
      let current = DEFAULT_CONFIG;
      let etag: string | null = null;
      if (file.query_exists(null)) {
        const [, bytes, version] = file.load_contents(null);
        current = mergeConfig(JSON.parse(new TextDecoder().decode(bytes)));
        etag = version;
      }

      const next = update(current);
      if (next === current) return true;
      const json = JSON.stringify(next, null, 2) + "\n";
      try {
        const [ok] = file.replace_contents(
          new TextEncoder().encode(json),
          etag,
          false,
          Gio.FileCreateFlags.REPLACE_DESTINATION,
          null,
        );
        return ok;
      } catch (err) {
        if (!(err instanceof GLib.Error) ||
          !err.matches(Gio.io_error_quark(), Gio.IOErrorEnum.WRONG_ETAG)) throw err;
      }
    }
    throw new Error("Config changed repeatedly while saving");
  } catch (err) {
    console.error("[config] failed to save config:", err);
    return false;
  }
}

export function saveConfig(cfg: PalobarConfig): void {
  updateConfig((current) => ({ ...cfg, favoriteApps: current.favoriteApps }));
}

export function addFavoriteApp(id: string): boolean {
  if (!id.endsWith(".desktop")) return false;
  return updateConfig((current) =>
    current.favoriteApps.includes(id)
      ? current
      : { ...current, favoriteApps: [...current.favoriteApps, id] },
  );
}

export function removeFavoriteApp(id: string): boolean {
  return updateConfig((current) =>
    current.favoriteApps.includes(id)
      ? { ...current, favoriteApps: current.favoriteApps.filter((entry) => entry !== id) }
      : current,
  );
}

type ConfigListener = (cfg: PalobarConfig) => void;

class ConfigService {
  private current: PalobarConfig;
  private listeners = new Set<ConfigListener>();
  private monitor: Gio.FileMonitor | null = null;
  private debounceId: number | null = null;

  constructor() {
    this.current = loadConfig();
    this.watch();
  }

  get(): PalobarConfig {
    return this.current;
  }

  subscribe(listener: ConfigListener): () => void {
    this.listeners.add(listener);
    listener(this.current);
    return () => this.listeners.delete(listener);
  }

  private emit() {
    for (const listener of this.listeners) {
      listener(this.current);
    }
  }

  private watch() {
    try {
      GLib.mkdir_with_parents(CONFIG_DIR, 0o755);
      const dir = Gio.File.new_for_path(CONFIG_DIR);
      this.monitor = dir.monitor_directory(
        Gio.FileMonitorFlags.WATCH_MOVES,
        null,
      );
      this.monitor.connect("changed", (_mon, file, other, eventType) => {
        const name = file?.get_basename();
        const otherName = other?.get_basename();
        if (name !== "config.json" && otherName !== "config.json") {
          return;
        }
        if (
          eventType === Gio.FileMonitorEvent.DELETED &&
          otherName !== "config.json"
        ) {
          return;
        }
        if (this.debounceId !== null) {
          GLib.source_remove(this.debounceId);
        }
        this.debounceId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 120, () => {
          this.debounceId = null;
          this.current = loadConfig();
          console.log("[config] reloaded:", JSON.stringify(this.current));
          this.emit();
          return GLib.SOURCE_REMOVE;
        });
      });
    } catch (err) {
      console.error("[config] failed to watch config dir:", err);
    }
  }
}

export const configService = new ConfigService();
