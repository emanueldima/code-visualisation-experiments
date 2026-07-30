export type PathHighlighter = {
  destroy(): void;
};

export function createPathHighlighter(surface: HTMLElement): PathHighlighter {
  const segmentsById = new Map<string, HTMLElement[]>();
  for (const node of surface.querySelectorAll<HTMLElement>(".node[data-node-id]")) {
    const id = node.dataset.nodeId;
    if (!id) continue;

    const segments = segmentsById.get(id) ?? [];
    segments.push(node);
    segmentsById.set(id, segments);
  }

  let highlighted: HTMLElement[] = [];
  const clear = () => {
    for (const node of highlighted) node.classList.remove("path-hovered");
    highlighted = [];
  };
  const highlight = (node: HTMLElement) => {
    clear();
    const ids = node.dataset.ancestorIds?.split(",").filter(Boolean) ?? [];
    const nodeId = node.dataset.nodeId;
    if (nodeId) ids.push(nodeId);
    for (const id of ids) {
      for (const segment of segmentsById.get(id) ?? []) {
        segment.classList.add("path-hovered");
        highlighted.push(segment);
      }
    }
  };
  const onPointerOver = (event: PointerEvent) => {
    const node = closestNode(event.target, surface);
    if (!node) return;
    if (event.relatedTarget instanceof Node && node.contains(event.relatedTarget)) return;

    highlight(node);
  };
  const onPointerOut = (event: PointerEvent) => {
    const node = closestNode(event.target, surface);
    if (!node) return;
    if (event.relatedTarget instanceof Node && node.contains(event.relatedTarget)) return;
    if (closestNode(event.relatedTarget, surface)) return;

    clear();
  };
  const destroy = () => {
    clear();
    surface.removeEventListener("pointerover", onPointerOver);
    surface.removeEventListener("pointerout", onPointerOut);
  };

  surface.addEventListener("pointerover", onPointerOver);
  surface.addEventListener("pointerout", onPointerOut);
  return { destroy };
}

function closestNode(target: EventTarget | null, surface: HTMLElement): HTMLElement | undefined {
  if (!(target instanceof Element)) return undefined;

  const node = target.closest<HTMLElement>(".node");
  return node && surface.contains(node) ? node : undefined;
}
