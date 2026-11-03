# tools/

Since 5.0 (the Python app retired; `CHANGELOG.md`, 5.0.0):

- `build_and_check_rust.ps1`: every Rust check CI runs, locally; `check_pushed.ps1`.
- `page_guard/`, `meshopt_oracle/`, `gen_g7_nif.py`: checks and generators for the viewer
  and its crates, still runnable.
- `gen_*.py` that import `wraithguard` (`gen_cli_cases.py`, `gen_editcore_tables.py`,
  `gen_land_merge_cases.py`, `gen_mwscript_compiler.py`, `gen_plan_cases.py`,
  `gen_tes3_schema.py`, `gen_viewer_themes.py`, `gen_tes3_enums.py`,
  `gen_tes3_fieldtypes.py`, `gen_merged_lands_table.py`): kept as the record of where the
  crates' fixtures and tables came from (the Python app, last in the commit before 5.0). They do not run any
  more; a fixture that needs regenerating is edited by hand or regenerated from that
  commit.
- `build_locale.py`: builds a language's `.po`/`.mo` from `locale/` (`locale/README.md`);
  `make_pot.py` extracted the template from the Python app's source and has nothing to
  read now. Both kept for the translations (`TODO.md`).
- `check_plugin_roundtrip.py`, `diff_roundtrip_json.py`: tes3conv round-trip checks.
