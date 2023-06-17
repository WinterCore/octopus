<script lang="ts">
    import {getArc} from "../lib/math"
    import {preloadImage} from "../lib/image";
    import {fly} from "svelte/transition";
    import {secondsToTime} from "../lib/time";

    export let strokeWidth: number;
    export let time: number;
    export let totalTime: number;
    export let image: string = "https://en.gravatar.com/userimage/59286606/5642fce1fc1062708b31c90e2c381a55.jpg?size=512";

    const startAngle = 0;
    const endAngle = Math.PI * 1.65;
    const rotate = Math.PI * (1.675);

    const offsetY = 10;
    const cx = 150;
    const cy = 150 + offsetY;

    const pr = 6;
    const halfPr = pr / 2;

    const halfStroke = strokeWidth / 2;
    const radius = 150 - pr;
    const r = radius - halfStroke;

    const imagePadding = 20;

    const ix = imagePadding + pr;
    const iy = imagePadding + offsetY + pr;
    const iw = (radius * 2) - (imagePadding * 2);
    const ih = (radius * 2) - (imagePadding * 2);

    const pathFull = getArc([cx, cy], [r, r], startAngle, endAngle, rotate);
    const percentage = (time / totalTime);
    const currEndAngle = endAngle * percentage;
    const pathCurrent = getArc([cx, cy], [r, r], startAngle, currEndAngle, rotate);

    const gg = currEndAngle + rotate;
    const px = Math.cos(gg) * r + cx;
    const py = Math.sin(gg) * r + cy;
</script>

<svg xmlns="http://www.w3.org/2000/svg"
     {...$$restProps}
     class="select-none"
     viewBox="0 0 300 310"
     version="1.1">
    <text x={105 + halfPr}
          y={15 + halfPr}
          font-size="12"
          text-anchor="right"
          fill="#FFFFFFA0">
        {secondsToTime(time)}
    </text>
    <text x={145 + halfPr} y={13.5 + halfPr} fill="#FFFFFF55" font-size="12" text-anchor="middle">|</text>
    <text x={155 + halfPr}
          y={15 + halfPr}
          font-size="12"
          text-anchor="left"
          fill="#EABF8B">
        {secondsToTime(totalTime)}
    </text>
    <foreignObject x={ix}
                   y={iy}
                   width={iw}
                   height={ih}>
        {#await preloadImage(image)}
            <div class="bg-white/20 w-full h-full rounded-full"></div>
        {:then}
            <img in:fly alt="poster"
                 class="rounded-full"
                 src={image} />
        {:catch}
            <div class="bg-white/20 w-full h-full rounded-full"></div>
        {/await}
    </foreignObject>
    <path d={pathFull}
          fill="none"
          stroke-width={strokeWidth}
          stroke-linecap="round"
          stroke="#FFFFFF22" />

    <path d={pathCurrent}
          fill="none"
          stroke-width={strokeWidth}
          stroke-linecap="round"
          stroke="#EABF8B" />

    <circle cx={px} cy={py} fill="#EABF8B" r={pr} />
</svg>
