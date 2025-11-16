import { html, LitElement } from "lit";
import { customElement, property } from "lit/decorators.js";
import "./components/player-progress";
import { OggOpusDecoderWebWorker } from "ogg-opus-decoder";
import { WebSocketManager } from "./lib/ws-manager";
import type {ITimeProgress} from "./components/player-progress";

class OggOpusParser {
  parseOggPage(buffer: ArrayBuffer): number | null {
    const view = new DataView(buffer);

    // Check for "OggS" magic number
    if (view.getUint32(0, false) !== 0x4f676753) return null;

    // Granule position at bytes 6-13 (64-bit little-endian)
    const granuleLow = view.getUint32(6, true);
    const granuleHigh = view.getUint32(10, true);

    const granulePosition = granuleLow + (granuleHigh * 0x100000000);

    return granulePosition;
  }
}

interface AudioMetadata {
  readonly name: string;
  readonly author: string;
  readonly image: string | null;
  readonly active_file_start_time_ms: number;
  readonly active_file_duration_ms: number;
}

@customElement("octopus-app")
export class Octopus extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @property({ type: Boolean })
  isPlaying: boolean = false;

  @property({ type: Number })
  private currentTimeMs: number = 0;

  private audioContext: AudioContext | null = null;
  private scheduledUntil: number = 0;
  private abortController: AbortController | null = null;
  private currentDecoder: OggOpusDecoderWebWorker | null = null;
  private wsManager: WebSocketManager;
  private metadata: AudioMetadata | null = null;
  private readonly BUFFER_LOOKAHEAD = 0.1; // 100ms buffer lookahead

  constructor() {
    super();

    // Initialize WebSocket manager
    this.wsManager = new WebSocketManager('ws://localhost:3001');

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

      // Handle incoming messages here
    };
  }

  async firstUpdated(): Promise<void> {
    // Connect WebSocket on mount
    this.wsManager.connect();
  }

  private async startStream(): Promise<void> {
    // Stop any existing stream
    this.stopStream();

    // Create new audio context
    this.audioContext = new AudioContext();
    // Initialize with a small lookahead to build up buffer
    this.scheduledUntil = this.audioContext.currentTime + this.BUFFER_LOOKAHEAD;

    // Create new abort controller
    this.abortController = new AbortController();

    try {
      // Create decoder and parser
      const decoder = new OggOpusDecoderWebWorker();
      const parser = new OggOpusParser();
      this.currentDecoder = decoder;
      await decoder.ready;

      console.log('Decoder ready, starting stream...');

      // Fetch and decode stream
      const response = await fetch('http://localhost:3000', {
        signal: this.abortController.signal
      });
      const reader = response.body!.getReader();

      while (true) {
        const {done, value} = await reader.read();
        if (done) break;

        // Check if we've been aborted
        if (this.abortController?.signal.aborted) {
          break;
        }

        // Parse Ogg page to extract granule position
        const granulePosition = parser.parseOggPage(value!.buffer);

        // Decode the chunk
        const result = await decoder.decode(value!);

        // Calculate current time from granule position
        if (granulePosition !== null && result.sampleRate) {
          this.currentTimeMs = granulePosition / result.sampleRate * 1000;
        }

        // Play the decoded audio
        if (result.channelData && result.channelData.length > 0 && result.samplesDecoded > 0) {
          this.playAudioData(result.channelData, result.sampleRate);
        }
      }

      console.log('Stream ended');
      decoder.free();
      this.currentDecoder = null;
    } catch (error: any) {
      if (error.name === 'AbortError') {
        console.log('Stream aborted');
      } else {
        console.error('Error fetching or processing stream:', error);
      }

      // Clean up decoder
      if (this.currentDecoder) {
        this.currentDecoder.free();
        this.currentDecoder = null;
      }
    }
  }

  private stopStream(): void {
    // Abort the fetch request
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }

    // Close audio context
    if (this.audioContext) {
      this.audioContext.close();
      this.audioContext = null;
    }

    // Free decoder
    if (this.currentDecoder) {
      this.currentDecoder.free();
      this.currentDecoder = null;
    }

    this.scheduledUntil = 0;
  }

  private playAudioData(channelData: Float32Array[], sampleRate: number): void {
    if (!this.audioContext) return;

    const audioBuffer = this.audioContext.createBuffer(
      channelData.length,
      channelData[0].length,
      sampleRate
    );

    // Copy channel data to audio buffer
    for (let i = 0; i < channelData.length; i++) {
      audioBuffer.copyToChannel(channelData[i], i);
    }

    // Create source and schedule playback
    const source = this.audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(this.audioContext.destination);

    // Schedule playback with buffer lookahead to prevent underruns
    const now = this.audioContext.currentTime;

    // If we've fallen behind, reset to current time + lookahead
    if (this.scheduledUntil < now + this.BUFFER_LOOKAHEAD) {
      this.scheduledUntil = now + this.BUFFER_LOOKAHEAD;
    }

    source.start(this.scheduledUntil);
    this.scheduledUntil += audioBuffer.duration;
  }
  
  disconnectedCallback(): void {
    // Clean up when component is removed
    this.stopStream();
    this.wsManager.disconnect();
  }

  handleTogglePlayClick = async () => {
    if (this.isPlaying) {
      // Stop the stream
      this.stopStream();
      this.isPlaying = false;
      console.log('paused');
    } else {
      // Start a fresh stream
      this.isPlaying = true;
      await this.startStream();
      console.log('playing');
    }
  }

  get getProgress(): ITimeProgress {
    if (! this.metadata) {
      return { current: 0, total: 100 };
    }

    console.log('debug', {
      current: (this.currentTimeMs - this.metadata.active_file_start_time_ms) / 1000,
      total: this.metadata.active_file_duration_ms / 1000,
    });

    return {
      current: Math.ceil((this.currentTimeMs - this.metadata.active_file_start_time_ms) / 1000),
      total: Math.ceil(this.metadata.active_file_duration_ms / 1000),
    };
  }

  render() {
    return html`
      <main class="bg-gradient-to-b from-[#51756d] to-[#253330] flex-1 flex flex-col justify-center items-center">
        <h1 class="text-center text-white text-3xl">
          Song name
        </h1>
        <h2 class="text-center text-white/70 text-lg mt-1">
          Author Smith
        </h2>
        <div class="max-w-[400px] w-full p-4">
          <player-progress .strokeWidth=${4}
                           .progress=${this.getProgress} />
        </div>
        <button @click="${this.handleTogglePlayClick}" class="text-white mt-6 w-8 h-8 cursor-pointer hover:scale-110 transition-transform">
          ${this.isPlaying ? html`
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-pause"><rect x="6" y="4" width="4" height="16"></rect><rect x="14" y="4" width="4" height="16"></rect></svg>
          ` : html`
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-play"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
          `}
        </button>
      </main>
    `;
  }
}
