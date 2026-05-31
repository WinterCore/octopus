import { html, LitElement } from "lit";
import { customElement, state } from "lit/decorators.js";
import { listStreams, logout, pause, rename, resume, skip, UnauthorizedError } from "../../lib/admin-api";
import type { AdminStream } from "../../lib/admin-api";

@customElement("admin-dashboard")
export class AdminDashboard extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @state()
  private streams: AdminStream[] | null = null;

  @state()
  private error: string | null = null;

  @state()
  private editing: Record<string, string> = {};

  @state()
  private busy: Record<string, boolean> = {};

  private refreshTimer: number | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("flex", "flex-col", "flex-1");
    void this.refresh();
    this.refreshTimer = window.setInterval(() => void this.refresh(), 5000);
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this.refreshTimer !== null) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = null;
    }
  }

  private async refresh(): Promise<void> {
    try {
      this.streams = await listStreams();
      this.error = null;
    } catch (e) {
      if (e instanceof UnauthorizedError) {
        this.dispatchEvent(new CustomEvent("admin-unauthorized", { bubbles: true, composed: true }));
        return;
      }
      this.error = e instanceof Error ? e.message : "Failed to load streams";
    }
  }

  private async run(streamId: string, op: () => Promise<void>): Promise<void> {
    this.busy = { ...this.busy, [streamId]: true };
    try {
      await op();
      await this.refresh();
    } catch (e) {
      if (e instanceof UnauthorizedError) {
        this.dispatchEvent(new CustomEvent("admin-unauthorized", { bubbles: true, composed: true }));
        return;
      }
      this.error = e instanceof Error ? e.message : "Action failed";
    } finally {
      this.busy = { ...this.busy, [streamId]: false };
    }
  }

  private startEdit(s: AdminStream) {
    this.editing = { ...this.editing, [s.id]: s.name };
  }

  private cancelEdit(id: string) {
    const next = { ...this.editing };
    delete next[id];
    this.editing = next;
  }

  private async commitEdit(id: string) {
    const newName = this.editing[id];
    if (newName === undefined) return;
    await this.run(id, () => rename(id, newName));
    this.cancelEdit(id);
  }

  private async handleLogout() {
    try {
      await logout();
    } catch (_) {
      // ignore
    }
    this.dispatchEvent(new CustomEvent("admin-logged-out", { bubbles: true, composed: true }));
  }

  render() {
    return html`
      <main class="bg-gradient-to-b p-4 from-[#51756d] to-[#253330] flex-1">
        <div class="max-w-3xl mx-auto">
          <header class="flex items-center justify-between mb-6 mt-2">
            <h1 class="text-white text-2xl">Admin</h1>
            <div class="flex items-center gap-3">
              <a href="/" class="text-white/60 hover:text-white text-sm underline">View streams</a>
              <button @click=${this.handleLogout} class="text-white/60 hover:text-white text-sm">Sign out</button>
            </div>
          </header>

          ${this.error ? html`<p class="mb-4 text-red-300 text-sm">${this.error}</p>` : ""}

          ${this.streams === null
            ? html`<div class="h-24 bg-white/5 rounded-xl animate-pulse"></div>`
            : html`
                <ul class="space-y-3">
                  ${this.streams.map((s) => this.renderRow(s))}
                </ul>
              `}
        </div>
      </main>
    `;
  }

  private renderRow(s: AdminStream) {
    const isEditing = this.editing[s.id] !== undefined;
    const isBusy = this.busy[s.id] === true;

    return html`
      <li class="bg-white/5 border border-white/10 rounded-xl p-4">
        <div class="flex items-center justify-between gap-3 flex-wrap">
          <div class="min-w-0 flex-1">
            ${isEditing
              ? html`
                  <div class="flex items-center gap-2">
                    <input
                      type="text"
                      .value=${this.editing[s.id]}
                      @input=${(e: Event) =>
                        (this.editing = { ...this.editing, [s.id]: (e.target as HTMLInputElement).value })}
                      class="bg-black/30 text-white border border-white/10 rounded-md px-2 py-1 text-sm w-full max-w-xs"
                    />
                    <button
                      @click=${() => void this.commitEdit(s.id)}
                      ?disabled=${isBusy || (this.editing[s.id] ?? "").trim() === ""}
                      class="text-emerald-300 hover:text-emerald-200 text-sm disabled:opacity-50"
                    >
                      Save
                    </button>
                    <button
                      @click=${() => this.cancelEdit(s.id)}
                      class="text-white/50 hover:text-white text-sm"
                    >
                      Cancel
                    </button>
                  </div>
                `
              : html`
                  <div class="flex items-center gap-2">
                    <span class="text-white text-lg font-medium truncate">${s.name}</span>
                    <button
                      @click=${() => this.startEdit(s)}
                      class="text-white/40 hover:text-white text-xs underline"
                      title="Rename"
                    >
                      rename
                    </button>
                    ${s.paused
                      ? html`<span class="px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-200 text-xs">PAUSED</span>`
                      : ""}
                  </div>
                `}
            <div class="text-white/60 text-sm truncate mt-1">
              ${s.title ?? "Idle"}${s.author ? html` <span class="text-white/40">— ${s.author}</span>` : ""}
            </div>
            ${s.playlist ? html`<div class="text-white/30 text-xs mt-1 truncate">${s.playlist}</div>` : ""}
          </div>

          <div class="flex items-center gap-2 shrink-0">
            ${s.paused
              ? html`
                  <button
                    @click=${() => void this.run(s.id, () => resume(s.id))}
                    ?disabled=${isBusy}
                    class="bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-100 text-sm px-3 py-1.5 rounded-md disabled:opacity-50"
                  >
                    Resume
                  </button>
                `
              : html`
                  <button
                    @click=${() => void this.run(s.id, () => pause(s.id))}
                    ?disabled=${isBusy}
                    class="bg-amber-500/20 hover:bg-amber-500/30 text-amber-100 text-sm px-3 py-1.5 rounded-md disabled:opacity-50"
                  >
                    Pause
                  </button>
                `}
            <button
              @click=${() => void this.run(s.id, () => skip(s.id))}
              ?disabled=${isBusy}
              class="bg-white/10 hover:bg-white/20 text-white text-sm px-3 py-1.5 rounded-md disabled:opacity-50"
            >
              Skip
            </button>
          </div>
        </div>
      </li>
    `;
  }
}
