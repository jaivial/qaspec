# qaspec website

Svelte 5 + Vite. Deployed to https://jaivial.github.io/qaspec/ from the `gh-pages` branch.

```bash
npm ci
npm run dev       # http://localhost:5173/qaspec/
npm run build     # -> dist/
```

## Data

`src/data/run.txt`, `src/data/check.txt` and `src/data/report.json` are the real output of
`qaspec run` / `qaspec check` against the fixture app in `tests/fixtures/app`. Example specs and
`qaspec.toml` are imported from `examples/` and `tests/fixtures/` at build time, so the site always
shows the files in the repo.

## Screenshots

`?shot=run|report|spec|check` renders a single view at 1000px. To regenerate the README
screenshots:

```bash
npm run build && npx vite preview --port 4173 &
for n in run report spec check; do
  agent-browser open "http://localhost:4173/qaspec/?shot=$n"
  agent-browser wait --load networkidle
  agent-browser screenshot "#shot" "public/screenshots/$n.png"
done
cp public/screenshots/*.png ../assets/screenshots/
```

`agent-view.png` is `agent-browser screenshot --annotate` of the fixture app's `/todos` page.
