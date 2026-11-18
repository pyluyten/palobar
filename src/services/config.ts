import Gio from "gi://Gio";
import GLib from "gi://GLib";

export type WallpaperMode = "fill" | "fit" | "stretch" | "center" | "tile";

export interface PalobarConfig {
  wallpaper: {
    path: string;
    mode: WallpaperMode;
    color: string;
  };
  lockscreen: {
    image: string;
    color: string;
  };
}

export const DEFAULT_CONFIG: PalobarConfig = {
  wallpaper: {
    path: "",
    mode: "fill",
    color: "#131316",
  },
  lockscreen: {
    image: "",
    color: "#131316",
  },
};

const CONFIG_DIR = GLib.build_filenamev([
  GLib.get_user_config_dir(),
  "palobar",
]);
export const CONFIG_PATH = GLib.build_filenamev([CONFIG_DIR, "config.json"]);

function mergeConfig(raw: Partial<PalobarConfig>): PalobarConfig {
  return {
    wallpaper: {
      ...DEFAULT_CONFIG.wallpaper,
      ...(raw.wallpaper ?? {}),
    },
    lockscreen: {
      ...DEFAULT_CONFIG.lockscreen,
      ...(raw.lockscreen ?? {}),
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

export function saveConfig(cfg: PalobarConfig): void {
  try {
    GLib.mkdir_with_parents(CONFIG_DIR, 0o755);
    const file = Gio.File.new_for_path(CONFIG_PATH);
    const json = JSON.stringify(cfg, null, 2) + "\n";
    file.replace_contents(
      new TextEncoder().encode(json),
      null,
      false,
      Gio.FileCreateFlags.REPLACE_DESTINATION,
      null,
    );
  } catch (err) {
    console.error("[config] failed to save config:", err);
  }
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
      const file = Gio.File.new_for_path(CONFIG_PATH);
      this.monitor = file.monitor_file(Gio.FileMonitorFlags.NONE, null);
      this.monitor.connect("changed", (_mon, _file, _other, eventType) => {
        if (
          eventType !== Gio.FileMonitorEvent.CHANGED &&
          eventType !== Gio.FileMonitorEvent.CREATED &&
          eventType !== Gio.FileMonitorEvent.CHANGES_DONE_HINT
        ) {
          return;
        }
        if (this.debounceId !== null) {
          GLib.source_remove(this.debounceId);
        }
        this.debounceId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 120, () => {
          this.debounceId = null;
          this.current = loadConfig();
          this.emit();
          return GLib.SOURCE_REMOVE;
        });
      });
    } catch (err) {
      console.error("[config] failed to watch config file:", err);
    }
  }
}

export const configService = new ConfigService();
