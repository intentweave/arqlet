/** The scroll offset that keeps the world point under `anchor` fixed when the scale changes. */
export function zoomAnchor(scroll: number, anchor: number, oldScale: number, newScale: number): number;
/** Wheel zoom at the pointer and drag panning for a scrolling viewport element. */
export function bindNavigation(viewport: HTMLElement, getScale: () => number, setScale: (scale: number) => void): void;
