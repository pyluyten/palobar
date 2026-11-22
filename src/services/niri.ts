import Gio from "gi://Gio";
import GLib from "gi://GLib";
import { execAsync } from "ags/process";

Gio._promisify(Gio.SocketClient.prototype, "connect_async");
Gio._promisify(Gio.OutputStream.prototype, "write_all_async");
Gio._promisify(Gio.DataInputStream.prototype, "read_line_async");

export interface NiriWindow {
  id: number;
  title: string | null;
  app_id: string | null;
  pid?: number | null;
  workspace_id: number | null;
  is_focused: boolean;
  layout?: { pos_in_scrolling_layout?: [number, number] | null };
}

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
  private windowClosedListeners = new Set<(id: number) => void>();
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

  subscribeWindowClosed(listener: (id: number) => void): () => void {
    this.windowClosedListeners.add(listener);
    return () => this.windowClosedListeners.delete(listener);
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

  async queryWorkspaces(): Promise<NiriWorkspace[]> {
    const out = await execAsync(["niri", "msg", "--json", "workspaces"]);
    return this.sortWorkspaces(JSON.parse(out) as NiriWorkspace[]);
  }

  async queryWindows(): Promise<NiriWindow[]> {
    const out = await execAsync(["niri", "msg", "--json", "windows"]);
    return JSON.parse(out) as NiriWindow[];
  }

  async closeWindow(id: number) {
    await execAsync(["niri", "msg", "action", "close-window", "--id", String(id)]);
  }

  async forceKillWindow(id: number, pid: number) {
    const window = (await this.queryWindows()).find((candidate) => candidate.id === id);
    if (!window || window.pid !== pid || !Number.isSafeInteger(pid) || pid <= 1) {
      throw new Error("The window no longer has a matching process");
    }
    await execAsync(["kill", "-KILL", String(pid)]);
  }

  async moveWindowToWorkspace(windowId: number, workspaceId: number) {
    await this.requestAction({
      MoveWindowToWorkspace: { window_id: windowId, reference: { Id: workspaceId }, focus: false },
    });
  }

  async renameWorkspace(id: number, name: string) {
    await this.requestAction({ SetWorkspaceName: { name, workspace: { Id: id } } });
  }

  private async requestAction(action: Record<string, object>) {
    const path = GLib.getenv("NIRI_SOCKET");
    if (!path) throw new Error("NIRI_SOCKET is unavailable");

    // The CLI parses workspace references as indices or names. IPC also accepts stable IDs.
    const request = { Action: action };
    const client = new Gio.SocketClient({ timeout: 3 });
    const connection = await client.connect_async(new Gio.UnixSocketAddress({ path }), null);
    try {
      const bytes = new TextEncoder().encode(JSON.stringify(request) + "\n");
      await connection.output_stream.write_all_async(bytes, GLib.PRIORITY_DEFAULT, null);
      const input = new Gio.DataInputStream({
        base_stream: connection.input_stream,
        close_base_stream: false,
      });
      const [line] = await input.read_line_async(GLib.PRIORITY_DEFAULT, null);
      if (!line) throw new Error("Niri closed the connection without replying");
      const reply = JSON.parse(new TextDecoder().decode(line));
      if (!("Ok" in reply)) throw new Error(reply.Err ?? "Niri rejected the action");
    } finally {
      connection.close(null);
    }
  }

  async refreshWorkspaces() {
    try {
      this.workspaces = await this.queryWorkspaces();
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

  async focusWindow(id: number) {
    await this.sendAction("focus-window", "--id", String(id));
  }

  async getWindows(): Promise<NiriWindow[]> {
    try {
      return await this.queryWindows();
    } catch (err) {
      console.error("[niri] failed to fetch windows:", err);
      return [];
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
      } else if ("WindowClosed" in event) {
        const { id } = event.WindowClosed as { id: number };
        for (const listener of this.windowClosedListeners) listener(id);
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
