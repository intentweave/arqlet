import type {PlacedLabel, ResolvedPort, Scene, SceneNode} from '@arqlet/contracts';

export interface RenderSvgOptions {
  /** Compare with the previous drawing and keep unchanged elements (needs a real DOM). Default `false`. */
  keyed?: boolean;
  /** Document used to create elements; default `host.ownerDocument`. */
  document?: Document;
  /** Scale of the drawn size against scene units. Default 1. */
  scale?: number;
  selected?: string | null;
  selectedRoute?: string | null;
  /** Ids of pinned nodes. */
  pinned?: Set<string>;
  /** Per node id, the axes whose size the user set. */
  sized?: Map<string, string[]>;
  onSelect?: (nodeId: string, event: Event) => void;
  onSelectRoute?: (routeId: string) => void;
  onToggle?: (nodeId: string) => void;
  onPin?: (nodeId: string, pin: boolean) => void;
  onReset?: (nodeId: string, axis: string) => void;
  onNodeDragStart?: (node: SceneNode, event: PointerEvent, detail?: {fromPin: boolean}) => void;
  onResize?: (node: SceneNode, event: PointerEvent) => void;
  onPortDragStart?: (port: ResolvedPort, event: PointerEvent) => void;
  onLabelDragStart?: (label: PlacedLabel, event: PointerEvent) => void;
  /**
   * Provisional (path cut-out): marks a trace (from the engine's `trace` operation, `visible`): its nodes and routes in
   * amber, the other routes fainter; with `fade`, each node the fainter the farther it is from the path (`distance`).
   */
  trace?: {
    nodes?: Iterable<string>;
    routes?: Iterable<string>;
    distance?: Record<string, number> | Map<string, number>;
    fade?: boolean;
  } | null;
}

/**
 * Paints a scene into an `<svg>` element. Pure rendering: it never computes layout or changes the scene; interaction is
 * reported through the callbacks.
 */
export function renderSvg(host: SVGSVGElement, scene: Scene, options?: RenderSvgOptions): void;
