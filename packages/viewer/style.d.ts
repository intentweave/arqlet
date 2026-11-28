/** Provisional: the named colours of the appearance, for every part a renderer draws and for the states. */
export interface StyleTheme {
  frameFill: string; frameStroke: string; title: string;
  leafFill: string; leafStroke: string; text: string; row: string;
  selectedFill: string; selectedStroke: string; invalid: string;
  route: string; activeRoute: string; arrow: string;
  bus: string; busText: string; hubFill: string; hubStroke: string;
  portSource: string; portTarget: string; crossing: string; labelFill: string; labelText: string;
  junctionFill: string; junctionStroke: string;
  pin: string; control: string; controlIcon: string; hover: string; trace: string;
}
/** What a rule sets on a node: colours may be CSS colours, a token (`$route`) or `auto` (categorical by kind). */
/** `radius`: the corner radius, a number or `'pill'` (half the shorter side). */
export interface NodeStyle { fill?: string; stroke?: string; strokeWidth?: number; text?: string; dash?: string; radius?: number | 'pill'; /** `'dot'`: a station dot of `glyphSize` (default 7) instead of the box, the name outside. Provisional `'tick'`: a stub of `glyphSize` (default 9) and `strokeWidth` (default 4) in `stroke`, across the station's line (`orientGlyphs`), the name beyond its tip. Provisional `'pill'`: a pill `glyphSize` thick (default 11) and 2.4 times as long, `fill` with a `strokeWidth` border (default 3), across the station's lines, the name beyond its end. */ glyph?: 'dot' | 'tick' | 'pill'; glyphSize?: number }
/** What a rule sets on an edge (route). `width` and `opacity` replace the weight-based defaults; `arrow: 'none'` draws no arrowhead. */
export interface EdgeStyle { stroke?: string; arrow?: string; width?: number; opacity?: number; dash?: string }
/**
 * A rule: a selector over an element's data and its properties. Selectors: `node`, `container`, `leaf`, `edge` or `*`,
 * then any of `[key=value]` (for example `[kind=module]`, `[shape=circle]`), `[key]` (the field is set) and `#id`.
 * Later rules win. With `bind(scene)` the fields `bus` (edges and nodes on one bus), `buses` and `interchange` (nodes on
 * several buses) come from the scene's explicit buses. Colours: CSS, `$token`, `auto` (by kind) or `auto(field)`.
 */
export interface StyleRule { selector: string; style: NodeStyle & EdgeStyle }
/** A style sheet: tokens over the default theme, rules, and the colours of `auto`. All parts optional. */
export interface StyleSheet { theme?: Partial<StyleTheme>; rules?: StyleRule[]; palette?: string[] }
/** A resolved style (from `createStyle`). */
export interface ResolvedStyle {
  readonly resolved: true;
  readonly theme: StyleTheme;
  /** No rules and the default theme. */
  readonly isDefault: boolean;
  node(node: {id: string; kind?: string; shape?: string; container?: boolean; [key: string]: unknown}): NodeStyle;
  edge(route: {id: string; kind?: string; [key: string]: unknown}): EdgeStyle;
  /** `node` and `edge` for the elements of `scene`, with the fields it derives (bus, buses, interchange). */
  bind(scene: {buses?: string[][]; routes?: {id: string; source: string; target: string}[]}): {node: ResolvedStyle['node']; edge: ResolvedStyle['edge']};
}

export const DEFAULT_THEME: Readonly<StyleTheme>;
export const PALETTE: readonly string[];
/** The categorical colour of a kind (stable for a kind across views). */
export function autoColour(kind: string | null | undefined, palette?: readonly string[]): string;
/** Provisional: resolves a sheet (a resolved style is returned as it is). */
export function createStyle(sheet?: StyleSheet | ResolvedStyle | null): ResolvedStyle;
/** The theme tokens set as CSS custom properties (`--arqlet-leaf-fill`, …) on or above `element`. */
export function themeFromCss(element: Element, view?: Window): Partial<StyleTheme>;
/** Built-in sheets: `default`, `byKind` (routes and leaf outlines by kind), `metro` (buses as metro lines, stations,
 * interchanges; light), `light` (needs a light background). */
export const STYLE_PRESETS: Readonly<Record<'default' | 'byKind' | 'metro' | 'light', StyleSheet>>;

/** Provisional: a glyph's geometry. A dot: middle, radius and where its name goes. A tick: from the middle along the unit
 * normal (`nx`, `ny`) to (`x2`, `y2`), `width` wide. A pill: `length` along the normal and `thickness` across, centred,
 * a border `width` wide, its end at (`x2`, `y2`). A tick's or pill's name at (`labelX`, `labelY`) with canvas `align`
 * and `baseline`. */
export interface Glyph { kind: 'dot' | 'tick' | 'pill'; cx: number; cy: number; r: number; labelX: number; labelY: number;
  length?: number; thickness?: number; width?: number; nx?: number; ny?: number; x2?: number; y2?: number; align?: 'left' | 'right' | 'center'; baseline?: 'middle' | 'top' | 'bottom' }
/** Provisional: a node's glyph (`glyph: 'dot'`, `'tick'` or `'pill'`); null for a box. Ticks and pills point up until
 * `orientGlyphs`. */
export function glyphOf(look: NodeStyle | null | undefined, bounds: {x: number; y: number; width: number; height: number}): Glyph | null;
/** Provisional: turns every tick and pill in `glyphs` (node id → glyph) across the lines of its routes (those of an
 * explicit bus first; with more than two, their main axis), to the right of the line or up when it runs across; in
 * place, returns `glyphs`. */
export function orientGlyphs(glyphs: Map<string, Glyph>, scene: {routes?: {id: string; source: string; target: string; points: {x: number; y: number}[]}[]; buses?: string[][]}): Map<string, Glyph>;
/** The points a route is drawn with when an end is a glyph (to the dot's middle). */
export function routeToGlyphs(points: {x: number; y: number}[], sourceGlyph: {cx: number; cy: number} | null | undefined, targetGlyph: {cx: number; cy: number} | null | undefined): {x: number; y: number}[];
