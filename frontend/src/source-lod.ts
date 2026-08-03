import type { MapNode, Rect, SourceFlow } from "./map-types";

export type SourceLodOpacities = {
  detailed: number;
  medium: number;
  coarse: number;
};

type SourceLodTargets = {
  headers: HTMLElement[];
  flows: HTMLElement[];
  medium: HTMLElement;
  coarse: HTMLElement;
};

const DETAILED_ONLY_SCALE = 0.24;
const MEDIUM_ONLY_SCALE = 0.16;
const MEDIUM_TO_COARSE_SCALE = 0.09;
const COARSE_ONLY_SCALE = 0.05;
const TARGETS = new WeakMap<HTMLElement, SourceLodTargets>();

export function renderSourceLods(root: MapNode, wrapColumn: number): HTMLElement[] {
  return [renderLayer(root, wrapColumn, "medium"), renderLayer(root, wrapColumn, "coarse")];
}

export function applySourceLodScale(map: HTMLElement, scale: number): void {
  const opacity = sourceLodOpacities(scale);
  const targets = lodTargets(map);
  for (const header of targets.headers) header.style.opacity = opacity.detailed.toString();
  for (const flow of targets.flows) flow.style.color = textColor(opacity.detailed);
  targets.medium.style.color = textColor(opacity.medium);
  targets.coarse.style.color = textColor(opacity.coarse);
  map.classList.toggle("lod-detailed-hidden", opacity.detailed === 0);
  map.classList.toggle("lod-medium-hidden", opacity.medium === 0);
  map.classList.toggle("lod-coarse-hidden", opacity.coarse === 0);
}

export function sourceLodOpacities(scale: number): SourceLodOpacities {
  if (scale >= DETAILED_ONLY_SCALE) return { detailed: 1, medium: 0, coarse: 0 };
  if (scale > MEDIUM_ONLY_SCALE) {
    const detailed = blend(scale, MEDIUM_ONLY_SCALE, DETAILED_ONLY_SCALE);
    return { detailed, medium: 1 - detailed, coarse: 0 };
  }
  if (scale >= MEDIUM_TO_COARSE_SCALE) return { detailed: 0, medium: 1, coarse: 0 };
  if (scale > COARSE_ONLY_SCALE) {
    const medium = blend(scale, COARSE_ONLY_SCALE, MEDIUM_TO_COARSE_SCALE);
    return { detailed: 0, medium, coarse: 1 - medium };
  }
  return { detailed: 0, medium: 0, coarse: 1 };
}

function renderLayer(root: MapNode, wrapColumn: number, level: "medium" | "coarse") {
  const layer = document.createElement("section");
  layer.className = `source-lod-layer ${level}`;
  layer.setAttribute("aria-hidden", "true");
  layer.append(renderRegion(root, wrapColumn, level));
  return layer;
}

function renderRegion(
  node: MapNode,
  wrapColumn: number,
  level: "medium" | "coarse",
): HTMLElement {
  const region = document.createElement("div");
  region.className = `source-lod-region ${node.kind}`;
  place(region, node.rect);

  if (node.kind === "directory") region.append(renderLabel(node, level));
  for (const child of node.children) {
    if (child.kind === "directory") {
      region.append(renderRegion(child, wrapColumn, level));
      continue;
    }
    if (child.kind !== "file") continue;

    const label = renderLabel(child, level);
    placeAtOrigin(label, child.rect);
    region.append(label);
    if (level === "medium" && child.flow) {
      region.append(renderFileChunks(child, child.flow, wrapColumn));
    }
  }
  return region;
}

function renderLabel(node: MapNode, level: "medium" | "coarse"): HTMLElement {
  const label = document.createElement("div");
  label.className = `source-lod-label ${level} ${node.kind}`;
  label.textContent = node.breadcrumbs.map(({ name }) => name).join("/");
  return label;
}

function renderFileChunks(node: MapNode, flow: SourceFlow, wrapColumn: number): HTMLElement {
  const file = document.createElement("div");
  file.className = "source-lod-file";
  place(file, node.rect);
  file.append(renderChunkFlow(flow, wrapColumn, node.header_height));
  return file;
}

function renderChunkFlow(flow: SourceFlow, wrapColumn: number, headerHeight: number): HTMLElement {
  const element = document.createElement("div");
  element.className = "source-lod-flow";
  element.style.top = `${headerHeight + 8}px`;
  const bytes = new TextEncoder().encode(flow.source);
  const decoder = new TextDecoder();
  for (const sourceColumn of flow.columns) {
    const column = document.createElement("div");
    column.className = "source-lod-column";
    column.style.inlineSize = `${wrapColumn}ch`;
    for (const sourceRow of sourceColumn.rows) {
      const row = document.createElement("div");
      row.className = `source-lod-row${sourceRow.highlighted ? " highlighted" : ""}`;
      for (const sourceRun of sourceRow.runs) {
        const text = decoder.decode(bytes.slice(sourceRun.start_byte, sourceRun.end_byte));
        if (sourceRun.highlighted) {
          const anchor = document.createElement("span");
          anchor.className = "source-lod-chunk-anchor";
          const label = document.createElement("span");
          label.className = "source-lod-chunk-label";
          label.textContent = text;
          anchor.append(label);
          row.append(anchor);
          continue;
        }
        const spacer = document.createElement("span");
        spacer.className = "source-lod-spacer";
        spacer.textContent = text;
        row.append(spacer);
      }
      column.append(row);
    }
    element.append(column);
  }
  return element;
}

function place(element: HTMLElement, rect: Rect): void {
  element.style.left = `${rect.x}px`;
  element.style.top = `${rect.y}px`;
  element.style.width = `${rect.width}px`;
  element.style.height = `${rect.height}px`;
}

function placeAtOrigin(element: HTMLElement, rect: Rect): void {
  element.style.left = `${rect.x}px`;
  element.style.top = `${rect.y}px`;
}

function blend(value: number, low: number, high: number): number {
  return (value - low) / (high - low);
}

function lodTargets(map: HTMLElement): SourceLodTargets {
  const cached = TARGETS.get(map);
  if (cached) return cached;

  const medium = map.querySelector<HTMLElement>(".source-lod-layer.medium");
  const coarse = map.querySelector<HTMLElement>(".source-lod-layer.coarse");
  if (!medium || !coarse) throw new Error("Missing source LOD layers");

  const targets = {
    headers: [...map.querySelectorAll<HTMLElement>(".node-header")],
    flows: [...map.querySelectorAll<HTMLElement>(".source-flow")],
    medium,
    coarse,
  };
  TARGETS.set(map, targets);
  return targets;
}

function textColor(opacity: number): string {
  return `rgb(23 23 23 / ${opacity})`;
}
