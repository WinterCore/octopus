import { html, LitElement, type PropertyValues } from "lit";
import { customElement, property } from "lit/decorators.js";
import "./components/player-progress";
import { WebSocketManager } from "./lib/ws-manager";
import type {ITimeProgress} from "./components/player-progress";
import { PlaybackController } from "./controllers/playback-controller";

interface AudioMetadata {
  readonly id: string;
  readonly title: string;
  readonly author: string;
  readonly image: string | null;
  readonly buffer_size_ms: number;
  readonly active_file_duration_ms: number;
  readonly active_file_start_time_ms: number;
  readonly active_file_current_time_ms: number;
}

@customElement("octopus-app")
export class Octopus extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  private playback = new PlaybackController(this);
  private wsManager: WebSocketManager;

  @property({ type: Object })
  private metadata: AudioMetadata | null = null;
  private lastHandledFileId: string | null = null;
  private newMetadataRequested: boolean = false;

  @property({ type: String })
  private imageUrl: string = "/logo.webp";

  constructor() {
    super();

    // Initialize WebSocket manager
    this.wsManager = new WebSocketManager(import.meta.env.VITE_WS_URL);

    // Set up WebSocket event handlers
    this.wsManager.onOpen = () => {
      console.log('WebSocket connection established');

      this.wsManager.send("metadata");
    };

    this.wsManager.onClose = () => {
      console.log('WebSocket connection closed');
    };

    this.wsManager.onError = (error) => {
      console.error('WebSocket error:', error);
    };

    this.wsManager.onMessage = (data) => {
      console.log('WebSocket message received:', data);
      this.metadata = data as AudioMetadata;
      this.playback.initialize(this.metadata.active_file_current_time_ms, this.metadata.buffer_size_ms)
      this.newMetadataRequested = false;
      this.lastHandledFileId = this.metadata.id;

      // Update image URL with cache-busting query param
      if (this.metadata.image) {
        const apiBaseUrl = import.meta.env.VITE_API_BASE_URL || "http://localhost:3000";
        const cacheBuster = Math.random().toString(36).substring(7);
        this.imageUrl = `${apiBaseUrl}${this.metadata.image}?v=${cacheBuster}`;
      } else {
        this.imageUrl = "/logo.webp";
      }

      // Handle incoming messages here
    };
  }

  protected updated(changedProperties: PropertyValues): void {
    super.updated(changedProperties);

    // Current track ended
    if (this.metadata && this.playback.currentTimeMs >= this.metadata.active_file_start_time_ms + this.metadata.active_file_duration_ms && !this.newMetadataRequested && this.metadata.id === this.lastHandledFileId) {
      // Request metadata for next track
      this.wsManager.send("metadata");
      this.newMetadataRequested = true;
    }
  }

  async firstUpdated(): Promise<void> {
    // Connect WebSocket on mount
    this.wsManager.connect();
  }

  disconnectedCallback(): void {
    // Clean up when component is removed
    this.wsManager.disconnect();
  }

  handleTogglePlayClick = async () => {
    this.playback.toggle();
    console.log(this.playback.isPlaying ? 'playing' : 'paused');
  }

  get getProgress(): ITimeProgress | null {
    if (! this.metadata) {
      return null;
    }

    return {
      current: Math.max(Math.ceil((this.playback.currentTimeMs - this.metadata.active_file_start_time_ms) / 1000), 0),
      total: Math.ceil(this.metadata.active_file_duration_ms / 1000),
    };
  }

  render() {
    return html`
      <main class="bg-gradient-to-b p-4 from-[#51756d] to-[#253330] flex-1 flex flex-col justify-center items-center">
        ${this.metadata
          ? html`
            <h1 class="text-center text-white text-3xl">
              ${this.metadata ? this.metadata.title : 'Loading...'}
            </h1>
            <h2 class="text-center text-white/70 text-lg mt-1">
              ${this.metadata ? this.metadata.author : 'Loading...'}
            </h2>
          `: html`
            <div role="status" class="max-w-sm animate-pulse flex items-center flex-col gap-4">
              <div class="h-5 bg-[#FFFFFF22] rounded-full w-48"></div>
              <div class="h-1.5 bg-[#FFFFFF22] rounded-full w-22"></div>
            </div>
          `}
        <div class="max-w-[400px] w-full p-4">
          <player-progress .strokeWidth=${4}
                           .progress=${this.getProgress}
                           .image=${this.imageUrl} />
        </div>
        <button @click="${this.handleTogglePlayClick}" class="text-white mt-6 w-8 h-8 cursor-pointer hover:scale-110 transition-transform">
          ${this.playback.isPlaying ? html`
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-pause"><rect x="6" y="4" width="4" height="16"></rect><rect x="14" y="4" width="4" height="16"></rect></svg>
          ` : html`
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-play"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
          `}
        </button>
      </main>
    `;
  }
}
