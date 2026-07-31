"""Hardening tests: malformed, hostile and unusual real-world input.

Every case here corresponds to a defect found by adversarial probing of the
parsers and writers. The tool consumes files it did not create -- plugins from
the internet, hand-edited cfg/TOML, downloads -- so "does not crash" and "does
not corrupt" are features, not implementation details.
"""

from __future__ import annotations

import struct
from pathlib import Path

import pytest

import wraithguard_toolkit as mss
from wraithguard.configurator import simulate_configurator_apply, toml_value
from wraithguard.plugins import PluginFileIndex
from wraithguard.sort import build_and_sort, expand_pattern

# Deliberately malformed TES3 byte streams. Each has broken something a naive
# reader would trust: the magic, a declared size, or a subrecord boundary.
MALFORMED_PLUGINS: dict[str, bytes] = {
    "empty": b"",
    "magic_only": b"TES3",
    "partial_size_field": b"TES3\x10\x00",
    "header_no_body": b"TES3" + struct.pack("<III", 100, 0, 0),
    "declared_size_exceeds_file": b"TES3" + struct.pack("<III", 0xFFFFFFFF, 0, 0) + b"AB",
    "zero_length_record": b"TES3" + struct.pack("<III", 0, 0, 0) * 2,
    "subrecord_size_overflow": (
        b"TES3"
        + struct.pack("<III", 20, 0, 0)
        + struct.pack("<4sI", b"MAST", 0xFFFFFFFF)
        + b"x" * 8
    ),
    "zero_length_subrecords": (
        b"TES3"
        + struct.pack("<III", 16, 0, 0)
        + struct.pack("<4sI", b"MAST", 0)
        + struct.pack("<4sI", b"DATA", 0)
    ),
    "truncated_mid_subrecord": (
        b"TES3" + struct.pack("<III", 40, 0, 0) + struct.pack("<4sI", b"MAST", 30) + b"short"
    ),
    "mast_without_data": (
        b"TES3" + struct.pack("<III", 13, 0, 0) + struct.pack("<4sI", b"MAST", 5) + b"a.esm"
    ),
    "data_field_too_short": (
        b"TES3"
        + struct.pack("<III", 21, 0, 0)
        + struct.pack("<4sI", b"MAST", 5)
        + b"a.esm"
        + struct.pack("<4sI", b"DATA", 4)
        + b"\x01\x02\x03\x04"
    ),
    "non_utf8_master_name": (
        b"TES3" + struct.pack("<III", 14, 0, 0) + struct.pack("<4sI", b"MAST", 6) + b"\xff\xfe.esm"
    ),
    "many_empty_records": (
        b"TES3" + struct.pack("<III", 0, 0, 0) + (b"STAT" + struct.pack("<III", 0, 0, 0)) * 500
    ),
}


@pytest.fixture(params=sorted(MALFORMED_PLUGINS), ids=sorted(MALFORMED_PLUGINS))
def malformed_plugin(request, tmp_path: Path) -> Path:
    path = tmp_path / f"{request.param}.esp"
    path.write_bytes(MALFORMED_PLUGINS[request.param])
    return path


class TestBinaryReadersTolerateGarbage:
    """Readers must degrade to "no data", never raise or hang."""

    def test_read_plugin_masters(self, core, malformed_plugin):
        assert isinstance(core.read_plugin_masters(malformed_plugin), list)

    def test_read_plugin_masters_with_sizes(self, core, malformed_plugin):
        assert isinstance(core.read_plugin_masters_with_sizes(malformed_plugin), list)

    def test_parse_tes3_records(self, core, malformed_plugin):
        assert isinstance(list(core.parse_tes3_records(malformed_plugin)), list)

    def test_read_savegame_content_files(self, core, malformed_plugin):
        files, error = core.read_savegame_content_files(malformed_plugin)
        assert files is None or isinstance(files, list)
        assert files is not None or error


class TestResyncNeverCorrupts:
    """sync_plugin_master_sizes writes to the user's plugins -- the blast
    radius of a mistake here is real data loss."""

    def test_malformed_input_is_rejected_without_mutation(self, core, malformed_plugin, tmp_path):
        index = PluginFileIndex([str(tmp_path)])
        before = malformed_plugin.read_bytes()

        updated, _unresolved, _error = core.sync_plugin_master_sizes(malformed_plugin, index)

        after = malformed_plugin.read_bytes()
        assert len(after) == len(before), "file size changed"
        if not updated:
            assert after == before, "file mutated without reporting an update"

    def test_magic_only_file_is_rejected_cleanly(self, core, tmp_path):
        """Regression: a 4-byte file passed the magic check then unpacked past
        the end of the buffer, raising struct.error."""
        stub = tmp_path / "Truncated.esp"
        stub.write_bytes(b"TES3")

        updated, unresolved, error = core.sync_plugin_master_sizes(stub, tmp_path)

        assert error is not None and "not a TES3" in error
        assert updated == [] and unresolved == []
        assert stub.read_bytes() == b"TES3"


class TestScannersTolerateGarbage:
    def test_scanners_survive_a_directory_of_broken_plugins(self, core, tmp_path):
        for name, blob in MALFORMED_PLUGINS.items():
            (tmp_path / f"{name}.esp").write_bytes(blob)
        (tmp_path / "plain_text.esp").write_text("this is not a plugin")
        index = PluginFileIndex([str(tmp_path)])
        names = sorted(p.name for p in tmp_path.iterdir())

        warnings, stats = core.lint_plugins(names, index, subset_names=names)
        missing, *_rest, problems = core.check_missing_masters(names, index)
        coverage = core.build_cell_coverage(names, index, subset_names=names)

        assert isinstance(warnings, list) and isinstance(stats, dict)
        assert isinstance(missing, list) and isinstance(problems, set)
        assert "exterior" in coverage and "interior" in coverage


class TestCfgEncodingRoundTrip:
    """openmw.cfg may contain bytes that are not valid UTF-8 (a cp1252
    accented mod folder). Losing them rewrites the user's data= path and
    breaks their load order."""

    NON_UTF8 = (
        'data="E:/Mods/Caf\xe9/Data Files"\n' "content=Morrowind.esm\n" "content=Caf\xe9Mod.esp\n"
    ).encode("latin-1")

    def test_round_trip_is_byte_preserving(self, core, tmp_path):
        cfg = tmp_path / "openmw.cfg"
        cfg.write_bytes(self.NON_UTF8)

        lines, _cp, _content, _dp, _data = core.read_cfg(cfg)
        core.write_cfg(cfg, lines, [], dry_run=False, no_backup=True)

        assert cfg.read_bytes() == self.NON_UTF8

    def test_backup_is_byte_identical(self, core, tmp_path):
        """Regression: the backup decoded then re-encoded as UTF-8, which
        raised UnicodeDecodeError and blocked export entirely."""
        cfg = tmp_path / "openmw.cfg"
        cfg.write_bytes(self.NON_UTF8)

        core.backup_file(cfg, no_backup=False)

        backup = next(tmp_path.glob("openmw.cfg.bak-*"))
        assert backup.read_bytes() == self.NON_UTF8

    def test_unicode_content_is_parsed(self, core, tmp_path):
        cfg = tmp_path / "openmw.cfg"
        cfg.write_bytes('data="E:/Mods/日本語"\ncontent=Ünïcode.esp\n'.encode())

        _lines, _cp, content, _dp, data = core.read_cfg(cfg)

        assert [name for name, _ in content] == ["Ünïcode.esp"]
        assert "日本語" in data[0]

    def test_subset_file_with_non_utf8_bytes_does_not_crash(self, core, tmp_path):
        subset = tmp_path / "subset.txt"
        subset.write_bytes("Caf\xe9Mod.esp\nOther.esp\n".encode("latin-1"))

        plugins, _data_paths = core.extract_subset_from_subset_file(subset)

        assert len(plugins) == 2
        assert plugins[1] == "Other.esp"

    def test_non_utf8_toml_reports_clearly(self, core, tmp_path):
        """TOML is spec-mandated UTF-8; the user needs an actionable message,
        not a raw UnicodeDecodeError."""
        toml_file = tmp_path / "customizations.toml"
        toml_file.write_bytes(b'[[Customizations]]\nlistName = "x"\ninsert = "Caf\xe9.esp"\n')

        with pytest.raises(SystemExit, match="not valid UTF-8"):
            core.extract_subset_from_toml(toml_file)


class TestCustomizationTypeSafety:
    """A TOML typo must not silently destroy the load order."""

    CFG = ["content=Alpha.esp", "content=Beta.esp", "content=Gamma.esp", 'data="E:/M/Core"']

    def test_string_instead_of_array_does_not_wipe_the_cfg(self):
        """Regression: iterating a string yielded single characters as removal
        patterns, which matched -- and deleted -- almost every line."""
        doc = "[[Customizations]]\nremoveContent = 'Alpha.esp'\n"

        lines, errors, _notes = simulate_configurator_apply(self.CFG, doc)

        assert lines == self.CFG, "cfg was modified by a malformed removeContent"
        assert any("must be an array" in e for e in errors)

    def test_non_string_entries_are_skipped_not_applied(self):
        doc = "[[Customizations]]\nremoveContent = ['Beta.esp', 123]\n"

        lines, errors, _notes = simulate_configurator_apply(self.CFG, doc)

        assert "content=Beta.esp" not in lines
        assert "content=Alpha.esp" in lines
        assert any("not a string" in e for e in errors)

    def test_non_table_customizations_entry_is_reported(self):
        lines, errors, _notes = simulate_configurator_apply(self.CFG, "Customizations = ['oops']\n")

        assert lines == self.CFG
        assert any("not a table" in e for e in errors)

    def test_valid_array_still_removes(self):
        lines, errors, _notes = simulate_configurator_apply(
            self.CFG, "[[Customizations]]\nremoveContent = ['Beta.esp']\n"
        )

        assert not errors
        assert "content=Beta.esp" not in lines and len(lines) == 3

    @pytest.mark.parametrize(
        "document",
        [
            "this is not toml at all {{{",
            "",
            "[other]\nx = 1\n",
            "[[Customizations]]\n[[Customizations.insert]]\ninsert='X.esp'\n",
            "[[Customizations]]\n[[Customizations.insert]]\nafter='A.esp'\n",
            "[[Customizations]]\n[[Customizations.insert]]\ninsert='X'\nafter='A.esp'\nbefore='B.esp'\n",
            "[[Customizations]]\n[[Customizations.replace]]\nsource='A.esp'\n",
        ],
    )
    def test_malformed_documents_are_handled_not_raised(self, document):
        lines, errors, _notes = simulate_configurator_apply(self.CFG, document)
        assert lines is None or isinstance(lines, list)
        assert isinstance(errors, list)


class TestPatternMatchingEdgeCases:
    """Real plugin names contain regex metacharacters."""

    NAMES = [
        "Mod (v1.2).esp",
        "Mod [Final].esp",
        "Mod+Plus.esp",
        "A^B.esp",
        "C$D.esp",
        "E|F.esp",
        "G{2}.esp",
        "back\\slash.esp",
        "dot.any.esp",
    ]

    @pytest.mark.parametrize("name", NAMES)
    def test_exact_names_with_metacharacters_match_only_themselves(self, name):
        assert expand_pattern(name, self.NAMES) == [name]

    @pytest.mark.parametrize(
        "pattern", ["Mod [Final]*.esp", "Mod (v*).esp", "A^*.esp", "G{2,}*.esp", "bad[.esp"]
    )
    def test_wildcard_patterns_with_metacharacters_do_not_raise(self, pattern):
        assert isinstance(expand_pattern(pattern, self.NAMES), list)


class TestSortDegenerateInputs:
    @pytest.mark.parametrize(
        ("base", "subset", "masters"),
        [
            ([], [], {}),
            ([], ["A.esp"], {"a.esp": []}),
            (["A.esp"], [], {}),
            (["A.esp"], ["A.esp"], {"a.esp": []}),
            (["A.esp"], ["B.esp"], {"b.esp": ["B.esp"]}),  # self-master
            (["A.esp"], ["B.esp"], {"b.esp": ["Ghost.esm"]}),  # missing master
            (["A.esp", "A.esp", "B.esp"], ["C.esp"], {"c.esp": []}),  # duplicate cfg lines
        ],
    )
    def test_degenerate_inputs_do_not_raise(self, base, subset, masters):
        assert isinstance(build_and_sort(base, subset, [], masters), list)

    def test_deep_transitive_chain_resolves(self):
        """600 mods each mastering the previous one -- the anchor resolver must
        not blow the recursion limit."""
        depth = 600
        subset = [f"C{i}.esp" for i in range(depth)]
        masters = {
            f"c{i}.esp": ["Morrowind.esm"] + ([f"C{i - 1}.esp"] if i else []) for i in range(depth)
        }

        result = build_and_sort(["Morrowind.esm"], subset, [], masters)

        assert result[1:] == subset


class TestTomlValueEscaping:
    """Emitted TOML must reparse -- a broken quote would corrupt the file the
    Configurator consumes."""

    @pytest.mark.parametrize(
        "name",
        [
            "Uvirith's Legacy_3.53.ESP",
            'Say "Hello".esp',
            "back\\slash.esp",
            "both'and\".esp",
            "triple'''quote.esp",
            "tab\there.esp",
            "unicode_日本語.esp",
            "trailing space .esp",
        ],
    )
    def test_any_name_round_trips(self, name):
        try:
            import tomllib
        except ModuleNotFoundError:  # pragma: no cover
            tomllib = pytest.importorskip("tomli")

        assert tomllib.loads(f"x = {toml_value(name)}")["x"] == name


class TestGroundcoverIsNeverContent:
    """A grass plugin must never be inserted as ``content=``.

    Reported by a user: "is it normal for it to toggle on ALL mods, including
    mods flagged as grass mods". It was not the folder scan's fault alone. The
    scan cannot tell a grass mod from any other mod -- it walks a directory and
    takes every plugin -- but the *cfg* can, because it declares grass on
    ``groundcover=`` lines, and the tool read those lines and ignored them.

    The result was the plugin declared twice: once as grass, once as content.
    OpenMW then loads the grass through the groundcover system *and* spawns
    every blade as a real object, which is exactly the cost groundcover exists
    to avoid, arriving silently.
    """

    CFG = (
        'data="E:/Morrowind/Data Files"\n'
        "content=Morrowind.esm\n"
        "content=Patch for Purists.esp\n"
        "groundcover=Remiros_Groundcover.esp\n"
        "groundcover=Vurt_Grass.esp\n"
    )

    def test_groundcover_lines_are_read(self, core, tmp_path: Path) -> None:
        """The information was always there; nothing used it.

        Args:
            core: The engine module.
            tmp_path: Pytest temporary directory.
        """
        cfg = tmp_path / "openmw.cfg"
        cfg.write_text(self.CFG, encoding="utf-8")
        lines, _cp, _content, _dp, _data = core.read_cfg(cfg)

        assert core.read_groundcover_names(lines) == [
            "Remiros_Groundcover.esp",
            "Vurt_Grass.esp",
        ]

    def test_groundcover_is_not_mistaken_for_content(self, core, tmp_path: Path) -> None:
        """The two live in the same file and must not bleed into each other.

        Args:
            core: The engine module.
            tmp_path: Pytest temporary directory.
        """
        cfg = tmp_path / "openmw.cfg"
        cfg.write_text(self.CFG, encoding="utf-8")
        _lines, _cp, content, _dp, _data = core.read_cfg(cfg)

        assert [name for name, _raw in content] == ["Morrowind.esm", "Patch for Purists.esp"]

    def test_commented_groundcover_is_skipped(self, core, tmp_path: Path) -> None:
        """A disabled grass line is disabled, exactly like a disabled content one.

        Args:
            core: The engine module.
            tmp_path: Pytest temporary directory.
        """
        cfg = tmp_path / "openmw.cfg"
        cfg.write_text("#groundcover=Off.esp\n# groundcover=AlsoOff.esp\n", encoding="utf-8")
        lines, *_rest = core.read_cfg(cfg)

        assert core.read_groundcover_names(lines) == []

    def test_a_declared_grass_plugin_is_held_back(self, core) -> None:
        """The fix, stated directly.

        Args:
            core: The engine module.
        """
        kept, held = core.hold_back_groundcover(
            ["MyNewQuest.esp", "Remiros_Groundcover.esp"], ["Remiros_Groundcover.esp"]
        )

        assert kept == ["MyNewQuest.esp"]
        assert held == ["Remiros_Groundcover.esp"]

    def test_matching_is_case_insensitive(self, core) -> None:
        """Plugin names on Windows are whatever case someone typed.

        Args:
            core: The engine module.
        """
        kept, held = core.hold_back_groundcover(
            ["REMIROS_groundcover.ESP"], ["Remiros_Groundcover.esp"]
        )

        assert kept == []
        assert held == ["REMIROS_groundcover.ESP"]

    def test_nothing_is_held_back_without_groundcover_lines(self, core) -> None:
        """A cfg with no grass must behave exactly as it did before.

        Args:
            core: The engine module.
        """
        subset = ["A.esp", "B.esp"]

        assert core.hold_back_groundcover(subset, []) == (subset, [])

    def test_ordinary_plugins_are_untouched(self, core) -> None:
        """The filter must be narrow: only what the cfg names as grass.

        Args:
            core: The engine module.
        """
        kept, held = core.hold_back_groundcover(
            ["Grass_Patch_Compatibility.esp"], ["Remiros_Groundcover.esp"]
        )

        assert kept == ["Grass_Patch_Compatibility.esp"]
        assert held == []

    def test_end_to_end_export_keeps_grass_out_of_content(self, core, tmp_path: Path) -> None:
        """The user's exact scenario, from subset to written cfg.

        The grass plugin's data path must still be written: OpenMW has to find
        the file for the ``groundcover=`` line to work at all, so dropping the
        data= entry would break the mod this check exists to protect.

        Args:
            core: The engine module.
            tmp_path: Pytest temporary directory.
        """
        data = tmp_path / "Data Files"
        data.mkdir()
        for name in ("Morrowind.esm", "Patch for Purists.esp"):
            (data / name).write_bytes(b"TES3" + b"\x00" * 300)
        grass = tmp_path / "mods" / "Remiros Groundcover"
        (grass / "meshes").mkdir(parents=True)
        (grass / "Remiros_Groundcover.esp").write_bytes(b"TES3" + b"\x00" * 300)
        quest = tmp_path / "mods" / "MyNewQuest"
        (quest / "meshes").mkdir(parents=True)
        (quest / "MyNewQuest.esp").write_bytes(b"TES3" + b"\x00" * 300)

        cfg = tmp_path / "openmw.cfg"
        cfg.write_text(
            f'data="{data}"\n'
            "content=Morrowind.esm\n"
            "content=Patch for Purists.esp\n"
            "groundcover=Remiros_Groundcover.esp\n",
            encoding="utf-8",
        )
        rules = tmp_path / "mlox_base.txt"
        rules.write_text("", encoding="utf-8")
        subset = tmp_path / "subset.txt"
        subset.write_text(
            f"{grass}\nRemiros_Groundcover.esp\n\n{quest}\nMyNewQuest.esp\n", encoding="utf-8"
        )

        args = core.build_arg_parser().parse_args(
            [
                "--cfg",
                str(cfg),
                "--rules",
                str(rules),
                "--subset-file",
                str(subset),
                "--write-cfg",
                "--sort-data-paths",
            ]
        )
        plan = core.compute_plan(args)
        core.write_plan(args, plan)

        written = cfg.read_text(encoding="utf-8").splitlines()
        content = [line for line in written if line.startswith("content=")]
        assert "content=Remiros_Groundcover.esp" not in content, "grass was inserted as content"
        assert "content=MyNewQuest.esp" in content, "the real custom mod was lost"
        assert "groundcover=Remiros_Groundcover.esp" in written, "the grass line was disturbed"
        assert any(
            line.startswith("data=") and "Remiros Groundcover" in line for line in written
        ), "the grass mod's data path must still be written or OpenMW cannot find it"


class TestDeclaringYourOwnGroundcover:
    """Grass the cfg does not know about yet.

    Holding back what the cfg already declares only helps a mod that is already
    installed and declared. A grass mod the user has just added is in neither
    place, so there is nothing to read the fact off -- they say so once, and the
    declaration drives both outputs.
    """

    def test_a_declaration_line_is_read(self, core) -> None:
        """The syntax deliberately matches openmw.cfg's own.

        Args:
            core: The engine module.
        """
        lines = ["C:/mods/Grass", "groundcover=Vurt_Grass.esp", "MyQuest.esp"]

        assert core.extract_groundcover_declarations(lines) == ["Vurt_Grass.esp"]

    def test_a_declared_plugin_does_not_become_content(self, core) -> None:
        """Reading it as a subset entry too would defeat the whole thing.

        Args:
            core: The engine module.
        """
        plugins, _data = core.extract_subset_from_lines(
            ["groundcover=Vurt_Grass.esp", "MyQuest.esp"]
        )

        assert plugins == ["MyQuest.esp"]

    def test_the_data_path_line_is_still_a_data_path(self, core) -> None:
        """The half that makes it work: OpenMW must find the file.

        Args:
            core: The engine module.
        """
        plugins, data = core.extract_subset_from_lines(
            ["C:/mods/Vurt Grass", "groundcover=Vurt_Grass.esp"]
        )

        assert plugins == []
        assert [d["value"] for d in data] == ["C:/mods/Vurt Grass"]

    def test_declarations_are_deduplicated_case_insensitively(self, core) -> None:
        """Naming it twice is a typo, not two mods.

        Args:
            core: The engine module.
        """
        lines = ["groundcover=Grass.esp", "groundcover=GRASS.ESP"]

        assert core.extract_groundcover_declarations(lines) == ["Grass.esp"]

    def test_a_non_plugin_declaration_is_refused(self, core) -> None:
        """ "groundcover=some folder" is a mistake worth naming.

        Args:
            core: The engine module.
        """
        assert core.extract_groundcover_declarations(["groundcover=not a plugin"]) == []

    def test_comments_are_stripped(self, core) -> None:
        """Subset files allow them everywhere else.

        Args:
            core: The engine module.
        """
        assert core.extract_groundcover_declarations(["groundcover=Grass.esp  # my grass"]) == [
            "Grass.esp"
        ]

    def test_end_to_end_declaration_reaches_both_outputs(self, core, tmp_path: Path) -> None:
        """The whole feature, from subset file to cfg and TOML.

        The plugin must be absent from ``content=``, present as ``groundcover=``,
        and its data path present -- in both the patched cfg and the emitted
        customizations.

        Args:
            core: The engine module.
            tmp_path: Pytest temporary directory.
        """
        base = tmp_path / "Data Files"
        base.mkdir()
        for name in ("Morrowind.esm", "Patch for Purists.esp"):
            (base / name).write_bytes(b"TES3" + b"\x00" * 300)
        grass = tmp_path / "mods" / "Vurt Grass"
        (grass / "meshes").mkdir(parents=True)
        (grass / "Vurt_Grass.esp").write_bytes(b"TES3" + b"\x00" * 300)

        cfg = tmp_path / "openmw.cfg"
        cfg.write_text(
            f'data="{base}"\ncontent=Morrowind.esm\ncontent=Patch for Purists.esp\n',
            encoding="utf-8",
        )
        rules = tmp_path / "mlox_base.txt"
        rules.write_text("", encoding="utf-8")
        subset = tmp_path / "subset.txt"
        subset.write_text(f"{grass}\ngroundcover=Vurt_Grass.esp\n", encoding="utf-8")
        out_toml = tmp_path / "out.toml"

        args = core.build_arg_parser().parse_args(
            [
                "--cfg",
                str(cfg),
                "--rules",
                str(rules),
                "--subset-file",
                str(subset),
                "--emit-toml",
                str(out_toml),
                "--list-name",
                "total-overhaul",
                "--write-cfg",
                "--sort-data-paths",
            ]
        )
        plan = core.compute_plan(args)
        core.write_plan(args, plan)

        written = cfg.read_text(encoding="utf-8").splitlines()
        assert "content=Vurt_Grass.esp" not in written, "declared grass became content"
        assert "groundcover=Vurt_Grass.esp" in written, "the groundcover line was not written"
        assert any(
            "Vurt Grass" in line and line.startswith("data=") for line in written
        ), "the data path is required or OpenMW cannot find the file"

        toml_text = out_toml.read_text(encoding="utf-8")
        assert "append = 'groundcover=Vurt_Grass.esp'" in toml_text
        assert "insert = 'Vurt_Grass.esp'" not in toml_text
        assert "Vurt Grass" in toml_text, "the data path insert is missing from the TOML"

    def test_the_cli_flag_declares_too(self, core, tmp_path: Path) -> None:
        """Parity with --subset, for people who do not keep a subset file.

        Args:
            core: The engine module.
            tmp_path: Pytest temporary directory.
        """
        args = core.build_arg_parser().parse_args(
            [
                "--cfg",
                str(tmp_path / "openmw.cfg"),
                "--rules",
                str(tmp_path / "r.txt"),
                "--groundcover",
                "Vurt_Grass.esp",
                "Remiros.esp",
            ]
        )

        assert core.declared_groundcover(args) == ["Vurt_Grass.esp", "Remiros.esp"]


class TestResourceConflictsCompareContents:
    """A path in two data folders is a candidate, not necessarily a conflict.

    The scan used to report every shared path, so a mod that re-ships an
    unedited vanilla texture -- or a patch that simply re-includes an asset it
    did not touch -- appeared exactly like a genuine override. On a real load
    order that is most of the list, and it buries the overrides that matter.
    """

    @staticmethod
    def build(tmp_path: Path, layout: dict[str, dict[str, bytes]]) -> list[str]:
        """Create data folders from a nested mapping.

        Args:
            tmp_path: The temp directory to build under.
            layout: Folder name to ``{relative path: bytes}``.

        Returns:
            The folder paths, in the order given.
        """
        made: list[str] = []
        for folder, files in layout.items():
            root = tmp_path / folder
            for rel, blob in files.items():
                target = root / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(blob)
            made.append(str(root))
        return made

    def test_identical_files_are_marked(self, tmp_path: Path) -> None:
        """Same bytes in every provider is not a decision anybody has to make.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(
            tmp_path,
            {
                "a": {"textures/t.dds": b"same" * 64},
                "b": {"textures/t.dds": b"same" * 64},
            },
        )
        conflicts, stats = mss.detect_resource_conflicts(dirs)

        assert [c["identical"] for c in conflicts] == [True]
        assert stats["identical"] == 1
        assert stats["differing"] == 0

    def test_a_real_override_is_not(self, tmp_path: Path) -> None:
        """Same length, different bytes -- the case the size check cannot settle.

        Deliberately equal-length, because a size comparison alone would call
        these identical and the hash is the only thing that catches it.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(
            tmp_path,
            {"a": {"meshes/m.nif": b"A" * 512}, "b": {"meshes/m.nif": b"B" * 512}},
        )
        conflicts, stats = mss.detect_resource_conflicts(dirs)

        assert conflicts[0]["identical"] is False
        assert stats["differing"] == 1

    def test_differing_files_sort_first(self, tmp_path: Path) -> None:
        """The list is read from the top, so the decisions belong there.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(
            tmp_path,
            {
                "a": {"aaa_same.dds": b"x" * 8, "zzz_differs.dds": b"y" * 8},
                "b": {"aaa_same.dds": b"x" * 8, "zzz_differs.dds": b"z" * 8},
            },
        )
        conflicts, _stats = mss.detect_resource_conflicts(dirs)

        assert conflicts[0]["path"] == "zzz_differs.dds", "an override must outrank a duplicate"
        assert conflicts[-1]["path"] == "aaa_same.dds"

    def test_a_file_that_cannot_be_sized_is_never_called_identical(self, tmp_path: Path) -> None:
        """The size stage has to fail closed too.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(tmp_path, {"a": {"x.dds": b"same"}, "b": {"x.dds": b"same"}})
        replaced = Path(dirs[1]) / "x.dds"
        replaced.unlink()
        replaced.mkdir()

        conflicts, _stats = mss.detect_resource_conflicts(dirs)

        assert conflicts == [] or conflicts[0]["identical"] is False

    def test_a_file_that_cannot_be_read_is_never_called_identical(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Claiming a match on a failed *read* would retire a real conflict.

        The sizes are made to agree so the comparison gets past the cheap check
        and into the hash, which is the branch under test. A first attempt used
        a directory in place of the file and never reached here -- the sizes
        disagreed and it returned one stage earlier -- so the mutation that
        made a read failure mean "identical" went uncaught.

        Args:
            tmp_path: Pytest's temp directory.
            monkeypatch: Pytest's patcher.
        """
        dirs = self.build(tmp_path, {"a": {"x.dds": b"same" * 8}, "b": {"x.dds": b"same" * 8}})
        blocked = Path(dirs[1]) / "x.dds"
        real_open = Path.open

        def refuse(self: Path, *args: object, **kwargs: object):
            """Fail to open one specific file, as a permission error would.

            Args:
                self: The path being opened.
                args: Passed through.
                kwargs: Passed through.

            Returns:
                The real file object for every other path.

            Raises:
                OSError: For the blocked path.
            """
            if self == blocked:
                raise OSError(13, "Permission denied")
            return real_open(self, *args, **kwargs)

        monkeypatch.setattr(Path, "open", refuse)

        conflicts, _stats = mss.detect_resource_conflicts(dirs)

        assert conflicts[0]["identical"] is False

    def test_comparison_can_be_switched_off(self, tmp_path: Path) -> None:
        """A slow or network filesystem may not want the read.

        The key is then absent rather than ``False``: "not compared" and
        "compared and differing" are different facts, and a caller filtering on
        the second must not sweep up the first.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(tmp_path, {"a": {"t.dds": b"one"}, "b": {"t.dds": b"two"}})

        conflicts, stats = mss.detect_resource_conflicts(dirs, compare_contents=False)

        assert "identical" not in conflicts[0]
        assert stats["identical"] == 0

    def test_the_report_says_how_many_need_no_decision(self, tmp_path: Path) -> None:
        """The sentence that makes a list of thousands legible.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(
            tmp_path,
            {"a": {"s.dds": b"q" * 4, "d.dds": b"w"}, "b": {"s.dds": b"q" * 4, "d.dds": b"e"}},
        )
        conflicts, stats = mss.detect_resource_conflicts(dirs)

        report = mss.format_resource_report(conflicts, stats)

        assert "1 differ" in report
        assert "1 are byte-identical" in report
        assert "[identical]" in report

    def test_the_csv_distinguishes_not_compared_from_differing(self, tmp_path: Path) -> None:
        """An empty cell, not "no", when nothing was compared.

        Args:
            tmp_path: Pytest's temp directory.
        """
        dirs = self.build(tmp_path, {"a": {"t.dds": b"one"}, "b": {"t.dds": b"two"}})
        out = tmp_path / "r.csv"

        conflicts, _ = mss.detect_resource_conflicts(dirs, compare_contents=False)
        mss.write_resource_csv(out, conflicts)
        skipped = out.read_text(encoding="utf-8").splitlines()[1]

        conflicts, _ = mss.detect_resource_conflicts(dirs)
        mss.write_resource_csv(out, conflicts)
        compared = out.read_text(encoding="utf-8").splitlines()[1]

        assert ",," in skipped, "an uncompared file must leave the column blank"
        assert ",no," in compared
