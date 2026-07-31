"""Read, simulate and emit ``openmw.cfg`` customisations.

Split by concern: :mod:`~wraithguard.configurator.cfglines` for individual
lines, :mod:`~wraithguard.configurator.datapaths` for VFS ordering,
:mod:`~wraithguard.configurator.apply` for the dry-run simulation, and
:mod:`~wraithguard.configurator.emit` for generating the TOML.
"""

from __future__ import annotations

from wraithguard.configurator.apply import (
    REMOVE_KEYS,
    configurator_remove_matches,
    customization_string_list,
    preview_configurator_result,
    simulate_configurator_apply,
)
from wraithguard.configurator.cfglines import (
    cfg_line_value,
    detect_data_quoting,
    extract_data_path_value,
    find_anchor_index,
    format_data_line,
    normalize_data_path,
    toml_value,
)
from wraithguard.configurator.datapaths import (
    infer_data_path_anchors,
    insert_data_paths,
)
from wraithguard.configurator.emit import generate_customizations_toml

__all__ = [
    "REMOVE_KEYS",
    "cfg_line_value",
    "configurator_remove_matches",
    "customization_string_list",
    "detect_data_quoting",
    "extract_data_path_value",
    "find_anchor_index",
    "format_data_line",
    "generate_customizations_toml",
    "infer_data_path_anchors",
    "insert_data_paths",
    "normalize_data_path",
    "preview_configurator_result",
    "simulate_configurator_apply",
    "toml_value",
]
