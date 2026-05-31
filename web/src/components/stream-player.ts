import { html, LitElement, type PropertyValues } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import "./player-progress";
import type { ITimeProgress } from "./player-progress";
import { PlaybackController } from "../controllers/playback-controller";
import { WebSocketManager } from "../lib/ws-manager";
import { playlistImageUrl, wsUrl } from "../lib/env";
import { routeToPath } from "../lib/router";

interface AudioMetadata {
  readonly id: string;
  readonly title: string;
  readonly author: string;
  readonly image: string | null;
  readonly paused?: boolean;
  readonly stream_name?: string;
  readonly buffer_size_ms: number;
  readonly active_file_duration_ms: number;
  readonly active_file_start_time_ms: number;
  readonly active_file_current_time_ms: number;
}

@customElement("stream-player")
export class StreamPlayer extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @property({ type: String })
  streamId!: string;

  private playback = new PlaybackController(this);
  private wsManager: WebSocketManager | null = null;

  @state()
  private metadata: AudioMetadata | null = null;

  private lastHandledFileId: string | null = null;
  private newMetadataRequested: boolean = false;

  @state()
  private imageUrl: string = "";

  connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("flex", "flex-col", "flex-1");
  }

  protected updated(changedProperties: PropertyValues): void {
    super.updated(changedProperties);

    if (changedProperties.has("streamId")) {
      const wasPlaying = this.playback.isPlaying;
      this.metadata = null;
      this.imageUrl = "";
      this.lastHandledFileId = null;
      this.newMetadataRequested = false;
      this.connectWs();
      if (wasPlaying) {
        void this.playback.start(this.streamId);
      }
    }

    // Current track ended → request next metadata
    if (
      this.metadata &&
      this.playback.currentTimeMs >=
        this.metadata.active_file_start_time_ms + this.metadata.active_file_duration_ms &&
      !this.newMetadataRequested &&
      this.metadata.id === this.lastHandledFileId
    ) {
      this.wsManager?.send("metadata");
      this.newMetadataRequested = true;
    }
  }

  private connectWs(): void {
    if (!this.streamId) return;

    const targetUrl = wsUrl(this.streamId);
    if (this.wsManager) {
      this.wsManager.setUrl(targetUrl);
      return;
    }

    this.wsManager = new WebSocketManager(targetUrl);
    this.wsManager.onOpen = () => {
      this.wsManager?.send("metadata");
    };
    this.wsManager.onMessage = (data) => {
      this.metadata = data as AudioMetadata;

      this.playback.setPaused(this.metadata.paused === true);
      this.playback.initialize(
        this.metadata.active_file_current_time_ms,
        this.metadata.buffer_size_ms,
        this.metadata.active_file_start_time_ms + this.metadata.active_file_duration_ms,
      );
      this.newMetadataRequested = false;
      this.lastHandledFileId = this.metadata.id;

      const streamLabel = this.metadata.stream_name ?? "Octopus";
      const trackLabel = this.metadata.author
        ? `${this.metadata.title} — ${this.metadata.author}`
        : this.metadata.title;
      document.title = `${streamLabel} · ${trackLabel}`;

      if (this.metadata.image) {
        const cacheBuster = Math.random().toString(36).substring(7);
        this.imageUrl = playlistImageUrl(this.streamId, cacheBuster);
      } else {
        this.imageUrl = "/logo.webp";
      }
    };
    this.wsManager.connect();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.wsManager?.disconnect();
    this.wsManager = null;
  }

  private handleTogglePlayClick = async () => {
    if (this.playback.isPlaying) {
      this.playback.toggle();
    } else {
      await this.playback.start(this.streamId);
    }
  };

  get getProgress(): ITimeProgress | null {
    if (!this.metadata) return null;
    return {
      current: Math.max(
        Math.ceil((this.playback.currentTimeMs - this.metadata.active_file_start_time_ms) / 1000),
        0,
      ),
      total: Math.ceil(this.metadata.active_file_duration_ms / 1000),
    };
  }

  render() {
    const paused = this.metadata?.paused === true;
    const streamLabel = this.metadata?.stream_name;

    return html`
      <main class="bg-gradient-to-b p-4 from-[#51756d] to-[#253330] flex-1 flex flex-col justify-center items-center relative">
        <a href=${routeToPath({ kind: "picker" })} class="absolute top-4 start-4 text-white/60 hover:text-white text-sm">← All streams</a>

        ${streamLabel
          ? html`<div class="text-white/50 text-xs uppercase tracking-wider mb-2">${streamLabel}</div>`
          : ""}

        ${this.metadata
          ? html`
              <h1 class="text-center text-white text-3xl">${this.metadata.title}</h1>
              <h2 class="text-center text-white/70 text-lg mt-1">${this.metadata.author}</h2>
            `
          : html`
              <div role="status" class="max-w-sm animate-pulse flex items-center flex-col gap-4">
                <div class="h-5 bg-[#FFFFFF22] rounded-full w-48"></div>
                <div class="h-1.5 bg-[#FFFFFF22] rounded-full w-22"></div>
              </div>
            `}

        <div class="max-w-[400px] w-full p-4 relative">
          <player-progress
            .strokeWidth=${4}
            .progress=${this.getProgress}
            .image=${this.imageUrl}
          ></player-progress>
          ${paused
            ? html`<div class="absolute inset-0 flex items-center justify-center pointer-events-none">
                <span class="px-3 py-1 rounded-full bg-amber-500/30 text-amber-100 text-xs tracking-wider backdrop-blur-sm">PAUSED</span>
              </div>`
            : ""}
        </div>

        <button
          @click=${this.handleTogglePlayClick}
          class="text-white mt-6 w-8 h-8 cursor-pointer hover:scale-110 transition-transform"
        >
          ${this.playback.isPlaying
            ? html`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="4" width="4" height="16"></rect><rect x="14" y="4" width="4" height="16"></rect></svg>`
            : html`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>`}
        </button>
      </main>
    `;
  }
}
