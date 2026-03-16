import { useEffect, useRef } from "react";
import type { RefObject } from "react";

/**
 * Extracts the dominant saturated colour from each album cover image inside a
 * container and sets it as `--glow-rgb` on the parent `.sleeve` element.
 *
 * Attach the returned ref to the grid/container wrapping your `.sleeve` cards.
 * Pass a dependency that changes when the list changes (e.g. the data array
 * length) so the effect re-runs for new cards.
 *
 * Ported from the Leptos implementation in `crates/yoink-app/src/shell.rs`.
 */
export function useSleeveGlow(deps: ReadonlyArray<unknown> = []): RefObject<HTMLDivElement | null> {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const images = container.querySelectorAll<HTMLImageElement>(".sleeve-cover");
    for (const img of images) {
      processImg(img);
    }
  }, deps);

  return containerRef;
}

// ── Shared processing helpers ──────────────────────────────────

function processImg(img: HTMLImageElement) {
  if (img.complete && img.naturalWidth > 0) {
    applyGlow(img);
  } else {
    img.addEventListener(
      "load",
      () => {
        applyGlow(img);
      },
      { once: true },
    );
  }
}

function applyGlow(img: HTMLImageElement) {
  const sleeve = img.closest<HTMLElement>(".sleeve");
  if (!sleeve || sleeve.dataset.glowApplied === "1") return;
  try {
    const colour = pickGlowColour(img);
    if (colour) {
      sleeve.style.setProperty(
        "--glow-rgb",
        `${String(colour.r)}, ${String(colour.g)}, ${String(colour.b)}`,
      );
    } else {
      // No saturated colour found (B&W / grey cover) — disable glow
      sleeve.classList.add("sleeve--no-glow");
    }
  } catch {
    // Cross-origin or tainted canvas — disable glow rather than showing
    // the blue fallback colour
    sleeve.classList.add("sleeve--no-glow");
  }
  sleeve.dataset.glowApplied = "1";
}

// ── Colour extraction ──────────────────────────────────────────

interface RGB {
  r: number;
  g: number;
  b: number;
}

function hslToRgb(h: number, s: number, l: number): RGB {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let r: number, g: number, b: number;

  if (h < 60) {
    r = c;
    g = x;
    b = 0;
  } else if (h < 120) {
    r = x;
    g = c;
    b = 0;
  } else if (h < 180) {
    r = 0;
    g = c;
    b = x;
  } else if (h < 240) {
    r = 0;
    g = x;
    b = c;
  } else if (h < 300) {
    r = x;
    g = 0;
    b = c;
  } else {
    r = c;
    g = 0;
    b = x;
  }

  return {
    r: Math.round((r + m) * 255),
    g: Math.round((g + m) * 255),
    b: Math.round((b + m) * 255),
  };
}

function pickGlowColour(img: HTMLImageElement): RGB | null {
  const canvas = document.createElement("canvas");
  canvas.width = 24;
  canvas.height = 24;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;

  ctx.drawImage(img, 0, 0, 24, 24);
  const data = ctx.getImageData(0, 0, 24, 24).data;

  let rSum = 0,
    gSum = 0,
    bSum = 0,
    n = 0;

  for (let i = 0; i < data.length; i += 4) {
    const pr = data[i];
    const pg = data[i + 1];
    const pb = data[i + 2];
    const pa = data[i + 3];

    if (pa < 180) continue;

    const max = Math.max(pr, pg, pb);
    const min = Math.min(pr, pg, pb);
    const sat = max === 0 ? 0 : (max - min) / max;
    const lum = (pr + pg + pb) / 3;

    if (lum < 25 || lum > 235 || sat < 0.18) continue;

    rSum += pr;
    gSum += pg;
    bSum += pb;
    n++;
  }

  if (n === 0) return null;

  // Normalise lightness: convert average to HSL, fix L=55%, clamp saturation
  const ar = rSum / n / 255;
  const ag = gSum / n / 255;
  const ab = bSum / n / 255;

  const cmax = Math.max(ar, ag, ab);
  const cmin = Math.min(ar, ag, ab);
  const d = cmax - cmin;

  let h = 0;
  if (d > 0) {
    if (cmax === ar) h = 60 * (((ag - ab) / d) % 6);
    else if (cmax === ag) h = 60 * ((ab - ar) / d + 2);
    else h = 60 * ((ar - ag) / d + 4);
  }
  if (h < 0) h += 360;

  const sl = (cmax + cmin) / 2;
  const ss = d === 0 ? 0 : d / (1 - Math.abs(2 * sl - 1));

  const targetL = 0.55;
  let targetS = Math.min(ss, 0.85);
  if (targetS < 0.3) targetS = 0.3;

  return hslToRgb(h, targetS, targetL);
}
