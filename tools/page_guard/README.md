# Page guards (viewer-shell/ui)

Three checks for the viewer page after a merge of Gardenfell's page (R3 of the 18hu rebase,
`viewer-shell/R3-LEDGER.md`). The page is one script assembled from `ui/src` in `ui/ORDER`,
so a part merged from upstream can call a function, an element or a string that only a part
we left out (the grass tools) defined. The page tests only find what a test happens to click.

- `names.js` - every global name the page uses that nothing in it defines (eslint's
  `no-undef` over the parts, concatenated in `ORDER`). Needs eslint 9 and globals 15:
  `npm i --no-save eslint@9 globals@15` in a scratch folder, then
  `node <scratch>/names.js viewer-shell/ui` with this file copied there (or `NODE_PATH`).
- `ids.py` - element ids the scripts look up (`$('#x')`, `getElementById('x')`, any
  `'#x'` string) that no markup or script creates. `python tools/page_guard/ids.py viewer-shell/ui`.
- `keys.py` - `T('key')` strings and `data-tx`/`data-txt` keys missing from `05_text.js`.
  Markup keys fall back to the markup's own text; script keys (`keys.py ui script`) show
  their raw key, so those are the ones that matter.

Each prints what it finds; compare with a run on the page before the merge. After the R3
merge (2026-10-12): names 0, ids 18, script keys 8 - all of them there before the merge too,
ids built in template strings and keys with inline fallbacks, none of them errors.
