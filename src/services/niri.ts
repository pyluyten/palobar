import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";

export interface NiriWorkspace {
  id: number;
  idx: number;
  name: string | null;
  output: string | null;
  is_active: boolean;
  is_focused: boolean;
  is_urgent: boolean;
}

type Listener = (workspaces: NiriWorkspace[]) => void;

class NiriService {
  private workspaces: NiriWorkspace[] = [];
  private listeners = new Set<Listener>();
  private running = false;

  constructor() {
    this.init();
  }

  private async init() {
    await this.refreshWorkspaces();
    this.startEventStream();
  }

  getWorkspaces(): NiriWorkspace[] {
    return this.workspaces;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.workspaces);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private emit() {
    for (const listener of this.listeners) {
      listener(this.workspaces);
    }
  }

  private sortWorkspaces(list: NiriWorkspace[]): NiriWorkspace[] {
    return [...list].sort((a, b) => a.idx - b.idx);
  }

  async refreshWorkspaces() {
    try {
      const out = await execAsync(["niri", "msg", "-j", "workspaces"]);
      const parsed = JSON.parse(out) as NiriWorkspace[];
      this.workspaces = this.sortWorkspaces(parsed);
      this.emit();
    } catch (err) {
      console.error("[niri] failed to fetch workspaces:", err);
    }
  }

  async focusWorkspace(idx: number) {
    try {
      await execAsync(["niri", "msg", "action", "focus-workspace", String(idx)]);
    } catch (err) {
      console.error("[niri] failed to focus workspace:", err);
    }
  }

  async toggleOverview() {
    try {
      await execAsync(["niri", "msg", "action", "toggle-overview"]);
    } catch (err) {
      console.error("[niri] failed to toggle overview:", err);
    }
  }

  private startEventStream() {
    if (this.running) return;
    this.running = true;

    const proc = Gio.Subprocess.new(
      ["niri", "msg", "-j", "event-stream"],
      Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE,
    );

    const stdout = proc.get_stdout_pipe();
    if (!stdout) return;

    const stream = new Gio.DataInputStream({ base_stream: stdout });

    const readNextLine = () => {
      stream.read_line_async(GLib.PRIORITY_DEFAULT, null, (_src, res) => {
        try {
          const [lineBytes] = stream.read_line_finish_utf8(res);
          if (lineBytes === null) {
            this.running = false;
            return;
          }

          if (lineBytes.trim().length > 0) {
            this.handleEvent(lineBytes);
          }
          readNextLine();
        } catch (err) {
          console.error("[niri] event-stream read error:", err);
          this.running = false;
        }
      });
    };

    readNextLine();
  }

  private handleEvent(rawLine: string) {
    try {
      const event = JSON.parse(rawLine) as Record<string, unknown>;

      if ("WorkspacesChanged" in event) {
        const payload = event.WorkspacesChanged as {
          workspaces: NiriWorkspace[];
        };
        this.workspaces = this.sortWorkspaces(payload.workspaces);
        this.emit();
      } else if ("WorkspaceActivated" in event) {
        const payload = event.WorkspaceActivated as {
          id: number;
          focused: boolean;
        };
        this.workspaces = this.workspaces.map((ws) => ({
          ...ws,
          is_active: ws.id === payload.id ? true : ws.is_active,
          is_focused: payload.focused ? ws.id === payload.id : ws.is_focused,
        }));
        this.emit();
      }
    } catch (err) {
      console.error("[niri] failed to parse event:", err);
    }
  }
}

export const niri = new NiriService();
