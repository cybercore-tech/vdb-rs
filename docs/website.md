# vdb-rs project page

GitHub Pages serves `main` → `/docs` at https://cybercore-tech.github.io/vdb-rs/.
The root `index.html` mirrors `docs/index.html` with adjusted asset paths for
root-directory previews. Update both copies when changing page content.

Preview from this directory: `python -m http.server 8793 --bind 127.0.0.1`.

`index.html` contains project copy grounded in README.md and the architecture, recovery, and performance/semantic-search documents. Current sections cover atomic batches,
incremental HNSW, synthetic mixed-workload measurements, the real local-note pilot,
readable search/report commands, and the remaining application integration work. `theme.js` uses the shared Cybercore theme registry. `lab.js` implements a deterministic 2D exact metric demonstration, tag filtering, recovery tabs, and command search. It does not execute the Rust library, HNSW, or a database. `base.css` preserves the family presentation kit; `site.css` applies the project's violet/cyan visual direction. Font licenses are bundled alongside the local typefaces.

Validation: local asset references; JS syntax; three responsive widths; metric direction; cosine zero query; metadata tag results; recovery tabs; command search. Synthetic benchmark figures and the real-note pilot are labeled separately, with
workload sizes, ranking caveats and links to their measurement methods. Original
notes and private JSON/Markdown reports are never published with the site.
