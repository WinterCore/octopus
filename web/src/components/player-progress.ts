import {html, LitElement, svg} from "lit";
import {customElement, property} from "lit/decorators.js";
import {secondsToHumanReadable} from "../utils/time";
import {getArc} from "../utils/svg";
import {clamp} from "../utils/math";

export interface ITimeProgress {
  readonly current: number;
  readonly total: number;
}

export interface IAudioMetadata {
  readonly name: string;
  readonly image: string | undefined;
  readonly author: string | undefined;
}

@customElement("player-progress")
export class PlayerProgress extends LitElement {
  protected createRenderRoot(): HTMLElement | DocumentFragment {
    return this;
  }

  @property({ type: String })
  className: string = "";

  @property({ type: Object })
  progress?: ITimeProgress;

  @property({ type: Object })
  metadata?: IAudioMetadata;

  @property({ type: Number })
  strokeWidth: number = 4;

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
    
    function getProgressParams(progress: ITimeProgress) {
      const { current, total } = progress;

      const pathFull = getArc([cx, cy], [r, r], startAngle, endAngle, rotate);
      const percentage = clamp(0, 1, (current / total));
      const currEndAngle = endAngle * percentage;
      const pathCurrent = getArc([cx, cy], [r, r], startAngle, currEndAngle, rotate);

      const gg = currEndAngle + rotate;
      const px = Math.cos(gg) * r + cx;
      const py = Math.sin(gg) * r + cy;

      return {
        pathFull,
        pathCurrent,
        px,
        py,
      };
    }
    
    const progressParams = this.progress ? getProgressParams(this.progress) : null;

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
          <img alt="poster" draggable="false" class="select-none rounded-full h-full w-full object-cover" src="${this.metadata?.image ?? "/logo.webp"}" />
        </foreignObject>

        ${progressParams && svg`
          <path d=${progressParams.pathFull}
                fill="none"
                stroke-width=${this.strokeWidth}
                stroke-linecap="round"
                stroke="#FFFFFF22" />

          <path d=${progressParams.pathCurrent}
                fill="none"
                stroke-width=${this.strokeWidth}
                stroke-linecap="round"
                stroke="#EABF8B" />

          <circle cx=${progressParams.px}
                  cy=${progressParams.py}
                  fill="#EABF8B"
                  r=${pr} />
        `}
      </svg>
    `;
  }
}
