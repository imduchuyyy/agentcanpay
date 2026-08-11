# Vendored assets

Third-party code served to the page that displays recovery phrases. It is
committed rather than fetched at build time so that what ships is exactly
what was reviewed, and so the build works offline.

| File | Version | Source | SHA-256 |
|---|---|---|---|
| `htmx.min.js` | 2.0.10 | `https://unpkg.com/htmx.org@2.0.10/dist/htmx.min.js` | `71ea67185bfa8c98c39d31717c6fce5d852370fcdfd129db4543774d3145c0de` |

To update, re-download and record the new version and digest here in the
same commit, so the diff shows both the bytes and their provenance:

```sh
VER=2.0.11
curl -sL "https://unpkg.com/htmx.org@${VER}/dist/htmx.min.js" \
  -o crates/connect/assets/htmx.min.js
shasum -a 256 crates/connect/assets/htmx.min.js
```
