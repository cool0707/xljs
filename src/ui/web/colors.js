"use strict";
// The accent, background and radius options of the built-in form pages
// (form.js, forms.js).
// The accent option: the accent color (buttons, tabs, focus, checkboxes,
// selected rows, the title bar icon) per theme, with its hover color, the
// text on it (black or white, whichever reads better) and the selection
// color derived from it. The background option: the page's (and the custom
// title bar's) background per theme; a dark background in the light theme
// (or a light one in the dark theme) takes the other theme's text, input and
// border colors so the page stays readable. An unknown color keeps the default.
function applyColors(accent, background, radius) {
  // Black or white text on a color: by the OKLab lightness (L <= 0.65 takes
  // white), not the WCAG luminance, whose small weights for red and green put
  // black text on red, teal or mid blue. White there still keeps 3:1 (WCAG's
  // large text / UI components). The color is drawn on a canvas so any CSS
  // color syntax (names, hsl(), oklch(), color-mix()) gives its sRGB value.
  const lightness = (c) => {
    const ctx = Object.assign(document.createElement("canvas"), { width: 1, height: 1 }).getContext("2d");
    ctx.fillStyle = c;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = [...ctx.getImageData(0, 0, 1, 1).data].map((v) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
    const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    return 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  };
  const isDark = (c) => lightness(c) <= 0.65;
  const valid = (c) => !!c && CSS.supports("color", c);
  const accentVars = (c, dark) => {
    if (!valid(c)) return "";
    const mix = dark ? "black" : "white";
    return `--accent: ${c}; --accent-hover: color-mix(in srgb, ${c} 88%, ${mix});` +
      ` --accent-fg: ${isDark(c) ? "#ffffff" : "#000000"}; --xljs-titlebar-accent: ${c};` +
      ` --sel: color-mix(in srgb, ${c} ${dark ? "45%" : "22%"}, ${dark ? "#18181b" : "white"});`;
  };
  // The text and control colors of base.css for a light and a dark background.
  const palettes = {
    light: { fg: "#18181b", muted: "#71717a", border: "#e4e4e7", inputBorder: "#d4d4d8", input: "#ffffff" },
    dark: { fg: "#fafafa", muted: "#a1a1aa", border: "#2e2e33", inputBorder: "#3f3f46", input: "#1c1c20" },
  };
  const backgroundVars = (c, dark) => {
    if (!valid(c)) return "";
    const darkBg = isDark(c);
    const p = palettes[darkBg ? "dark" : "light"];
    // The footer, table heads, striped rows and the tab bar: tints of the background.
    const tint = (k) => `color-mix(in srgb, ${c} ${k}, ${p.fg})`;
    let vars = `--bg: ${c}; --xljs-titlebar-bg: ${c}; --xljs-titlebar-fg: ${p.fg};` +
      ` --hover: ${tint(darkBg ? "88%" : "94%")}; --subtle: ${tint(darkBg ? "88%" : "94%")};` +
      ` --panel: ${tint(darkBg ? "95%" : "98%")}; --alt: ${tint(darkBg ? "97%" : "98.5%")};`;
    if (darkBg !== dark) {
      vars += ` --fg: ${p.fg}; --muted: ${p.muted}; --border: ${p.border}; --input-border: ${p.inputBorder}; --input: ${p.input};` +
        ` --sel-fg: ${p.fg}; color-scheme: ${darkBg ? "dark" : "light"};`;
    }
    return vars;
  };
  // The radius option: the corners of buttons, inputs and boxes (a CSS length; "" keeps the default).
  if (radius && CSS.supports("border-radius", radius)) document.documentElement.style.setProperty("--radius", radius);
  else document.documentElement.style.removeProperty("--radius");
  const light = backgroundVars(background.light, false) + " " + accentVars(accent.light, false);
  const dark = backgroundVars(background.dark, true) + " " + accentVars(accent.dark, true);
  let css = "";
  if (light.trim()) css += `:root { ${light.trim()} }\n`;
  if (dark.trim()) css += `@media (prefers-color-scheme: dark) { :root { ${dark.trim()} } }\n`;
  let style = document.getElementById("accent-style");
  if (!style) style = document.head.appendChild(Object.assign(document.createElement("style"), { id: "accent-style" }));
  style.textContent = css;
}

