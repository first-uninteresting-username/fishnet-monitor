# shadcn/ui components

Ports of [shadcn/ui](https://ui.shadcn.com) components (new-york style,
Tailwind v4) to Preact + htm. The class strings follow upstream; markup that
upstream gets from Radix is written by hand, since these components need no
popovers or focus traps. Each component takes `class` and merges it with
`cn()`, so callers can override any default.

The styling is Material 3, not shadcn's new-york look: each file says which M3
component it follows (card, segmented button, common button, banner, linear
progress, data table, chip). The tokens are in `frontend/tailwind.css`; buttons
and toggles share the M3 state layer `stateLayer` from `lib/utils.js`, and icons
come from `../icons.js` (Material Symbols).

To add one: copy the upstream `.tsx` from the shadcn repo
(`apps/v4/registry/new-york-v4/ui/`), turn JSX into an `html` template, `className`
into `class`, and keep the `data-slot` attributes.
