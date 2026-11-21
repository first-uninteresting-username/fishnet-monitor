# shadcn/ui components

Ports of [shadcn/ui](https://ui.shadcn.com) components (new-york style,
Tailwind v4) to Preact + htm. The class strings follow upstream; markup that
upstream gets from Radix is written by hand, since these components need no
popovers or focus traps. Each component takes `class` and merges it with
`cn()`, so callers can override any default.

To add one: copy the upstream `.tsx` from the shadcn repo
(`apps/v4/registry/new-york-v4/ui/`), turn JSX into an `html` template, `className`
into `class`, and keep the `data-slot` attributes.
