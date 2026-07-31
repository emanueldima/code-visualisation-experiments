import { trimBlankLines } from "./source-text.ts";

type SourceNode = {
  breadcrumbs: { id: number; path: string }[];
  flow?: { source: string };
  start_byte?: number;
  end_byte?: number;
  children: SourceNode[];
};

type Box = {
  top: number;
  right: number;
  left: number;
};

type Size = {
  width: number;
  height: number;
};

type Position = {
  left: number;
  top: number;
};

export type SourcePreview = {
  destroy(): void;
};

const SHOW_DELAY = 250;
const HIDE_DELAY = 100;
const POPUP_GAP = 8;
const VIEWPORT_PADDING = 12;

export function createSourceResolver(
  structureRoot: SourceNode,
  sourceRoot: SourceNode,
): (id: string) => string | undefined {
  const fileSources = new Map<string, Uint8Array>();
  const ranges = new Map<string, { file: string; start: number; end: number }>();
  const cache = new Map<string, string>();
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const indexFiles = (node: SourceNode) => {
    if (node.flow) {
      const path = node.breadcrumbs.at(-1)?.path;
      if (path !== undefined) fileSources.set(path, encoder.encode(node.flow.source));
    }
    for (const child of node.children) indexFiles(child);
  };
  const indexRanges = (node: SourceNode) => {
    if (node.start_byte !== undefined && node.end_byte !== undefined) {
      for (const breadcrumb of node.breadcrumbs) {
        const separator = breadcrumb.path.lastIndexOf("#");
        if (separator < 0) continue;
        ranges.set(breadcrumb.id.toString(), {
          file: breadcrumb.path.slice(0, separator),
          start: node.start_byte,
          end: node.end_byte,
        });
      }
    }
    for (const child of node.children) indexRanges(child);
  };
  indexFiles(sourceRoot);
  indexRanges(structureRoot);

  return (id) => {
    if (cache.has(id)) return cache.get(id);

    const range = ranges.get(id);
    const file = range && fileSources.get(range.file);
    if (!range || !file) return undefined;
    const source = trimBlankLines(decoder.decode(file.slice(range.start, range.end)));
    cache.set(id, source);
    return source;
  };
}

export function createSourcePreview(
  viewport: HTMLElement,
  surface: HTMLElement,
  sourceForId: (id: string) => string | undefined,
): SourcePreview {
  const popup = document.createElement("aside");
  popup.className = "source-preview";
  popup.dataset.cameraControl = "";
  popup.setAttribute("role", "region");
  popup.setAttribute("aria-label", "Source preview");
  popup.hidden = true;
  const code = document.createElement("pre");
  code.className = "source-preview-code";
  popup.append(code);
  viewport.append(popup);

  let activeNode: HTMLElement | undefined;
  let showTimer: number | undefined;
  let hideTimer: number | undefined;
  const cancelTimers = () => {
    window.clearTimeout(showTimer);
    window.clearTimeout(hideTimer);
    showTimer = undefined;
    hideTimer = undefined;
  };
  const hide = () => {
    cancelTimers();
    activeNode = undefined;
    popup.hidden = true;
  };
  const show = (node: HTMLElement) => {
    if (activeNode !== node) return;

    const id = node.dataset.previewSourceId;
    const source = id === undefined ? undefined : sourceForId(id);
    if (source === undefined) return;

    code.textContent = source;
    popup.hidden = false;
    positionPopup(popup, node);
  };
  const onPointerOver = (event: PointerEvent) => {
    const node = closestPreview(event.target, surface);
    if (!node) return;
    if (event.relatedTarget instanceof Node && node.contains(event.relatedTarget)) return;

    cancelTimers();
    popup.hidden = true;
    activeNode = node;
    showTimer = window.setTimeout(() => show(node), SHOW_DELAY);
  };
  const onPointerOut = (event: PointerEvent) => {
    const node = closestPreview(event.target, surface);
    if (!node) return;
    if (event.relatedTarget instanceof Node && node.contains(event.relatedTarget)) return;

    window.clearTimeout(showTimer);
    showTimer = undefined;
    if (event.relatedTarget instanceof Node && popup.contains(event.relatedTarget)) return;
    hideTimer = window.setTimeout(hide, HIDE_DELAY);
  };
  const onPopupEnter = () => window.clearTimeout(hideTimer);
  const onPopupLeave = (event: PointerEvent) => {
    if (
      activeNode &&
      event.relatedTarget instanceof Node &&
      activeNode.contains(event.relatedTarget)
    ) {
      return;
    }
    hide();
  };
  const destroy = () => {
    cancelTimers();
    surface.removeEventListener("pointerover", onPointerOver);
    surface.removeEventListener("pointerout", onPointerOut);
    popup.removeEventListener("pointerenter", onPopupEnter);
    popup.removeEventListener("pointerleave", onPopupLeave);
    popup.remove();
  };

  surface.addEventListener("pointerover", onPointerOver);
  surface.addEventListener("pointerout", onPointerOut);
  popup.addEventListener("pointerenter", onPopupEnter);
  popup.addEventListener("pointerleave", onPopupLeave);
  return { destroy };
}

function closestPreview(target: EventTarget | null, surface: HTMLElement): HTMLElement | undefined {
  if (!(target instanceof Element)) return undefined;

  const node = target.closest<HTMLElement>(".node[data-preview-source-id]");
  if (!node || !surface.contains(node)) return undefined;
  return node;
}

function positionPopup(popup: HTMLElement, anchor: HTMLElement): void {
  const anchorRect = anchor.getBoundingClientRect();
  const popupRect = popup.getBoundingClientRect();
  const position = calculatePopupPosition(
    anchorRect,
    popupRect,
    { width: window.innerWidth, height: window.innerHeight },
  );
  popup.style.left = `${position.left}px`;
  popup.style.top = `${position.top}px`;
}

export function calculatePopupPosition(anchor: Box, popup: Size, viewport: Size): Position {
  let left = anchor.right + POPUP_GAP;
  if (left + popup.width > viewport.width - VIEWPORT_PADDING) {
    left = anchor.left - popup.width - POPUP_GAP;
  }
  return {
    left: Math.min(
      Math.max(VIEWPORT_PADDING, left),
      viewport.width - popup.width - VIEWPORT_PADDING,
    ),
    top: Math.min(
      Math.max(VIEWPORT_PADDING, anchor.top),
      viewport.height - popup.height - VIEWPORT_PADDING,
    ),
  };
}
