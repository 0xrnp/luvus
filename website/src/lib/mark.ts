import raw from '../assets/luvus-logo-solid.svg?raw';

/**
 * The nav mark, retinted so it takes its colour from CSS.
 *
 * The source file is drawn in flat `white` on `#2A2A2A`. Shipped as authored it
 * is always pure white, which does two wrong things: it disagrees with the
 * wordmark beside it (`--text` is near-white, not white, so the pair looked
 * mismatched), and it vanishes on the light palettes the site can be set to.
 *
 * Mapping the two colours onto `currentColor` and `var(--bg)` keeps the drawing
 * exactly as drawn, while letting whatever sets `color` on the lockup drive the
 * mark and the wordmark together.
 *
 * Exported from here rather than inlined at each call site because it was
 * inlined at each call site, the landing pages missed an edit to the artwork,
 * and the two navs drifted apart.
 */
export const NAV_MARK = raw
  .replace(/#2A2A2A/gi, 'var(--bg)')
  .replace(/(fill|stroke)="white"/g, '$1="currentColor"');
