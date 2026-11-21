import { html } from "../html.js";

// Segmented buttons. `options`: [[value, label]].
export function Seg({ options, value, onChange, label }) {
  return html`<div class="seg" role="group" aria-label=${label}>
    ${options.map(
      ([v, text]) =>
        html`<button type="button" aria-pressed=${String(v === value)} onClick=${() => onChange(v)}>${text}</button>`,
    )}
  </div>`;
}

export function Empty({ title, children }) {
  return html`<div class="empty">${title && html`<b>${title}</b>`}${children}</div>`;
}

export function Panel({ title, hint, actions, class: cls = "", children }) {
  return html`<section class=${`panel ${cls}`}>
    <div class="panel-head">
      <h2>${title}</h2>
      ${hint != null && hint !== "" && html`<span class="hint num">${hint}</span>`}
      ${actions}
    </div>
    ${children}
  </section>`;
}

export const Logo = () => html`<svg width="30" height="30" viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true">
  <path d="M4 7 Q16 3 28 7 L25 26 Q16 29 7 26 Z" fill="none" />
  <path d="M6 13 Q16 10 26.5 13 M7 19.5 Q16 17 25.5 19.5 M10 5.5 L12 27.5 M16 4.5 L16 28.5 M22 5.5 L20 27.5" />
</svg>`;

const icon = (paths) => () =>
  html`<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">${paths}</svg>`;

export const ClockIcon = icon(html`<circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />`);
export const AlertIcon = icon(html`<path d="M12 3 2 21h20L12 3Z" /><path d="M12 10v5M12 18v.5" />`);
export const ReconnectIcon = icon(
  html`<path d="M3 12a9 9 0 0 1 15.5-6.2M21 12a9 9 0 0 1-15.5 6.2" /><path d="M18 3v3h-3M6 21v-3h3" />`,
);

export function Wing({ label = true }) {
  return html`<svg class="pat" viewBox="0 0 12 10" role=${label ? "img" : null} aria-label=${label ? "Lichess supporter" : null} aria-hidden=${label ? null : "true"}>
    ${label && html`<title>Lichess supporter</title>`}
    <path fill="currentColor" d="M.5 1.5c3.2-.4 5.4 1 6.3 3.6C7.6 3 9.1 1.8 11.5 1.5 11 5 9.3 8 6.4 9 3.4 8.3 1.2 5.6.5 1.5Z" />
  </svg>`;
}
