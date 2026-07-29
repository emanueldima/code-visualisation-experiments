export type SourceTooltip = {
  hide(): void;
  destroy(): void;
};

export function createSourceTooltip(
  viewport: HTMLElement,
  surface: HTMLElement,
  delay = 250,
): SourceTooltip {
  const tooltip = document.createElement("aside");
  tooltip.className = "source-tooltip";
  tooltip.dataset.cameraControl = "";
  tooltip.hidden = true;
  const source = document.createElement("pre");
  source.className = "source-tooltip-code";
  tooltip.append(source);
  viewport.append(tooltip);

  let activeChunk: HTMLElement | undefined;
  let chunkHovered = false;
  let tooltipHovered = false;
  let showTimer: number | undefined;
  let hideTimer: number | undefined;
  let pointer = { x: 0, y: 0 };

  const hide = () => {
    window.clearTimeout(showTimer);
    window.clearTimeout(hideTimer);
    showTimer = undefined;
    hideTimer = undefined;
    tooltip.hidden = true;
    activeChunk?.classList.remove("hovered");
  };
  const scheduleHide = () => {
    window.clearTimeout(hideTimer);
    if (chunkHovered || tooltipHovered) return;

    hideTimer = window.setTimeout(hide, 100);
  };
  const show = (chunk: HTMLElement) => {
    if (activeChunk !== chunk || !chunkHovered) return;

    source.textContent =
      chunk.querySelector<HTMLElement>(":scope > .chunk-content > pre")?.textContent ?? "";
    tooltip.hidden = false;
    placeTooltip(tooltip, pointer);
  };
  const onPointerOver = (event: PointerEvent) => {
    const chunk = closestChunk(event.target, surface);
    if (!chunk) return;
    if (event.relatedTarget instanceof Node && chunk.contains(event.relatedTarget)) return;

    window.clearTimeout(showTimer);
    window.clearTimeout(hideTimer);
    if (activeChunk !== chunk) tooltip.hidden = true;
    activeChunk?.classList.remove("hovered");
    activeChunk = chunk;
    activeChunk.classList.add("hovered");
    chunkHovered = true;
    pointer = { x: event.clientX, y: event.clientY };
    showTimer = window.setTimeout(() => show(chunk), delay);
  };
  const onPointerMove = (event: PointerEvent) => {
    if (!tooltip.hidden || closestChunk(event.target, surface) !== activeChunk) return;

    pointer = { x: event.clientX, y: event.clientY };
  };
  const onPointerOut = (event: PointerEvent) => {
    const chunk = closestChunk(event.target, surface);
    if (!chunk || chunk !== activeChunk) return;
    if (event.relatedTarget instanceof Node && chunk.contains(event.relatedTarget)) return;

    chunkHovered = false;
    chunk.classList.remove("hovered");
    scheduleHide();
  };
  const onTooltipEnter = () => {
    tooltipHovered = true;
    window.clearTimeout(hideTimer);
  };
  const onTooltipLeave = () => {
    tooltipHovered = false;
    scheduleHide();
  };
  const destroy = () => {
    hide();
    surface.removeEventListener("pointerover", onPointerOver);
    surface.removeEventListener("pointermove", onPointerMove);
    surface.removeEventListener("pointerout", onPointerOut);
    tooltip.removeEventListener("pointerenter", onTooltipEnter);
    tooltip.removeEventListener("pointerleave", onTooltipLeave);
    tooltip.remove();
  };

  surface.addEventListener("pointerover", onPointerOver);
  surface.addEventListener("pointermove", onPointerMove);
  surface.addEventListener("pointerout", onPointerOut);
  tooltip.addEventListener("pointerenter", onTooltipEnter);
  tooltip.addEventListener("pointerleave", onTooltipLeave);
  return { hide, destroy };
}

function closestChunk(target: EventTarget | null, surface: HTMLElement): HTMLElement | undefined {
  if (!(target instanceof Element)) return undefined;

  const chunk = target.closest<HTMLElement>(".chunk");
  return chunk && surface.contains(chunk) ? chunk : undefined;
}

function placeTooltip(tooltip: HTMLElement, pointer: { x: number; y: number }): void {
  const gap = 12;
  const margin = 8;
  const width = tooltip.offsetWidth;
  const height = tooltip.offsetHeight;
  const right = pointer.x + gap + width;
  const left = right <= innerWidth - margin ? pointer.x + gap : pointer.x - gap - width;

  tooltip.style.left = `${Math.max(margin, left)}px`;
  tooltip.style.top = `${Math.min(innerHeight - height - margin, Math.max(margin, pointer.y + gap))}px`;
}
