import {OggOpusDecoderWebWorker} from "ogg-opus-decoder";

export class Player {
  private url: string;
  private parser: OggOpusDecoderWebWorker;
  private audioElement: HTMLAudioElement;
  private mediaSource: MediaSource;

  constructor(url: string, audioElement: HTMLAudioElement) {
    this.url = url;
    this.parser = new OggOpusDecoderWebWorker();
    this.audioElement = audioElement;
    this.mediaSource = new MediaSource();
    this.audioElement.src = URL.createObjectURL(this.mediaSource);
  }

  async stream(sourceBuffer: SourceBuffer) {
    const resp = await fetch(this.url);

    if (! resp.body) {
      throw new Error("Failed to parse body!");
    }

    const reader = resp.body.getReader();

    while (true) {
      const { done, value } = await reader.read();

      if (done) {
        throw new Error("Stream ended");
      }

      while (true) {
        const frame = await this.parser.decode(value);
        frame.channelData[0]

      }
    }
  }

  async init() {
    await this.parser.ready;

    this.mediaSource.addEventListener("sourceopen", () => {
      const sourceBuffer = this.mediaSource.addSourceBuffer("audio/ogg; codecs=opus");

      // 5. Start fetching and processing the stream
      this.stream(sourceBuffer);
    });

  }
}
