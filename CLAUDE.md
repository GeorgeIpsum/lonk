# lonk — agent instructions

## Superpowers document placement

All superpowers workflow documents go in the top-level `plans/` directory,
NOT under `docs/`:

- Design specs (brainstorming skill): `plans/YYYY-MM-DD-<topic>-design.md`
- Implementation plans (writing-plans skill): `plans/YYYY-MM-DD-<feature>-plan.md`

This overrides those skills' default `docs/superpowers/...` locations.
`docs/` is reserved for published documentation (`docs/guide/` is served on
GitHub Pages); internal planning material must never live there.
