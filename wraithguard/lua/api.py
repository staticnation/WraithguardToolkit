"""What OpenMW's Lua API allows where, for one API version.

Taken from the OpenMW 0.51.0 reference (Lua API revision 129):
https://openmw.readthedocs.io/en/openmw-0.51.0/reference/lua-scripting/engine_handlers.html
https://openmw.readthedocs.io/en/openmw-0.51.0/reference/lua-scripting/events.html
and the package pages beside them. The API changes between releases, so every table
here belongs to :data:`API` and a newer OpenMW gets a new :class:`ApiVersion` rather
than an edit of this one.

A script's *contexts* come from its ``.omwscripts`` flags: ``global``, ``menu``,
``load``, ``player`` (a local script on the player) and ``local`` (a local script on
any other object).

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from dataclasses import dataclass

_ANY = frozenset({"global", "menu", "load", "player", "local"})
_NON_MENU = frozenset({"global", "player", "local"})
_LOCAL = frozenset({"player", "local"})
_MENU_PLAYER = frozenset({"menu", "player"})


@dataclass(frozen=True)
class ApiVersion:
    """The rules of one OpenMW Lua API version.

    Attributes:
        openmw: The OpenMW release, e.g. ``"0.51.0"``.
        revision: Its ``core.API_REVISION``.
        handlers: Engine handler name -> the contexts that may define it.
        per_frame: Engine handlers called every frame.
        packages: ``openmw.*`` package -> the contexts that may require it (missing:
            any context).
        builtin_events: Events the engine's own scripts handle, so sending one is
            not a typo even when no mod handles it.
        builtin_interfaces: Interfaces the engine's own scripts provide.
    """

    openmw: str
    revision: int
    handlers: dict[str, frozenset[str]]
    per_frame: frozenset[str]
    packages: dict[str, frozenset[str]]
    builtin_events: frozenset[str]
    builtin_interfaces: frozenset[str]


API = ApiVersion(
    openmw="0.51.0",
    revision=129,
    handlers={
        "onInterfaceOverride": _ANY,
        "onInit": _NON_MENU,
        "onUpdate": _NON_MENU,
        "onSave": _NON_MENU,
        "onLoad": _NON_MENU,
        "onNewGame": frozenset({"global"}),
        "onPlayerAdded": frozenset({"global"}),
        "onObjectActive": frozenset({"global"}),
        "onActorActive": frozenset({"global"}),
        "onItemActive": frozenset({"global"}),
        "onActivate": frozenset({"global"}),
        "onNewExterior": frozenset({"global"}),
        "onActive": _LOCAL,
        "onInactive": _LOCAL,
        "onTeleported": _LOCAL,
        "onActivated": _LOCAL,
        "onConsume": _LOCAL,
        "onFrame": _MENU_PLAYER,
        "onKeyPress": _MENU_PLAYER,
        "onKeyRelease": _MENU_PLAYER,
        "onControllerButtonPress": _MENU_PLAYER,
        "onControllerButtonRelease": _MENU_PLAYER,
        "onInputAction": _MENU_PLAYER,
        "onTouchPress": _MENU_PLAYER,
        "onTouchRelease": _MENU_PLAYER,
        "onTouchMove": _MENU_PLAYER,
        "onMouseButtonPress": _MENU_PLAYER,
        "onMouseButtonRelease": _MENU_PLAYER,
        "onMouseWheel": _MENU_PLAYER,
        "onConsoleCommand": _MENU_PLAYER,
        "onQuestUpdate": frozenset({"player"}),
        "onStateChanged": frozenset({"menu"}),
        "onContentFilesLoaded": frozenset({"load"}),
    },
    per_frame=frozenset({"onUpdate", "onFrame"}),
    packages={
        "openmw.world": frozenset({"global"}),
        "openmw.self": _LOCAL,
        "openmw.nearby": _LOCAL,
        "openmw.ui": _MENU_PLAYER,
        "openmw.input": _MENU_PLAYER,
        "openmw.camera": frozenset({"player"}),
        "openmw.postprocessing": frozenset({"player"}),
        "openmw.ambient": _MENU_PLAYER,
        "openmw.menu": frozenset({"menu"}),
        "openmw.content": frozenset({"load"}),
    },
    builtin_events=frozenset(
        {
            "DialogueResponse",
            "Died",
            "StartAIPackage",
            "RemoveAIPackages",
            "UseItem",
            "ModifyStat",
            "AddVfx",
            "PlaySound3d",
            "BreakInvisibility",
            "Unequip",
            "Hit",
            "ModifyItemCondition",
            "ShowMessage",
            "UiModeChanged",
            "AddUiMode",
            "SetUiMode",
            "Pause",
            "Unpause",
            "SetGameTimeScale",
            "SetSimulationTimeScale",
            "SpawnVfx",
            "ConsumeItem",
            "Lock",
            "Unlock",
        }
    ),
    builtin_interfaces=frozenset(
        {
            "Activation",
            "AI",
            "AnimationController",
            "Camera",
            "Combat",
            "Controls",
            "Crimes",
            "GamepadControls",
            "ItemUsage",
            "MWUI",
            "Settings",
            "SkillProgression",
            "UI",
        }
    ),
)


def contexts_for_flags(flags: tuple[str, ...]) -> frozenset[str]:
    """The contexts a script with these ``.omwscripts`` flags runs in.

    Args:
        flags: The flags, upper case.

    Returns:
        A subset of ``global``, ``menu``, ``load``, ``player``, ``local``.
    """
    out: set[str] = set()
    for f in flags:
        if f == "GLOBAL":
            out.add("global")
        elif f == "MENU":
            out.add("menu")
        elif f == "LOAD":
            out.add("load")
        elif f == "PLAYER":
            out.add("player")
        elif f == "CUSTOM":
            # Attached by a global script to any object - the player included.
            out.update(("local", "player"))
        else:
            out.add("local")  # the object types: a local script on that kind of object
    return frozenset(out)
