import {css, html, LitElement, type CSSResultGroup} from "lit";
import {customElement} from "lit/decorators.js";
import "./components/player-progress";

@customElement("octopus-app")
export class Octopus extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
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
                           .progress=${{ current: 50, total: 100 }} />
        </div>
      </main>
    `;
  }
}
