export type Camera = {
  reset(): void;
  focus(element: HTMLElement): void;
  setFitScale(scale: number): void;
  destroy(): void;
};

type Point = { x: number; y: number };
type CameraState = Point & { scale: number };
type PinchState = CameraState & { distance: number; midpoint: Point; world: Point };
type CameraOptions = {
  fitScale?: number;
  minScale?: number;
  maxScale?: number;
  onScaleChange?(scale: number, fitScale: number): void;
};

export function createCamera(
  viewport: HTMLElement,
  surface: HTMLElement,
  {
    fitScale: initialFitScale = 1,
    minScale = 0.02,
    maxScale = 0.5,
    onScaleChange,
  }: CameraOptions = {},
): Camera {
  minScale = Math.min(maxScale, Math.max(Number.EPSILON, minScale));
  let fitScale = Math.min(maxScale, Math.max(minScale, initialFitScale));
  let state: CameraState = { x: 0, y: 0, scale: fitScale };
  let dragStart: (Point & { xOffset: number; yOffset: number }) | undefined;
  let pinchStart: PinchState | undefined;
  let notifiedScale: number | undefined;
  const pointers = new Map<number, Point>();

  surface.style.transformOrigin = "0 0";
  viewport.tabIndex = 0;

  const apply = () => {
    surface.style.transform = `translate3d(${state.x}px, ${state.y}px, 0) scale(${state.scale})`;
    if (state.scale === notifiedScale) return;

    notifiedScale = state.scale;
    onScaleChange?.(state.scale, fitScale);
  };
  const zoomAt = (point: Point, scale: number) => {
    const nextScale = Math.min(maxScale, Math.max(minScale, scale));
    const factor = nextScale / state.scale;
    state = {
      x: point.x - (point.x - state.x) * factor,
      y: point.y - (point.y - state.y) * factor,
      scale: nextScale,
    };
    apply();
  };
  const beginGesture = () => {
    const points = [...pointers.values()];
    if (points.length < 2) {
      const point = points[0];
      dragStart = point
        ? { ...point, xOffset: state.x, yOffset: state.y }
        : undefined;
      pinchStart = undefined;
      return;
    }

    const midpoint = {
      x: (points[0].x + points[1].x) / 2,
      y: (points[0].y + points[1].y) / 2,
    };
    pinchStart = {
      ...state,
      distance: distance(points[0], points[1]),
      midpoint,
      world: {
        x: (midpoint.x - state.x) / state.scale,
        y: (midpoint.y - state.y) / state.scale,
      },
    };
    dragStart = undefined;
  };
  const onPointerDown = (event: PointerEvent) => {
    if (
      event.target instanceof Element &&
      event.target.closest("[data-camera-control], button, a, input, select, textarea")
    ) {
      return;
    }
    if (event.pointerType === "mouse" && (event.button !== 0 || event.shiftKey)) return;

    viewport.setPointerCapture(event.pointerId);
    pointers.set(event.pointerId, eventPoint(event, viewport));
    viewport.classList.add("panning");
    beginGesture();
  };
  const onPointerMove = (event: PointerEvent) => {
    if (!pointers.has(event.pointerId)) return;

    pointers.set(event.pointerId, eventPoint(event, viewport));
    const points = [...pointers.values()];
    if (points.length >= 2 && pinchStart) {
      const midpoint = {
        x: (points[0].x + points[1].x) / 2,
        y: (points[0].y + points[1].y) / 2,
      };
      const scale = Math.min(
        maxScale,
        Math.max(
          minScale,
          pinchStart.scale * (distance(points[0], points[1]) / pinchStart.distance),
        ),
      );
      state = {
        x: midpoint.x - pinchStart.world.x * scale,
        y: midpoint.y - pinchStart.world.y * scale,
        scale,
      };
      apply();
      return;
    }
    if (!dragStart || !points[0]) return;

    state.x = dragStart.xOffset + points[0].x - dragStart.x;
    state.y = dragStart.yOffset + points[0].y - dragStart.y;
    apply();
  };
  const onPointerUp = (event: PointerEvent) => {
    pointers.delete(event.pointerId);
    if (pointers.size === 0) viewport.classList.remove("panning");
    beginGesture();
  };
  const onWheel = (event: WheelEvent) => {
    if (event.target instanceof Element && event.target.closest("[data-camera-control]")) return;

    event.preventDefault();
    if (event.ctrlKey || event.metaKey) {
      zoomAt(eventPoint(event, viewport), state.scale * Math.exp(-event.deltaY * 0.003));
      return;
    }

    const unit =
      event.deltaMode === WheelEvent.DOM_DELTA_LINE
        ? 16
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
          ? viewport.clientHeight
          : 1;
    const horizontal = event.shiftKey && event.deltaX === 0 ? event.deltaY : event.deltaX;
    const vertical = event.shiftKey && event.deltaX === 0 ? 0 : event.deltaY;
    state.x -= horizontal * unit;
    state.y -= vertical * unit;
    apply();
  };
  const onDoubleClick = (event: MouseEvent) => {
    const element = (event.target as Element).closest<HTMLElement>(".node");
    if (!element) return;

    event.preventDefault();
    focus(element);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (!["+", "=", "-", "0"].includes(event.key)) return;

    event.preventDefault();
    if (event.key === "0") {
      reset();
      return;
    }
    const center = { x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 };
    zoomAt(center, state.scale * (event.key === "-" ? 0.8 : 1.25));
  };
  const reset = () => {
    state = { x: 0, y: 0, scale: fitScale };
    apply();
  };
  const setFitScale = (scale: number) => {
    fitScale = Math.min(maxScale, Math.max(minScale, scale));
    reset();
  };
  const focus = (element: HTMLElement) => {
    const viewportRect = viewport.getBoundingClientRect();
    const elementRect = element.getBoundingClientRect();
    const factor =
      Math.min(viewport.clientWidth / elementRect.width, viewport.clientHeight / elementRect.height) *
      0.9;
    const scale = Math.min(maxScale, Math.max(minScale, state.scale * factor));
    const worldCenter = {
      x: (elementRect.left + elementRect.width / 2 - viewportRect.left - state.x) / state.scale,
      y: (elementRect.top + elementRect.height / 2 - viewportRect.top - state.y) / state.scale,
    };
    state = {
      x: viewport.clientWidth / 2 - worldCenter.x * scale,
      y: viewport.clientHeight / 2 - worldCenter.y * scale,
      scale,
    };
    apply();
  };
  const destroy = () => {
    viewport.removeEventListener("pointerdown", onPointerDown);
    viewport.removeEventListener("pointermove", onPointerMove);
    viewport.removeEventListener("pointerup", onPointerUp);
    viewport.removeEventListener("pointercancel", onPointerUp);
    viewport.removeEventListener("wheel", onWheel);
    viewport.removeEventListener("dblclick", onDoubleClick);
    viewport.removeEventListener("keydown", onKeyDown);
  };

  viewport.addEventListener("pointerdown", onPointerDown);
  viewport.addEventListener("pointermove", onPointerMove);
  viewport.addEventListener("pointerup", onPointerUp);
  viewport.addEventListener("pointercancel", onPointerUp);
  viewport.addEventListener("wheel", onWheel, { passive: false });
  viewport.addEventListener("dblclick", onDoubleClick);
  viewport.addEventListener("keydown", onKeyDown);
  apply();

  return { reset, focus, setFitScale, destroy };
}

function eventPoint(event: MouseEvent | PointerEvent | WheelEvent, viewport: HTMLElement): Point {
  const rect = viewport.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

function distance(first: Point, second: Point): number {
  return Math.hypot(second.x - first.x, second.y - first.y);
}
