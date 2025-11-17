import type { ReactiveController, ReactiveControllerHost } from "lit";
import type { OggOpusDecoderWebWorker } from "ogg-opus-decoder";

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

export class PlaybackController implements ReactiveController {
  private host: ReactiveControllerHost;
  private audioContext: AudioContext | null = null;
  private scheduledUntil: number = 0;
  private abortController: AbortController | null = null;
  private currentDecoder: OggOpusDecoderWebWorker | null = null;
  private readonly BUFFER_LOOKAHEAD = 0.1; // 100ms buffer lookahead
  private timeAdvanceInterval: number | null = null;
  private readonly TIME_ADVANCE_INTERVAL_MS = 100; // Update time every 100ms

  public isPlaying: boolean = false;
  public currentTimeMs: number = 0;

  constructor(host: ReactiveControllerHost) {
    this.host = host;
    host.addController(this);
  }

  hostConnected() {
    // Called when host is connected to DOM
  }

  hostDisconnected() {
    // Clean up when host is removed
    this.stop();
    this.clearTimeAdvanceInterval();
  }

  private clearTimeAdvanceInterval() {
    if (this.timeAdvanceInterval !== null) {
      clearInterval(this.timeAdvanceInterval);
      this.timeAdvanceInterval = null;
    }
  }

  private startTimeAdvanceInterval() {
    this.clearTimeAdvanceInterval();
    this.timeAdvanceInterval = setInterval(() => {
      this.currentTimeMs += this.TIME_ADVANCE_INTERVAL_MS;
      this.host.requestUpdate();
    }, this.TIME_ADVANCE_INTERVAL_MS) as unknown as number;
  }

  async start(): Promise<void> {
    // Stop any existing stream
    this.stop();

    // Clear the time advance interval since we're playing live audio
    this.clearTimeAdvanceInterval();

    this.isPlaying = true;
    this.host.requestUpdate();

    // Create new audio context
    this.audioContext = new AudioContext();
    // Initialize with a small lookahead to build up buffer
    this.scheduledUntil = this.audioContext.currentTime + this.BUFFER_LOOKAHEAD;

    // Create new abort controller
    this.abortController = new AbortController();

    try {
      // Create decoder and parser
      const decoder = new (await import("ogg-opus-decoder")).OggOpusDecoderWebWorker();
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
          this.host.requestUpdate();
        }

        // Play the decoded audio
        if (result.channelData && result.channelData.length > 0 && result.samplesDecoded > 0) {
          this.playAudioData(result.channelData, result.sampleRate);
        }
      }

      console.log('Stream ended');
      decoder.free();
      this.currentDecoder = null;
      this.isPlaying = false;
      this.startTimeAdvanceInterval();
      this.host.requestUpdate();
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

      this.isPlaying = false;
      this.startTimeAdvanceInterval();
      this.host.requestUpdate();
    }
  }

  stop(): void {
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
    this.isPlaying = false;

    // Start advancing time to give illusion stream is still going
    this.startTimeAdvanceInterval();

    this.host.requestUpdate();
  }

  toggle(): void {
    if (this.isPlaying) {
      this.stop();
    } else {
      this.start();
    }
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
}
