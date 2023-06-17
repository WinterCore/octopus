const prefixZeroes = (v: number, n: number) =>
    v.toString().padStart(n, "0");

export const secondsToTime = (seconds: number) => {
    const hours = Math.floor(seconds / 60 / 60);
    const minutes = Math.floor(seconds / 60); 
    const secs = seconds % 60;

    if (hours > 0) {
        return `${hours}:${prefixZeroes(minutes, 2)}:${prefixZeroes(secs, 2)}`;
    }

    return `${prefixZeroes(minutes, 2)}:${prefixZeroes(secs, 2)}`;
};
