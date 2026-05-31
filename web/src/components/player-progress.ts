import {html, LitElement, svg} from "lit";
import {customElement, property, state} from "lit/decorators.js";
import {secondsToHumanReadable} from "../utils/time";
import {getArc} from "../utils/svg";
import {clamp} from "../utils/math";

export interface ITimeProgress {
  readonly current: number;
  readonly total: number;
}

@customElement("player-progress")
export class PlayerProgress extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @property({ type: String })
  className: string = "";

  @property({ type: Object })
  progress!: ITimeProgress | null;

  @property({ type: String })
  image: string = "";

  @property({ type: Number })
  strokeWidth: number = 4;

  @state()
  private resolvedImageUrl: string = "/logo.webp";

  @state()
  private isInitialImageLoad: boolean = true;

  private currentImageHash: string | null = null;
  private currentObjectUrl: string | null = null;
  private pendingImageUrl: string | null = null;

  private async hashBuffer(buffer: ArrayBuffer): Promise<string> {
    const digest = await crypto.subtle.digest("SHA-1", buffer);
    return Array.from(new Uint8Array(digest))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  }

  private async resolveImageUrl(imageUrl: string) {
    if (!imageUrl || imageUrl === "/logo.webp") {
      if (this.currentObjectUrl) {
        URL.revokeObjectURL(this.currentObjectUrl);
        this.currentObjectUrl = null;
      }
      this.currentImageHash = null;
      this.resolvedImageUrl = "/logo.webp";
      this.pendingImageUrl = imageUrl || null;
      this.isInitialImageLoad = false;
      return;
    }

    this.pendingImageUrl = imageUrl;

    try {
      const response = await fetch(imageUrl);
      if (!response.ok) {
        throw new Error(`Image fetch failed with status ${response.status}`);
      }

      // A newer resolve has started; abandon this one before mutating state.
      if (this.pendingImageUrl !== imageUrl) return;

      const buffer = await response.arrayBuffer();
      if (this.pendingImageUrl !== imageUrl) return;

      const hash = await this.hashBuffer(buffer);
      if (this.pendingImageUrl !== imageUrl) return;

      if (hash === this.currentImageHash) {
        // Same bytes as what's already displayed — keep current image.
        return;
      }

      const newObjectUrl = URL.createObjectURL(new Blob([buffer]));
      if (this.currentObjectUrl) URL.revokeObjectURL(this.currentObjectUrl);
      this.currentObjectUrl = newObjectUrl;
      this.currentImageHash = hash;
      this.resolvedImageUrl = newObjectUrl;
    } catch (error) {
      console.error("Failed to load playlist image:", error);
      // Only fall back to the logo if we have nothing displayed yet;
      // otherwise keep the currently visible image to avoid flicker.
      if (this.currentImageHash === null) {
        this.resolvedImageUrl = "/logo.webp";
      }
    } finally {
      // Clear the initial-load placeholder once the first attempt finishes,
      // regardless of outcome. Subsequent fetches always happen in the
      // background without showing a placeholder.
      if (this.pendingImageUrl === imageUrl) {
        this.isInitialImageLoad = false;
      }
    }
  }

  updated(changedProperties: Map<string, any>) {
    if (changedProperties.has("image")) {
      this.resolveImageUrl(this.image);
    }
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this.currentObjectUrl) {
      URL.revokeObjectURL(this.currentObjectUrl);
      this.currentObjectUrl = null;
    }
  }

  render() {
    const startAngle = 0;
    const endAngle = Math.PI * 1.65;
    const rotate = Math.PI * (1.675);

    const offsetY = 10;
    const cx = 150;
    const cy = 150 + offsetY;

    const pr = 6;
    const halfPr = pr / 2;

    const halfStroke = this.strokeWidth / 2;
    const radius = 150 - pr;
    const r = radius - halfStroke;

    const imagePadding = 20;

    const ix = imagePadding + pr;
    const iy = imagePadding + offsetY + pr;
    const iw = (radius * 2) - (imagePadding * 2);
    const ih = (radius * 2) - (imagePadding * 2);
    
    function getProgressParams(progress: ITimeProgress | null) {
      const pathFull = getArc([cx, cy], [r, r], startAngle, endAngle, rotate);
      const percentage = clamp(0, 1, progress ? (progress.current / progress.total) : 0);
      const currEndAngle = endAngle * percentage;
      const pathCurrent = getArc([cx, cy], [r, r], startAngle, currEndAngle, rotate);

      const gg = currEndAngle + rotate;
      const px = Math.cos(gg) * r + cx;
      const py = Math.sin(gg) * r + cy;

      return {
        isLoading: !progress,
        pathFull,
        pathCurrent,
        px,
        py,
      };
    }
    
    const progressParams = getProgressParams(this.progress);

    return html`
      <svg xmlns="http://www.w3.org/2000/svg"
           class="select-none ${this.className}"
           viewBox="0 0 300 310"
           version="1.1">
        ${this.progress && svg`
          <text x=${135 + halfPr}
                y=${15 + halfPr}
                font-size="12"
                text-anchor="end"
                fill="#EABF8B">
            ${secondsToHumanReadable(this.progress.current)}
          </text>
          <text x=${145 + halfPr}
                y=${14 + halfPr}
                fill="#FFFFFF55"
                font-size="12"
                font-weight="bold"
                text-anchor="middle">|</text>
          <text x=${155 + halfPr}
                y=${15 + halfPr}
                font-size="12"
                text-anchor="start"
                fill="#FFFFFFA0">
            ${secondsToHumanReadable(this.progress.total)}
          </text>
        `}

        <foreignObject x=${ix}
                       y=${iy}
                       width=${iw}
                       height=${ih}>
          ${this.isInitialImageLoad
            ? html`<div class="h-full w-full rounded-full bg-white/10 animate-pulse"></div>`
            : html`<img alt="poster" draggable="false" class="select-none rounded-full h-full w-full object-cover" src="${this.resolvedImageUrl}" />`
          }
        </foreignObject>

        ${svg`
          <path d=${progressParams.pathFull}
                class="${progressParams.isLoading ? 'animate-pulse' : ''}"
                fill="none"
                stroke-width=${this.strokeWidth}
                stroke-linecap="round"
                stroke="#FFFFFF22" />

          ${!progressParams.isLoading
            ? svg`
              <path d=${progressParams.pathCurrent}
                    fill="none"
                    stroke-width=${this.strokeWidth}
                    stroke-linecap="round"
                    stroke="#EABF8B" />
              <circle cx=${progressParams.px}
                      cy=${progressParams.py}
                      fill="#EABF8B"
                      r=${pr} />
            `
            : ''
          }
        `}
      </svg>
    `;
  }
}
