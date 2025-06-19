export const clamp = (low: number, high: number, x: number) =>
    x < low ? low : (x > high) ? high : x;
