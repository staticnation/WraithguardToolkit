# Translation catalogues

The program is in English only for now. The catalogues here are the 4.x app's: its
template (`wraithguard_toolkit.pot`), the eleven languages built from it (`<lang>/`), and
their sources (`translations/<lang>.json`, the English string mapped to its translation).
They are kept as they are, as the source the program's translations will start from
(`TODO.md`, "After 5.0"): the page's string tables generated from them, and the page's
own strings extracted into the same template.

Until then, nothing reads them: the 4.x app that loaded the `.mo` files is retired.
`tools/make_pot.py`, which extracted the template, read that app's Python source and has
nothing to extract from now. `tools/build_locale.py` still builds a language's `.po` and
`.mo` from the template and its `translations/<lang>.json`
(`python tools/build_locale.py de`, or `--all`), refusing a translation that drops a
placeholder or gives a plural the wrong number of forms.

## Notes for translators

These still hold for the strings that are here, and will for the program's:

* Placeholders are **named** (`%(count)d`) and may be reordered freely. Never
  use positional `%s` - translators frequently need a different word order.
* Counted messages have plural forms, so your language's own plural rules apply.
* Plugin names, file paths and mlox rule keywords (`[Order]`, `content=`) are
  data, not prose -- leave them untranslated.
