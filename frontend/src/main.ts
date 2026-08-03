import "./style.css";
import { createCamera, type Camera } from "./camera";
import { createCachedLoader } from "./map-loader";
import type { MapNode, PackedMap, Rect, SourceFlow } from "./map-types";
import { createPathHighlighter, type PathHighlighter } from "./path-highlighter";
import {
  createSourceResolver,
  createCallPreviewResolver,
  createSourcePreview,
  type SourcePreview,
} from "./source-preview";
import { applySourceLodScale, renderSourceLods } from "./source-lod";

type MapResponse = {
  structure: PackedMap;
  source: PackedMap;
  wrap_column: number;
};
type CallsResponse = {
  structure: PackedMap;
  source: PackedMap;
  wrap_column: number;
};
type PrimaryMode = "structure" | "calls";

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("Missing app element");

const token = new URLSearchParams(location.hash.slice(1)).get("token");
if (!token) {
  app.textContent = "Missing session token. Open the URL printed by the server.";
} else {
  const requestMap = (publicOnly = false, showTests = false) => {
    const query = new URLSearchParams();
    if (publicOnly) query.set("public", "true");
    if (showTests) query.set("tests", "true");
    const suffix = query.size > 0 ? `?${query}` : "";
    return fetch(`/api/map${suffix}`, {
      headers: { Authorization: `Bearer ${token}` },
    })
      .then((response) => {
        if (!response.ok) throw new Error(`Project request failed (${response.status})`);
        return response.json() as Promise<MapResponse>;
      });
  };
  const requestCalls = (publicOnly: boolean, unresolved: boolean, showTests: boolean) => {
    const query = new URLSearchParams();
    if (publicOnly) query.set("public", "true");
    if (unresolved) query.set("unresolved", "true");
    if (showTests) query.set("tests", "true");
    return fetch(`/api/calls?${query}`, {
      headers: { Authorization: `Bearer ${token}` },
    }).then((response) => {
      if (!response.ok) throw new Error(`Call hierarchy request failed (${response.status})`);
      return response.json() as Promise<CallsResponse>;
    });
  };
  requestMap()
    .then((response) => {
      app.textContent = "";
      const controls = document.createElement("div");
      controls.className = "view-controls";
      controls.dataset.cameraControl = "";
      const modeButtons = new Map<PrimaryMode, HTMLButtonElement>();
      for (const [mode, label] of [
        ["structure", "Structure"],
        ["calls", "Calls"],
      ] as const) {
        const button = document.createElement("button");
        button.className = "view-button";
        button.type = "button";
        button.textContent = label;
        modeButtons.set(mode, button);
      }
      const sourceToggle = document.createElement("button");
      sourceToggle.className = "view-button";
      sourceToggle.type = "button";
      sourceToggle.textContent = "Source";
      sourceToggle.setAttribute("aria-pressed", "false");
      const publicToggle = document.createElement("button");
      publicToggle.className = "view-button";
      publicToggle.type = "button";
      publicToggle.textContent = "Public only";
      publicToggle.setAttribute("aria-pressed", "false");
      const unresolvedToggle = document.createElement("button");
      unresolvedToggle.className = "view-button";
      unresolvedToggle.type = "button";
      unresolvedToggle.textContent = "Unresolved calls";
      unresolvedToggle.setAttribute("aria-pressed", "false");
      const testsToggle = document.createElement("button");
      testsToggle.className = "view-button";
      testsToggle.type = "button";
      testsToggle.textContent = "Tests";
      testsToggle.setAttribute("aria-pressed", "false");
      const reset = document.createElement("button");
      reset.className = "view-button";
      reset.type = "button";
      reset.textContent = "Reset view";
      const zoom = document.createElement("output");
      zoom.className = "zoom-value";
      zoom.setAttribute("aria-live", "polite");
      controls.append(
        zoom,
        ...modeButtons.values(),
        sourceToggle,
        publicToggle,
        testsToggle,
        unresolvedToggle,
        reset,
      );
      app.append(controls);

      let mode: PrimaryMode = "structure";
      let inlineSource = false;
      let publicOnly = false;
      let showUnresolved = false;
      let showTests = false;
      let currentResponse = response;
      let currentCalls: CallsResponse | undefined;
      let map = renderPackedMap(response.structure, response.wrap_column);
      let camera: Camera;
      let highlighter: PathHighlighter;
      let preview: SourcePreview | undefined;
      const loadMap = createCachedLoader(
        (key) => {
          const [visibility, tests] = key.split(":");
          return requestMap(visibility === "public", tests === "tests");
        },
        ["all:hidden", response],
      );
      const loadCalls = createCachedLoader((key) => {
        const [visibility, unresolved, tests] = key.split(":");
        return requestCalls(
          visibility === "public",
          unresolved === "unresolved",
          tests === "tests",
        );
      });
      const renderedMaps = new Map<string, HTMLElement>([
        ["all:structure:false:false:false", map],
      ]);
      const mount = () => {
        camera?.destroy();
        highlighter?.destroy();
        preview?.destroy();
        map.remove();
        const layout = inlineSource ? "source" : "structure";
        const key = `${publicOnly ? "public" : "all"}:${mode}:${inlineSource}:${showUnresolved}:${showTests}`;
        const packed = mode === "calls" ? currentCalls![layout] : currentResponse[layout];
        const wrapColumn = mode === "calls" ? currentCalls!.wrap_column : currentResponse.wrap_column;
        const sourceLod = mode === "structure" && inlineSource;
        map =
          renderedMaps.get(key) ??
          renderPackedMap(packed, wrapColumn, mode === "calls", sourceLod);
        renderedMaps.set(key, map);
        app.insertBefore(map, controls);
        highlighter = createPathHighlighter(map);
        const sourceForId =
          mode === "calls"
            ? createCallPreviewResolver(currentCalls!.source.root)
            : createSourceResolver(currentResponse.structure.root, currentResponse.source.root);
        preview =
          mode === "calls" || !inlineSource
            ? createSourcePreview(app, map, sourceForId)
            : undefined;
        camera = createCamera(app, map, {
          fitScale: calculateFitScale(app, map),
          onScaleChange: (scale) => {
            zoom.textContent = `Zoom ${scale.toFixed(2)}×`;
            if (sourceLod) applySourceLodScale(map, scale);
          },
        });
        for (const [buttonMode, button] of modeButtons) {
          button.setAttribute("aria-pressed", (buttonMode === mode).toString());
          button.textContent = buttonMode === "structure" ? "Structure" : "Calls";
        }
        sourceToggle.textContent = "Source";
        sourceToggle.setAttribute("aria-pressed", inlineSource.toString());
        publicToggle.textContent = "Public only";
        publicToggle.setAttribute("aria-pressed", publicOnly.toString());
        testsToggle.textContent = "Tests";
        testsToggle.setAttribute("aria-pressed", showTests.toString());
        unresolvedToggle.hidden = mode !== "calls";
        unresolvedToggle.setAttribute("aria-pressed", showUnresolved.toString());
      };
      const changeView = async (
        nextMode: PrimaryMode,
        nextInlineSource: boolean,
        nextPublic: boolean,
        nextUnresolved: boolean,
        nextTests: boolean,
        pendingButton: HTMLButtonElement,
      ) => {
        const buttons = [
          ...modeButtons.values(),
          sourceToggle,
          publicToggle,
          testsToggle,
          unresolvedToggle,
        ];
        for (const button of buttons) button.disabled = true;
        try {
          if (nextMode === "calls") {
            pendingButton.textContent = "Loading calls…";
            const key = `${nextPublic ? "public" : "all"}:${
              nextUnresolved ? "unresolved" : "resolved"
            }:${nextTests ? "tests" : "hidden"}`;
            currentCalls = await loadCalls(key);
          } else {
            currentResponse = await loadMap(
              `${nextPublic ? "public" : "all"}:${nextTests ? "tests" : "hidden"}`,
            );
          }
          mode = nextMode;
          inlineSource = nextInlineSource;
          publicOnly = nextPublic;
          showUnresolved = nextUnresolved;
          showTests = nextTests;
          mount();
        } catch {
          pendingButton.textContent =
            pendingButton === testsToggle
              ? "Retry tests"
              : nextMode === "calls"
                ? "Retry calls"
                : "Retry public";
        } finally {
          for (const button of buttons) button.disabled = false;
        }
      };
      for (const [nextMode, button] of modeButtons) {
        button.addEventListener("click", () => {
          if (nextMode !== mode) {
            void changeView(
              nextMode,
              inlineSource,
              publicOnly,
              showUnresolved,
              showTests,
              button,
            );
          }
        });
      }
      sourceToggle.addEventListener("click", () => {
        void changeView(
          mode,
          !inlineSource,
          publicOnly,
          showUnresolved,
          showTests,
          sourceToggle,
        );
      });
      publicToggle.addEventListener("click", () => {
        void changeView(
          mode,
          inlineSource,
          !publicOnly,
          showUnresolved,
          showTests,
          publicToggle,
        );
      });
      testsToggle.addEventListener("click", () => {
        void changeView(
          mode,
          inlineSource,
          publicOnly,
          showUnresolved,
          !showTests,
          testsToggle,
        );
      });
      unresolvedToggle.addEventListener("click", () => {
        void changeView(
          "calls",
          inlineSource,
          publicOnly,
          !showUnresolved,
          showTests,
          unresolvedToggle,
        );
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

function renderPackedMap(
  packed: PackedMap,
  wrapColumn: number,
  callHierarchy = false,
  sourceLod = false,
): HTMLElement {
  const map = document.createElement("main");
  map.className = "map";
  map.style.width = `${packed.world_width}px`;
  map.style.height = `${packed.world_height}px`;
  map.append(renderNode(packed.root, wrapColumn, callHierarchy));
  if (sourceLod) {
    map.classList.add("source-lod", "lod-medium-hidden", "lod-coarse-hidden");
    map.append(...renderSourceLods(packed.root, wrapColumn));
  }
  return map;
}

function renderNode(node: MapNode, wrapColumn: number, callHierarchy: boolean): HTMLElement {
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
  if (callHierarchy && node.preview_key) element.dataset.previewSourceId = node.preview_key;
  if (!callHierarchy && preview) element.dataset.previewSourceId = preview.id.toString();
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
    if (node.subtitle) {
      const subtitle = document.createElement("span");
      subtitle.className = "node-subtitle";
      subtitle.textContent = node.subtitle;
      header.append(subtitle);
    }
    element.append(header);
  }

  if (node.flow) element.append(renderSourceFlow(node.flow, wrapColumn));
  if (node.source !== undefined) {
    const source = document.createElement("pre");
    source.className = "node-source";
    source.textContent = node.source;
    element.append(source);
  }
  for (const child of node.children) {
    element.append(renderNode(child, wrapColumn, callHierarchy));
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
