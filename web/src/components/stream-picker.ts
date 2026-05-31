import { html, LitElement } from "lit";
import { customElement, state } from "lit/decorators.js";
import { streamsListUrl } from "../lib/env";
import { routeToPath } from "../lib/router";

interface StreamSummary {
  id: string;
  name: string;
  paused: boolean;
  title?: string;
  author?: string;
}

@customElement("stream-picker")
export class StreamPicker extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @state()
  private streams: StreamSummary[] | null = null;

  @state()
  private error: string | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("flex", "flex-col", "flex-1");
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const response = await fetch(streamsListUrl());
      if (!response.ok) {
        throw new Error(`Failed to load streams: ${response.status}`);
      }
      this.streams = (await response.json()) as StreamSummary[];
    } catch (e) {
      console.error(e);
      this.error = e instanceof Error ? e.message : "Failed to load streams";
    }
  }

  render() {
    return html`
      <main class="bg-gradient-to-b p-4 from-[#51756d] to-[#253330] flex-1 flex flex-col items-center justify-center">
        <div class="w-full max-w-md">
          <h1 class="text-center text-white text-3xl mb-1">Octopus Radio</h1>
          <p class="text-center text-white/70 text-sm mb-6">Pick a stream to listen to</p>

          ${this.error
            ? html`<p class="text-center text-red-300">${this.error}</p>`
            : this.streams === null
              ? html`
                  <div class="space-y-3">
                    <div class="h-16 bg-white/5 rounded-xl animate-pulse"></div>
                    <div class="h-16 bg-white/5 rounded-xl animate-pulse"></div>
                  </div>
                `
              : this.streams.length === 0
                ? html`<p class="text-center text-white/70">No streams configured.</p>`
                : html`
                    <ul class="space-y-3">
                      ${this.streams.map(
                        (s) => html`
                          <li>
                            <a
                              href=${routeToPath({ kind: "listen", streamId: s.id })}
                              class="block bg-white/5 hover:bg-white/10 transition-colors rounded-xl p-4 border border-white/10"
                            >
                              <div class="flex items-center justify-between">
                                <div class="min-w-0">
                                  <div class="text-white text-lg font-medium truncate">${s.name}</div>
                                  ${s.title
                                    ? html`<div class="text-white/60 text-sm truncate">
                                        ${s.title}${s.author ? html` <span class="text-white/40">— ${s.author}</span>` : ""}
                                      </div>`
                                    : ""}
                                </div>
                                ${s.paused
                                  ? html`<span class="ms-3 shrink-0 px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-200 text-xs">PAUSED</span>`
                                  : ""}
                              </div>
                            </a>
                          </li>
                        `
                      )}
                    </ul>
                  `}
        </div>
      </main>
    `;
  }
}
