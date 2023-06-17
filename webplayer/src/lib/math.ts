
const matrixTimes = (
    [[a,b], [c,d]]: [[number, number], [number, number]],
    [x,y]: [number, number]
): [number, number] => [a * x + b * y, c * x + d * y];

const rotateMatrix = (x: number): [[number, number], [number, number]] => [
    [Math.cos(x), -Math.sin(x)],
    [Math.sin(x), Math.cos(x)],
];

const vecAdd = (
    [a1, a2]: [number, number],
    [b1, b2]: [number, number],
) => [a1 + b1, a2 + b2];

export const getArc = (
    [cx, cy]: [number, number],
    [rx, ry]: [number, number],
    startAngle: number,
    sweep: number,
    rotate: number,
) => {
    sweep = sweep % (2 * Math.PI);
    const rotMatrix = rotateMatrix(rotate);
    const [sx, sy] = vecAdd(
        matrixTimes(
            rotMatrix,
            [rx * Math.cos(startAngle), ry * Math.sin(startAngle)],
        ),
        [cx,cy],
    );

    const [ex, ey] = vecAdd(
        matrixTimes(
            rotMatrix,
            [
                rx * Math.cos(startAngle + sweep),
                ry * Math.sin(startAngle + sweep)
            ],
        ),
        [cx,cy],
    );

    const laf = (sweep > Math.PI) ? 1 : 0;
    const sf = (sweep > 0) ? 1 : 0;
    const angle = rotate / (2 * Math.PI) * 360;

    return `M ${sx} ${sy} A ${rx} ${ry} ${angle} ${laf} ${sf} ${ex} ${ey}`;
};
