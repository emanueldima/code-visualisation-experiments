export type Rect = { x: number; y: number; width: number; height: number };

export type Breadcrumb = {
  id: number;
  name: string;
  path: string;
  kind: "directory" | "file" | "container" | "source";
};

export type MapNode = {
  ancestor_ids: number[];
  breadcrumbs: Breadcrumb[];
  kind: Breadcrumb["kind"];
  subtitle?: string;
  rect: Rect;
  header_height: number;
  source?: string;
  start_byte?: number;
  end_byte?: number;
  flow?: SourceFlow;
  preview_key?: string;
  details?: string;
  children: MapNode[];
};

export type SourceFlow = {
  source: string;
  columns: { rows: SourceRow[] }[];
};

export type SourceRow = {
  runs: { start_byte: number; end_byte: number; highlighted: boolean }[];
  highlighted: boolean;
};

export type PackedMap = {
  root: MapNode;
  aspect_ratio: number;
  world_width: number;
  world_height: number;
};
