export function updateLabelLod(surface: HTMLElement, scale: number, fitScale: number): void {
  const zoomRatio = scale / Math.max(fitScale, Number.EPSILON);
  const overviewProgress = smoothstep(0.5, 1, zoomRatio);
  const fileProgress = smoothstep(1.2, 3.5, zoomRatio);
  const sourceReadable = smoothstep(0.45, 0.85, scale);
  const fileScreenSize =
    zoomRatio < 1 ? 10 + overviewProgress * 6 : 16 - fileProgress * 6;
  const chunkScreenSize =
    zoomRatio < 1 ? 9 + overviewProgress * 5 : 14 - sourceReadable * 5;

  surface.style.setProperty("--file-label-scale", `${fileScreenSize / (9 * scale)}`);
  surface.style.setProperty("--chunk-label-scale", `${chunkScreenSize / (9 * scale)}`);
  surface.style.setProperty("--chunk-label-opacity", `${1 - sourceReadable}`);
}

function smoothstep(start: number, end: number, value: number): number {
  const progress = Math.min(1, Math.max(0, (value - start) / (end - start)));
  return progress * progress * (3 - 2 * progress);
}
