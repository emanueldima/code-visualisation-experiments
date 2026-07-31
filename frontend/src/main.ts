import "./style.css";
import { createCamera, type Camera } from "./camera";
import { createMapLoader } from "./map-loader";
import { createPathHighlighter, type PathHighlighter } from "./path-highlighter";
import {
  createSourceResolver,
  createSourcePreview,
  type SourcePreview,
} from "./source-preview";

type Rect = { x: number; y: number; width: number; height: number };
type Breadcrumb = {
  id: number;
  name: string;
  path: string;
  kind: "directory" | "file" | "container" | "source";
};
type MapNode = {
  ancestor_ids: number[];
  breadcrumbs: Breadcrumb[];
  kind: Breadcrumb["kind"];
  rect: Rect;
  header_height: number;
  source?: string;
  start_byte?: number;
  end_byte?: number;
  flow?: SourceFlow;
  children: MapNode[];
};
type SourceFlow = {
  source: string;
  columns: { rows: SourceRow[] }[];
};
type SourceRow = {
  runs: { start_byte: number; end_byte: number; highlighted: boolean }[];
  highlighted: boolean;
};
type PackedMap = {
  root: MapNode;
  aspect_ratio: number;
  world_width: number;
  world_height: number;
};
type MapResponse = {
  structure: PackedMap;
  source: PackedMap;
  wrap_column: number;
};
type LayoutMode = "structure" | "source";

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("Missing app element");

const token = new URLSearchParams(location.hash.slice(1)).get("token");
if (!token) {
  app.textContent = "Missing session token. Open the URL printed by the server.";
} else {
  const requestMap = (publicOnly = false) =>
    fetch(`/api/map${publicOnly ? "?public=true" : ""}`, {
      headers: { Authorization: `Bearer ${token}` },
    })
    .then((response) => {
      if (!response.ok) throw new Error(`Project request failed (${response.status})`);
      return response.json() as Promise<MapResponse>;
    });
  requestMap()
    .then((response) => {
      app.textContent = "";
      const controls = document.createElement("div");
      controls.className = "view-controls";
      controls.dataset.cameraControl = "";
      const toggle = document.createElement("button");
      toggle.className = "view-button";
      toggle.type = "button";
      const publicToggle = document.createElement("button");
      publicToggle.className = "view-button";
      publicToggle.type = "button";
      publicToggle.textContent = "Public only";
      publicToggle.setAttribute("aria-pressed", "false");
      const reset = document.createElement("button");
      reset.className = "view-button";
      reset.type = "button";
      reset.textContent = "Reset view";
      const zoom = document.createElement("output");
      zoom.className = "zoom-value";
      zoom.setAttribute("aria-live", "polite");
      controls.append(zoom, toggle, publicToggle, reset);
      app.append(controls);

      let mode: LayoutMode = "structure";
      let publicOnly = false;
      let currentResponse = response;
      let map = renderPackedMap(response.structure, response.wrap_column);
      let camera: Camera;
      let highlighter: PathHighlighter;
      let preview: SourcePreview | undefined;
      const loadMap = createMapLoader(response, () => requestMap(true));
      const renderedMaps = new Map<string, HTMLElement>([["all:structure", map]]);
      const mount = () => {
        camera?.destroy();
        highlighter?.destroy();
        preview?.destroy();
        map.remove();
        const key = `${publicOnly ? "public" : "all"}:${mode}`;
        map =
          renderedMaps.get(key) ??
          renderPackedMap(currentResponse[mode], currentResponse.wrap_column);
        renderedMaps.set(key, map);
        app.insertBefore(map, controls);
        highlighter = createPathHighlighter(map);
        const sourceForId = createSourceResolver(
          currentResponse.structure.root,
          currentResponse.source.root,
        );
        preview =
          mode === "structure" ? createSourcePreview(app, map, sourceForId) : undefined;
        camera = createCamera(app, map, {
          fitScale: calculateFitScale(app, map),
          onScaleChange: (scale) => {
            zoom.textContent = `Zoom ${scale.toFixed(2)}×`;
          },
        });
        toggle.textContent = mode === "structure" ? "Show source" : "Show structure";
        publicToggle.textContent = "Public only";
        publicToggle.setAttribute("aria-pressed", publicOnly.toString());
      };
      toggle.addEventListener("click", () => {
        mode = mode === "structure" ? "source" : "structure";
        mount();
      });
      publicToggle.addEventListener("click", async () => {
        const next = !publicOnly;
        publicToggle.disabled = true;
        if (next) publicToggle.textContent = "Loading public…";
        try {
          currentResponse = await loadMap(next);
          publicOnly = next;
          mount();
        } catch {
          publicToggle.textContent = "Retry public";
        } finally {
          publicToggle.disabled = false;
        }
      });
      reset.addEventListener("click", () => camera.reset());
      mount();

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

function renderPackedMap(packed: PackedMap, wrapColumn: number): HTMLElement {
  const map = document.createElement("main");
  map.className = "map";
  map.style.width = `${packed.world_width}px`;
  map.style.height = `${packed.world_height}px`;
  map.append(renderNode(packed.root, wrapColumn));
  return map;
}

function renderNode(node: MapNode, wrapColumn: number): HTMLElement {
  const element = document.createElement("article");
  element.className = `node ${node.kind}`;
  element.dataset.nodeIds = node.breadcrumbs.map(({ id }) => id).join(",");
  element.dataset.ancestorIds = node.ancestor_ids.join(",");
  element.style.setProperty("--content-top", `${node.header_height + 10}px`);
  element.style.setProperty("--flow-top", `${node.header_height + 8}px`);
  const last = node.breadcrumbs.at(-1);
  const implementation = node.breadcrumbs
    .slice()
    .reverse()
    .find(({ name }) => name === "impl" || name.startsWith("impl "));
  const preview = node.kind === "source" && node.children.length === 0 ? last : implementation;
  if (preview) element.dataset.previewSourceId = preview.id.toString();
  element.setAttribute("aria-label", last?.path || last?.name || node.kind);
  place(element, node.rect);

  if (node.header_height > 0) {
    const header = document.createElement("header");
    header.className = "node-header";
    for (const [index, breadcrumb] of node.breadcrumbs.entries()) {
      if (index > 0) {
        const separator = document.createElement("span");
        separator.className = "breadcrumb-separator";
        separator.textContent = "/";
        header.append(separator);
      }
      const label = document.createElement("span");
      label.className = "breadcrumb";
      label.dataset.nodeId = breadcrumb.id.toString();
      label.textContent = breadcrumb.name;
      header.append(label);
    }
    element.append(header);
  }

  if (node.flow) element.append(renderSourceFlow(node.flow, wrapColumn));
  for (const child of node.children) {
    element.append(renderNode(child, wrapColumn));
  }
  return element;
}

function renderSourceFlow(flow: SourceFlow, wrapColumn: number): HTMLElement {
  const element = document.createElement("section");
  element.className = "source-flow";
  const bytes = new TextEncoder().encode(flow.source);
  const decoder = new TextDecoder();
  for (const sourceColumn of flow.columns) {
    const column = document.createElement("div");
    column.className = "source-column";
    column.style.inlineSize = `${wrapColumn}ch`;
    for (const sourceRow of sourceColumn.rows) {
      const row = document.createElement("div");
      row.className = `source-row${sourceRow.highlighted ? " highlighted" : ""}`;
      for (const sourceRun of sourceRow.runs) {
        const run = document.createElement("span");
        run.className = `source-run${sourceRun.highlighted ? " highlighted" : ""}`;
        run.textContent = decoder.decode(
          bytes.slice(sourceRun.start_byte, sourceRun.end_byte),
        );
        row.append(run);
      }
      column.append(row);
    }
    element.append(column);
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
