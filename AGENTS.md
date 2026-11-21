# Agent instructions

## Stay in the workspace

- Work only inside this repository (`fishnet-monitor/`). Do not read, search,
  list, or write anything outside it, and do not inspect system state (for
  example `systemctl` or `which`).
- If you need something from outside the repository, ask the user first.

## Don't write READMEs

- Do not create or write README files (`README.md` or any other README),
  including in subdirectories. Only edit an existing README if the user asks
  for that change by name. Reading existing READMEs is fine.

## Commit your work

- When you finish a task, commit your changes to git before reporting back.
  Keep each commit focused on one logical change, with a clear message that
  says what changed and why.
- Do not leave finished work uncommitted in the working tree.
- Do not add co-author or attribution footers (such as `Co-Authored-By`
  lines) to commit messages or PR bodies.
- Do not commit secrets or local config files (for example `.env` or
  `.claude/settings.local.json`).

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
