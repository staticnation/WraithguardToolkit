"""Whole-plugin merging -- a port of ``merge_to_master`` (public domain, Greatness7).

Folds a plugin, or a whole load order, into a single master file: objects resolve
last-plugin-wins, cells union their references, dialogue keeps its linked-list
order, and master/texture indices are remapped so edits keep pointing at the
right objects. This is distinct from :mod:`wraithguard.patch.merge`, which merges
one record field-by-field; here whole plugins are combined into one file.
"""

from __future__ import annotations

from wraithguard.merge.api import (
    MergeOptions,
    apply_moved_references,
    merge_load_order,
    merge_plugins,
    remove_duplicate_references,
)
from wraithguard.merge.combine import merge_plugin_into
from wraithguard.merge.model import PluginData

__all__ = [
    "MergeOptions",
    "PluginData",
    "apply_moved_references",
    "merge_load_order",
    "merge_plugin_into",
    "merge_plugins",
    "remove_duplicate_references",
]
