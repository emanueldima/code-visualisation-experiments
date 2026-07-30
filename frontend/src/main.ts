import "./style.css";
import { createCamera } from "./camera";
import { createPathHighlighter } from "./path-highlighter";
import { createSourceTooltip } from "./source-tooltip";

type Rect = { x: number; y: number; width: number; height: number };
type MapNode = {
  id: number;
  ancestor_ids: number[];
  name: string;
  path: string;
  kind: "directory" | "file" | "chunk";
  lines: string[];
  slot: number;
  continuation: boolean;
  source?: string;
  columns: number;
  rows: number;
  start_byte?: number;
  end_byte?: number;
};
type MapBlock = {
  rect: Rect;
  nodes: MapNode[];
};
type MapResponse = {
  blocks: MapBlock[];
  lines_per_block: number;
  wrap_column: number;
  aspect_ratio: number;
  world_width: number;
  world_height: number;
};

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("Missing app element");

const token = new URLSearchParams(location.hash.slice(1)).get("token");
if (!token) {
  app.textContent = "Missing session token. Open the URL printed by the server.";
} else {
  fetch("/api/map", {
    headers: { Authorization: `Bearer ${token}` },
  })
    .then((response) => {
      if (!response.ok) throw new Error(`Project request failed (${response.status})`);
      return response.json() as Promise<MapResponse>;
    })
    .then(({ blocks, lines_per_block, wrap_column, world_width, world_height }) => {
      app.textContent = "";
      const map = renderMap(blocks, lines_per_block, wrap_column);
      map.style.width = `${world_width}px`;
      map.style.height = `${world_height}px`;
      app.append(map);
      const camera = createCamera(app, map, {
        fitScale: calculateFitScale(app, map),
      });
      createSourceTooltip(app, map);
      createPathHighlighter(map);
      const controls = document.createElement("div");
      controls.className = "view-controls";
      controls.dataset.cameraControl = "";
      const reset = document.createElement("button");
      reset.className = "reset-view";
      reset.type = "button";
      reset.textContent = "Reset view";
      reset.addEventListener("click", camera.reset);
      controls.append(reset);
      app.append(controls);
      const fit = () =>
        requestAnimationFrame(() => {
          camera.setFitScale(calculateFitScale(app, map));
        });
      const resizeObserver = new ResizeObserver(fit);
      resizeObserver.observe(app);
    })
    .catch((error: unknown) => {
      app.textContent = error instanceof Error ? error.message : "Unable to load project";
    });
}

function renderMap(blocks: MapBlock[], linesPerBlock: number, wrapColumn: number): HTMLElement {
  const map = document.createElement("main");
  map.className = "map";
  for (const block of blocks) {
    const element = document.createElement("section");
    element.className = "block";
    place(element, block.rect);
    for (const node of block.nodes) {
      element.append(renderNode(node, linesPerBlock, wrapColumn));
    }
    map.append(element);
  }
  return map;
}

function renderNode(node: MapNode, linesPerBlock: number, wrapColumn: number): HTMLElement {
  const element = document.createElement("article");
  element.className = `node ${node.kind}${node.continuation ? " continuation" : ""}`;
  element.dataset.nodeId = node.id.toString();
  element.dataset.ancestorIds = node.ancestor_ids.join(",");
  element.setAttribute("aria-label", node.path || node.name);
  if (node.kind !== "chunk") element.title = node.path || node.name;
  element.style.top = `${(node.slot / linesPerBlock) * 100}%`;
  element.style.height = `${(node.lines.length / linesPerBlock) * 100}%`;

  const lines = document.createElement("div");
  lines.className = "node-lines";
  for (const text of node.lines) {
    const line = document.createElement("div");
    line.className = "node-line";
    line.textContent = text;
    lines.append(line);
  }
  element.append(lines);

  if (node.kind === "chunk") {
    const content = document.createElement("div");
    content.className = "chunk-content";
    const source = document.createElement("pre");
    source.textContent = node.source ?? "";
    source.style.inlineSize = `${Math.min(node.columns, wrapColumn)}ch`;
    content.append(source);
    element.append(content);
  }
  return element;
}

function calculateFitScale(container: HTMLElement, map: HTMLElement): number {
  const availableWidth = container.clientWidth - 16;
  const availableHeight = container.clientHeight - 16;
  return Math.min(1, availableWidth / map.offsetWidth, availableHeight / map.offsetHeight);
}

function place(element: HTMLElement, rect: Rect): void {
  element.style.left = `${rect.x}px`;
  element.style.top = `${rect.y}px`;
  element.style.width = `${rect.width}px`;
  element.style.height = `${rect.height}px`;
}
