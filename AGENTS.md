# Agent instructions

## Stay in the workspace

- Work only inside this repository (`fishnet-monitor/`). Do not read, search,
  list, or write anything outside it, and do not inspect system state (for
  example `systemctl` or `which`).
- If you need something from outside the repository, ask the user first.

## Frontend styling

- Style the dashboard with Tailwind CSS utility classes only. Don't write CSS
  files, `<style>` blocks or CSS classes of your own. `frontend/tailwind.css`
  holds only the theme tokens and a few base rules; `frontend/styles.css` is
  generated, so never edit it by hand. Run `build-css` (or `watch-css`) after
  changing classes. The pre-commit hook also rebuilds it; commit the result.
- Build UI elements from shadcn/ui components in `frontend/src/components/ui/`.
  If one you need is missing, port it from upstream shadcn/ui (see the README
  there) rather than writing a one-off.
- Use the theme colours (`bg-card`, `text-muted-foreground`, `text-ok`, …)
  instead of hex values, and check both light and dark mode. Text must meet
  WCAG AA contrast in both.
