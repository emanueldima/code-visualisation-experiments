import "./style.css";
import { createCamera } from "./camera";
import { updateLabelLod } from "./label-lod";

type Rect = { x: number; y: number; width: number; height: number };
type MapNode = {
  name: string;
  path: string;
  kind: "directory" | "file" | "chunk";
  rect: Rect;
  source?: string;
  children: MapNode[];
  columns: number;
  rows: number;
  start_byte?: number;
  end_byte?: number;
};
type MapResponse = {
  root: MapNode;
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
  const viewportAspect = innerWidth / innerHeight;
  fetch(`/api/map?aspect=${viewportAspect}`, {
    headers: { Authorization: `Bearer ${token}` },
  })
    .then((response) => {
      if (!response.ok) throw new Error(`Project request failed (${response.status})`);
      return response.json() as Promise<MapResponse>;
    })
    .then(({ root, wrap_column, world_width, world_height }) => {
      app.textContent = "";
      const map = renderNode(root, wrap_column, true);
      map.style.width = `${world_width}px`;
      map.style.height = `${world_height}px`;
      app.append(map);
      const camera = createCamera(app, map, {
        fitScale: calculateFitScale(app, map),
        onScaleChange: (scale, fitScale) => updateLabelLod(map, scale, fitScale),
      });
      const reset = document.createElement("button");
      reset.className = "reset-view";
      reset.type = "button";
      reset.textContent = "Reset view";
      reset.addEventListener("click", camera.reset);
      app.append(reset);
      const fit = () =>
        requestAnimationFrame(() => {
          camera.setFitScale(calculateFitScale(app, map));
        });
      new ResizeObserver(fit).observe(app);
    })
    .catch((error: unknown) => {
      app.textContent = error instanceof Error ? error.message : "Unable to load project";
    });
}

function renderNode(node: MapNode, wrapColumn: number, root = false): HTMLElement {
  const element = document.createElement("section");
  element.className = `node ${node.kind}${root ? " root" : ""}`;
  element.setAttribute("aria-label", node.path || node.name);
  if (!root) place(element, node.rect);

  const label = document.createElement("div");
  label.className = "label";
  const pathCharacters = [...node.path];
  label.textContent =
    node.kind === "file"
      ? pathCharacters.length > 50
        ? `…${pathCharacters.slice(-49).join("")}`
        : node.path
      : node.name;
  label.title = node.path || node.name;
  element.append(label);

  if (node.kind === "chunk") {
    const content = document.createElement("div");
    content.className = "chunk-content";
    const source = document.createElement("pre");
    source.textContent = node.source ?? "";
    source.style.inlineSize = `${Math.min(node.columns, wrapColumn)}ch`;
    content.append(source);
    element.append(content);
    return element;
  }

  const children = document.createElement("div");
  children.className = "children";
  for (const child of node.children) children.append(renderNode(child, wrapColumn));
  element.append(children);
  return element;
}

function calculateFitScale(container: HTMLElement, map: HTMLElement): number {
  const availableWidth = container.clientWidth - 16;
  const availableHeight = container.clientHeight - 16;
  return Math.min(1, availableWidth / map.offsetWidth, availableHeight / map.offsetHeight);
}

function place(element: HTMLElement, rect: Rect): void {
  element.style.left = `${rect.x * 100}%`;
  element.style.top = `${rect.y * 100}%`;
  element.style.width = `${rect.width * 100}%`;
  element.style.height = `${rect.height * 100}%`;
}
