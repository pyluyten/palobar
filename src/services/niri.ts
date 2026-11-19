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
type OverviewListener = (isOpen: boolean) => void;

class NiriService {
  private workspaces: NiriWorkspace[] = [];
  private overviewOpen = false;
  private listeners = new Set<Listener>();
  private overviewListeners = new Set<OverviewListener>();
  private running = false;

  constructor() {
    this.init();
  }

  private async init() {
    await Promise.all([this.refreshWorkspaces(), this.refreshOverviewState()]);
    this.startEventStream();
  }

  getWorkspaces(): NiriWorkspace[] {
    return this.workspaces;
  }

  isOverviewOpen(): boolean {
    return this.overviewOpen;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.workspaces);
    return () => {
      this.listeners.delete(listener);
    };
  }

  subscribeOverview(listener: OverviewListener): () => void {
    this.overviewListeners.add(listener);
    listener(this.overviewOpen);
    return () => {
      this.overviewListeners.delete(listener);
    };
  }

  private emit() {
    for (const listener of this.listeners) {
      listener(this.workspaces);
    }
  }

  private emitOverview() {
    for (const listener of this.overviewListeners) {
      listener(this.overviewOpen);
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

  async refreshOverviewState() {
    try {
      const out = await execAsync(["niri", "msg", "-j", "overview-state"]);
      const parsed = JSON.parse(out) as { is_open: boolean };
      this.overviewOpen = Boolean(parsed.is_open);
      this.emitOverview();
    } catch (err) {
      console.error("[niri] failed to fetch overview state:", err);
    }
  }

  async sendAction(action: string, ...args: string[]) {
    try {
      await execAsync(["niri", "msg", "action", action, ...args]);
    } catch (err) {
      console.error(`[niri] failed action ${action}:`, err);
    }
  }

  async focusWorkspace(idx: number) {
    await this.sendAction("focus-workspace", String(idx));
  }

  async toggleOverview() {
    await this.sendAction("toggle-overview");
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
      } else if ("OverviewOpenedOrClosed" in event) {
        const payload = event.OverviewOpenedOrClosed as {
          is_open: boolean;
        };
        this.overviewOpen = Boolean(payload.is_open);
        this.emitOverview();
      }
    } catch (err) {
      console.error("[niri] failed to parse event:", err);
    }
  }
}

export const niri = new NiriService();
