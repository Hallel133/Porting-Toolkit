# Hosting the Marketplace on another service

The app reads one or more **marketplace indexes** (`index.json`) and merges them; for the
same plugin id the newest version wins. The default index is on GitHub. When it cannot be
reached, publish a mirror anywhere that serves static files over https (Codeberg, GitLab
Pages, any web host) and add it as a source.

## 1. Build the files

```bash
python3 tools/build_marketplace.py https://codeberg.org/YOU/ptk-marketplace/raw/branch/main dist-marketplace
```

This zips every plugin in `marketplace/plugins/` (deterministically: an unchanged plugin
keeps its SHA-256) and writes `dist-marketplace/index.json`, whose `download` URLs start with
the base URL you give. Entries are marked `"reviewed": false`; a maintainer sets it after a
review.

## 2. Upload them (Codeberg example)

1. Create an account at codeberg.org and an empty public repository named `ptk-marketplace`.
2. Push the contents of `dist-marketplace/` to its `main` branch:

   ```bash
   cd dist-marketplace && git init -b main && git add . && git commit -m "Marketplace" \
     && git remote add origin https://codeberg.org/YOU/ptk-marketplace.git && git push -u origin main
   ```

3. Check that `https://codeberg.org/YOU/ptk-marketplace/raw/branch/main/index.json` opens in a browser.

Any other host works the same way: the zips must be at `BASE_URL/<id>-<version>.zip`, next to
`index.json`.

## 3. Add the source

- App: **Settings → Marketplace → Add source**, with the index URL.
- CLI: `ptk market sources --add https://codeberg.org/YOU/ptk-marketplace/raw/branch/main/index.json`

Adding the first source keeps the default index in the list, so both are read. `ptk market
sources --remove URL` and `--reset` undo it; `PTK_MARKETPLACE_URL` (one URL) overrides
everything, for testing.

## Trust

Every download is checked against the SHA-256 in the index before it is installed, and a
plugin's permissions are shown before installing. An index can only point at files; whoever
controls an index decides what it lists, so add only sources you trust.
