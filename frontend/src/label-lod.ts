export function updateLabelLod(surface: HTMLElement, scale: number): void {
  const labelScreenSize = Math.min(24, Math.max(8, 48 * scale));

  surface.style.setProperty("--file-label-scale", `${labelScreenSize / (9 * scale)}`);
}
