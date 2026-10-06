"""The load order's dialogue as something to look at: flows, maps, flags and a rehearsal.

A topic in the Construction Set is a list, and a list hides everything that makes
dialogue work: which response the engine actually picks (the first whose conditions
all hold, in the order :mod:`.dialogue` resolves), which lines branch on a ``Choice``,
which topics a line opens (``AddTopic``, and the words of its text the game
hyperlinks), which quest stages it sets, and which globals, locals and journal indexes
one line writes and another reads. This module reads all of that once, for the whole
load order, and answers four ways of looking at it:

- **Flow** (:meth:`DialogueIndex.flow`): one topic's responses in engine order, each
  with its conditions in words, its text cut into plain words and topic links, and what
  its result script does - the ``Choice`` options pointing at the responses that answer
  them (a tree inside the topic), the topics it adds, the quest stages, the variables.
- **Map** (:meth:`DialogueIndex.neighbourhood`): topics as a graph - what each one leads
  to and what leads to it, through ``AddTopic``, hyperlinked words, quests and shared
  variables - around one topic, a step or two out.
- **Flags** (:meth:`DialogueIndex.flags`): the blackboard - every global, local,
  journal index and item the dialogue writes or tests, with who writes it and who reads
  it, so one tree's effect on another is visible without a physical link between them.
- **Rehearsal** (:meth:`DialogueIndex.play`): the game's own dialogue window, as near as
  can be without running the game - a chosen NPC's greeting, the topics they answer,
  the response the engine would pick for each (the speaker conditions checked; the ones
  only the running game knows, said and assumed), hyperlinks and choices clickable.

The conditions in words, and the result-script reading, are the toolkit's own MWDE
adaptations (:mod:`wraithguard.tes3fields.dialogue`, :mod:`.journal_scripts`).

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final

from wraithguard.patch.dialogue import Response, orphans, responses_by_topic, topic_order
from wraithguard.tes3fields.dialogue import condition_lines, describe_filter

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

#: The topic kinds the game lists, hyperlinks and asks (the others answer events).
TOPIC: Final = "Topic"
GREETING: Final = "Greeting"
JOURNAL: Final = "Journal"

#: How many nodes a map answers at most (a busy topic links to hundreds).
MAP_CAP: Final = 80

_CHOICE_ARG = re.compile(r'"([^"]*)"\s*,?\s*(-?\d+)')
_LINE = re.compile(r"[^\r\n]+")
_SET = re.compile(r"(?i)^\s*set\s+(?:\"?([\w.\- ]+?)\"?\s*->\s*)?\"?([\w.\-]+)\"?\s+to\b")
_CALL = re.compile(r'(?i)^\s*(?:"?([\w.\- ]+?)"?\s*->\s*)?([a-z_]\w*)\b\s*,?\s*(.*)$')
_FIRST_ARG = re.compile(r'^\s*(?:"([^"]*)"|([^\s,]+))\s*,?\s*(.*)$')
#: The text macros the game fills in (shown as themselves, marked, in a rehearsal).
_MACRO = re.compile(r"%[A-Za-z]+")
#: A word, as topic names are matched: letters, digits and apostrophes.
_WORD = re.compile(r"[\w']+")


def _strip_comment(line: str) -> str:
    """A script line without its ``;`` comment (a ``;`` inside quotes is text)."""
    quoted = False
    for i, ch in enumerate(line):
        if ch == '"':
            quoted = not quoted
        elif ch == ";" and not quoted:
            return line[:i]
    return line


def _arg(rest: str) -> tuple[str, str]:
    """The first argument of a call (quoted or bare) and what follows it."""
    m = _FIRST_ARG.match(rest)
    if not m:
        return "", ""
    return (m.group(1) if m.group(1) is not None else m.group(2) or ""), m.group(3) or ""


@dataclass
class Effects:
    """What a result script does, as far as reading it tells.

    Attributes:
        choices: ``(label, number)`` for each ``Choice`` option, in order.
        addtopics: Topics it adds.
        journal: ``(quest, index)`` for each ``Journal``/``SetJournalIndex``.
        sets: ``(variable, target)`` for each ``set X to`` (``target`` the reference
            before ``->``, empty for the script's own or a global).
        items: ``(item, verb)`` for each ``AddItem``/``RemoveItem``.
        goodbye: It ends the conversation (``Goodbye``).
        other: Every other call, by name, once each.
    """

    choices: list[tuple[str, int]] = field(default_factory=list)
    addtopics: list[str] = field(default_factory=list)
    journal: list[tuple[str, int]] = field(default_factory=list)
    sets: list[tuple[str, str]] = field(default_factory=list)
    items: list[tuple[str, str]] = field(default_factory=list)
    goodbye: bool = False
    other: list[str] = field(default_factory=list)


def script_effects(text: str) -> Effects:
    """Read a result script for what it does (a line scan, not a compiler).

    Args:
        text: The script source.

    Returns:
        Its effects.
    """
    out = Effects()
    for raw in _LINE.findall(text or ""):
        line = _strip_comment(raw).strip()
        if not line:
            continue
        m = _SET.match(line)
        if m:
            out.sets.append((m.group(2), (m.group(1) or "").strip()))
            continue
        m = _CALL.match(line)
        if not m:
            continue
        target, name, rest = (m.group(1) or "").strip(), m.group(2).lower(), m.group(3) or ""
        if name in (
            "if",
            "elseif",
            "else",
            "endif",
            "while",
            "endwhile",
            "short",
            "long",
            "float",
            "return",
        ):
            continue
        if name == "choice":
            out.choices.extend((label, int(n)) for label, n in _CHOICE_ARG.findall(rest))
        elif name == "addtopic":
            topic, _ = _arg(rest)
            if topic:
                out.addtopics.append(topic)
        elif name in ("journal", "setjournalindex"):
            quest, more = _arg(rest)
            num = re.match(r"\s*(-?\d+)", more)
            if quest and num:
                out.journal.append((quest, int(num.group(1))))
        elif name in ("additem", "removeitem"):
            item, _ = _arg(rest)
            if item:
                out.items.append((item, "add" if name == "additem" else "remove"))
        elif name == "goodbye":
            out.goodbye = True
        else:
            call = (f"{target}->" if target else "") + m.group(2)
            if call not in out.other:
                out.other.append(call)
    return out


def choice_gate(rec: Mapping[str, Any]) -> int | None:
    """The ``Choice`` a response answers (its ``Function Choice = n`` condition), if any."""
    for flt in rec.get("filters") or []:
        if (
            isinstance(flt, dict)
            and flt.get("filter_type") == "Function"
            and flt.get("function") == "Choice"
        ):
            value = flt.get("value")
            data = value.get("data") if isinstance(value, dict) else value
            if isinstance(data, (int, float)):
                return int(data)
            return None
    return None


#: Filter kinds that read a name the dialogue may write somewhere else.
_READ_KINDS: Final = {
    "Global": "global",
    "Local": "local",
    "NotLocal": "local",
    "Journal": "journal",
    "Item": "item",
    "Dead": "dead",
}


def reads_of(rec: Mapping[str, Any]) -> list[tuple[str, str, str]]:
    """The named things a response's conditions test: ``(kind, name, clause)``."""
    out: list[tuple[str, str, str]] = []
    for flt in rec.get("filters") or []:
        if not isinstance(flt, dict):
            continue
        kind = _READ_KINDS.get(str(flt.get("filter_type")))
        name = str(flt.get("id") or "")
        if kind and name:
            out.append((kind, name, describe_filter(flt) or ""))
    return out


def speaker_line(rec: Mapping[str, Any]) -> str:
    """Who says it: ``Actor: x & Race: y``, or ``anyone``."""
    parts = [
        f"{label}: {rec[k]}"
        for k, label in (
            ("speaker_id", "Actor"),
            ("speaker_race", "Race"),
            ("speaker_class", "Class"),
            ("speaker_faction", "Faction"),
            ("speaker_cell", "Cell"),
        )
        if rec.get(k)
    ]
    return " & ".join(parts) or "anyone"


@dataclass
class Line:
    """One response, as every view reads it.

    Attributes:
        topic: The topic's id, as spelled.
        key: The response's id.
        rec: The winning record (the pool's typed changes applied).
        plugins: Every plugin defining it, in load order.
        orphan: Its predecessor is not in the topic (the engine puts it last).
    """

    topic: str
    key: str
    rec: dict[str, Any]
    plugins: list[str]
    orphan: bool = False
    _effects: Effects | None = None

    @property
    def text(self) -> str:
        """What is said (or the journal entry)."""
        return str(self.rec.get("text") or "")

    @property
    def script(self) -> str:
        """The result script."""
        return str(self.rec.get("script_text") or "")

    @property
    def effects(self) -> Effects:
        """What its result script does (read once)."""
        if self._effects is None:
            self._effects = script_effects(self.script)
        return self._effects


class DialogueIndex:
    """Every topic and response of a load order, read once, for the views above.

    Args:
        plugins: ``(name, records)`` per plugin in load order - the plugin's topics and
            responses in file order (what :meth:`.EditorSession._dialogue` reads).
        made: Responses and topics the patch makes: ``(record_type, topic, key, record)``.
        edits: The pool's typed changes to responses: response id (lower) -> ``{path:
            value}``, laid over the winning record (so a rehearsal says the new line).
    """

    def __init__(
        self,
        plugins: Sequence[tuple[str, Sequence[Mapping[str, Any]]]],
        made: Iterable[tuple[str, str, str, Mapping[str, Any]]] = (),
        edits: Mapping[str, Mapping[str, Any]] | None = None,
    ) -> None:
        """Read every topic: its responses in engine order, with the pool's changes."""
        from wraithguard.patch.merge import set_at

        defs: dict[str, list[Response]] = {}
        latest: dict[tuple[str, str], tuple[str, Mapping[str, Any]]] = {}
        self.kinds: dict[str, str] = {}
        self.spelled: dict[str, str] = {}
        for name, records in plugins:
            for tid, found in responses_by_topic(records, name).items():
                low = tid.lower()
                defs.setdefault(low, []).extend(found)
                self.spelled.setdefault(low, tid)
            current = ""
            for rec in records:
                kind = rec.get("type")
                if kind == "Dialogue":
                    tid = str(rec.get("id") or "")
                    current = tid.lower()
                    self.spelled.setdefault(current, tid)
                    self.kinds[current] = str(
                        rec.get("dialogue_type") or self.kinds.get(current, "")
                    )
                elif kind == "DialogueInfo" and current:
                    latest[(current, str(rec.get("id") or ""))] = (name, rec)
        for record_type, topic, key, rec in made:
            low = topic.lower() if record_type == "DialogueInfo" else key.lower()
            if record_type == "Dialogue":
                self.spelled.setdefault(low, key)
                self.kinds[low] = str(rec.get("dialogue_type") or self.kinds.get(low, ""))
            elif record_type == "DialogueInfo":
                self.spelled.setdefault(low, topic)
                defs.setdefault(low, []).append(
                    Response(key, str(rec.get("prev_id") or ""), "(this patch)")
                )
                latest[(low, key)] = ("(this patch)", rec)
        edits = {k.lower(): v for k, v in (edits or {}).items()}
        self.lines: dict[str, list[Line]] = {}
        for low, found in defs.items():
            order = topic_order(found)
            lost = set(orphans(order))
            out: list[Line] = []
            for placed in order:
                _winner, raw = latest.get((low, placed.key), ("", {}))
                rec = dict(raw)
                for path, value in (edits.get(placed.key.lower()) or {}).items():
                    set_at(rec, path, value)
                out.append(
                    Line(
                        self.spelled.get(low, low),
                        placed.key,
                        rec,
                        list(placed.plugins),
                        placed.key in lost,
                    )
                )
            self.lines[low] = out
        for low in self.spelled:
            self.lines.setdefault(low, [])
        self._phrases: dict[str, str] | None = None
        self._starts: dict[str, list[int]] = {}
        self._flags: dict[tuple[str, str], dict[str, Any]] | None = None

    # -- reading ------------------------------------------------------------------------

    def kind(self, topic: str) -> str:
        """A topic's kind (``Topic``, ``Greeting``, ``Journal``, ...)."""
        return self.kinds.get(topic.lower(), "")

    def topic_names(self, kind: str = TOPIC) -> list[str]:
        """Every topic of a kind, as spelled, by name."""
        return sorted((self.spelled[k] for k, v in self.kinds.items() if v == kind), key=str.lower)

    def _topic_words(self) -> dict[str, str]:
        """Every ``Topic`` by its lower-cased name (built once), and the most words one has."""
        if self._phrases is None:
            self._phrases = {}
            starts: dict[str, set[int]] = {}
            for name in self.topic_names(TOPIC):
                words = _WORD.findall(name.lower())
                if words:
                    self._phrases[" ".join(words)] = name
                    starts.setdefault(words[0], set()).add(len(words))
            # By first word: how many words the topics starting with it have, longest first.
            self._starts = {w: sorted(ns, reverse=True) for w, ns in starts.items()}
        return self._phrases

    def _matches(self, text: str) -> list[tuple[int, int, str]]:
        """Where a text names topics, as the game hyperlinks them.

        A word scan with a lookup per run of words rather than one huge pattern: a load
        order has thousands of topics and a hundred thousand responses.

        Returns:
            ``(start, end, topic)``, the longest name at each place first, never
            overlapping.
        """
        phrases = self._topic_words()
        if not phrases:
            return []
        spans = [(m.start(), m.end()) for m in _WORD.finditer(text)]
        low = text.lower()
        words = [low[a:b] for a, b in spans]
        starts = self._starts
        out: list[tuple[int, int, str]] = []
        i = 0
        while i < len(words):
            hit = None
            for n in starts.get(words[i], ()):
                if i + n > len(words):
                    continue
                name = phrases.get(words[i] if n == 1 else " ".join(words[i : i + n]))
                if name is not None:
                    hit = (spans[i][0], spans[i + n - 1][1], name, n)
                    break
            if hit:
                out.append(hit[:3])
                i += hit[3]
            else:
                i += 1
        return out

    def segments(self, text: str, allowed: set[str] | None = None) -> list[dict[str, str]]:
        """A line's text cut into plain words, topic links and macros, as the game shows it.

        Args:
            text: The text.
            allowed: Only these topics (lower) are links (what a speaker answers);
                None for every topic.

        Returns:
            ``[{"t": text}, {"t": text, "topic": id}, {"t": "%Name", "macro": "Name"}]``.
        """
        out: list[dict[str, str]] = []
        pos = 0
        for start, end, name in self._matches(text):
            if allowed is not None and name.lower() not in allowed:
                continue
            if start > pos:
                out.extend(self._macros(text[pos:start]))
            out.append({"t": text[start:end], "topic": name})
            pos = end
        if pos < len(text):
            out.extend(self._macros(text[pos:]))
        return out

    @staticmethod
    def _macros(text: str) -> list[dict[str, str]]:
        """Plain text with its ``%Macro`` names marked."""
        out: list[dict[str, str]] = []
        pos = 0
        for m in _MACRO.finditer(text):
            if m.start() > pos:
                out.append({"t": text[pos : m.start()]})
            out.append({"t": m.group(0), "macro": m.group(0)[1:]})
            pos = m.end()
        if pos < len(text):
            out.append({"t": text[pos:]})
        return out

    def links_of(self, line: Line) -> list[str]:
        """The topics a response's text hyperlinks, once each, as spelled."""
        return list(dict.fromkeys(name for _s, _e, name in self._matches(line.text)))

    # -- the flow of one topic ----------------------------------------------------------------

    def _line_view(self, line: Line, index: int) -> dict[str, Any]:
        """One response, for the flow."""
        eff = line.effects
        rec = line.rec
        return {
            "id": line.key,
            "n": index + 1,
            "speaker": speaker_line(rec),
            "conditions": condition_lines(rec),
            "gate": choice_gate(rec),
            "text": line.text,
            "segments": self.segments(line.text),
            "script": line.script,
            "choices": [{"label": label, "n": n} for label, n in eff.choices],
            "addtopics": eff.addtopics,
            "journal": [{"quest": q, "index": i} for q, i in eff.journal],
            "sets": [{"name": n, "target": t} for n, t in eff.sets],
            "items": [{"item": i, "verb": v} for i, v in eff.items],
            "goodbye": eff.goodbye,
            "other": eff.other,
            "links": self.links_of(line),
            "plugins": line.plugins,
            "orphan": line.orphan,
            "quest": str(rec.get("quest_state") or ""),
            "disposition": (
                (rec.get("data") or {}).get("disposition")
                if isinstance(rec.get("data"), dict)
                else None
            ),
        }

    def flow(self, topic: str) -> dict[str, Any]:
        """One topic as a flow: its responses in engine order, and its choice tree.

        The responses that answer a ``Choice`` (``Function Choice = n``) are listed with
        the rest, and each option of a ``Choice`` names the responses whose gate is its
        number - the tree inside the topic, which the page draws nested.

        Args:
            topic: The topic (any case).

        Returns:
            ``{id, type, responses, into, outof}``: ``into`` the topics that lead here
            (an ``AddTopic`` or a link in their text), ``outof`` where this one leads.

        Raises:
            ValueError: For a topic no plugin has.
        """
        low = topic.strip().lower()
        if low not in self.lines:
            raise ValueError(f"no plugin of this load order has the topic {topic}")
        rows = [self._line_view(line, i) for i, line in enumerate(self.lines[low])]
        out_edges, in_edges = self._edges_of(low)
        return {
            "id": self.spelled.get(low, topic),
            "type": self.kind(low),
            "responses": rows,
            "outof": out_edges,
            "into": in_edges,
        }

    # -- the map ------------------------------------------------------------------------------

    def _out(self, low: str) -> list[tuple[str, str, str]]:
        """Where a topic leads.

        Returns:
            ``(kind, target, via)``: ``addtopic``/``link`` to a topic (lower), ``quest``
            to a journal, ``sets`` to a variable; ``via`` the response.
        """
        seen: set[tuple[str, str]] = set()
        out: list[tuple[str, str, str]] = []

        def add(kind: str, target: str, via: str) -> None:
            """Add a link once."""
            if (kind, target) not in seen:
                seen.add((kind, target))
                out.append((kind, target, via))

        for line in self.lines.get(low, []):
            for t in line.effects.addtopics:
                add("addtopic", t.lower(), line.key)
            for t in self.links_of(line):
                if t.lower() != low:
                    add("link", t.lower(), line.key)
            for q, _i in line.effects.journal:
                add("quest", q.lower(), line.key)
            for name, target in line.effects.sets:
                if not target:
                    add("sets", name.lower(), line.key)
        return out

    def _edges_of(self, low: str) -> tuple[list[dict[str, str]], list[dict[str, str]]]:
        """A topic's edges out and in, for the flow's header."""
        outs = [
            {"kind": k, "to": self.spelled.get(t, t), "via": v}
            for k, t, v in self._out(low)
            if k in ("addtopic", "link")
        ]
        ins = [
            {"kind": k, "from": self.spelled.get(other, other), "via": v}
            for other, k, v in self._incoming().get(low, [])
        ]
        return outs, ins

    def _incoming(self) -> dict[str, list[tuple[str, str, str]]]:
        """Every ``addtopic``/``link`` into each topic, read once and kept.

        Returns:
            Topic (lower) -> ``(from topic, kind, via)``.
        """
        inc = getattr(self, "_in_cache", None)
        if inc is None:
            inc = {}
            for other in self.lines:
                for k, t, v in self._out_cached(other):
                    if k in ("addtopic", "link") and t != other:
                        inc.setdefault(t, []).append((other, k, v))
            self._in_cache = inc
        return inc

    _out_cache: dict[str, list[tuple[str, str, str]]]
    _in_cache: dict[str, list[tuple[str, str, str]]]

    def _out_cached(self, low: str) -> list[tuple[str, str, str]]:
        """A topic's outgoing links, worked out once per index."""
        cache = getattr(self, "_out_cache", None)
        if cache is None:
            cache = self._out_cache = {}
        hit = cache.get(low)
        if hit is None:
            hit = cache[low] = self._out(low)
        return hit

    def neighbourhood(self, topic: str, depth: int = 1) -> dict[str, Any]:
        """A topic and the topics, quests and variables a step or two from it.

        Args:
            topic: The centre (any case).
            depth: How many steps out (1 or 2).

        Returns:
            ``{centre, nodes: [{id, kind, label, depth, responses}], edges: [{from, to,
            kind}], capped}`` - a node's ``kind`` is a topic kind, ``quest`` or ``var``.

        Raises:
            ValueError: For a topic no plugin has.
        """
        centre = topic.strip().lower()
        if centre not in self.lines:
            raise ValueError(f"no plugin of this load order has the topic {topic}")
        depth = 2 if depth >= 2 else 1
        nodes: dict[str, dict[str, Any]] = {}
        edges: list[dict[str, str]] = []
        seen_edges: set[tuple[str, str, str]] = set()
        capped = False

        def node(nid: str, kind: str, label: str, d: int) -> bool:
            """Add a node unless the map is full; whether it is there."""
            nonlocal capped
            if nid in nodes:
                return True
            if len(nodes) >= MAP_CAP:
                capped = True
                return False
            nodes[nid] = {
                "id": nid,
                "kind": kind,
                "label": label,
                "depth": d,
                "responses": len(self.lines.get(nid[2:], [])) if nid.startswith("t:") else 0,
            }
            return True

        def edge(a: str, b: str, kind: str) -> None:
            """Add an edge once, between nodes on the map."""
            if (a, b, kind) not in seen_edges and a in nodes and b in nodes:
                seen_edges.add((a, b, kind))
                edges.append({"from": a, "to": b, "kind": kind})

        node("t:" + centre, self.kind(centre) or TOPIC, self.spelled.get(centre, centre), 0)
        frontier = [centre]
        for d in range(1, depth + 1):
            nxt: list[str] = []
            for low in frontier:
                me = "t:" + low
                for kind, target, _via in self._out_cached(low):
                    if kind in ("addtopic", "link"):
                        nid = "t:" + target
                        if node(
                            nid, self.kind(target) or TOPIC, self.spelled.get(target, target), d
                        ):
                            edge(me, nid, kind)
                            nxt.append(target)
                    elif kind == "quest":
                        if node("q:" + target, "quest", self.spelled.get(target, target), d):
                            edge(me, "q:" + target, "quest")
                    elif kind == "sets":
                        if node("v:" + target, "var", target, d):
                            edge(me, "v:" + target, "sets")
                # And what leads here.
                for other, kind, _via in self._incoming().get(low, []):
                    nid = "t:" + other
                    if node(nid, self.kind(other) or TOPIC, self.spelled.get(other, other), d):
                        edge(nid, me, kind)
                        nxt.append(other)
            frontier = nxt
        # Variables set here and tested elsewhere: the flag links between trees.
        flags = self._flag_index()
        for nid in [n for n in nodes if n.startswith("v:")]:
            for kind_name, entry in flags.items():
                if kind_name[1] != nid[2:]:
                    continue
                for r in entry["readers"]:
                    tid = "t:" + r["topic"].lower()
                    if tid in nodes:
                        edge(nid, tid, "reads")
        return {
            "centre": "t:" + centre,
            "nodes": list(nodes.values()),
            "edges": edges,
            "capped": capped,
        }

    # -- the blackboard -----------------------------------------------------------------------

    def _flag_index(self) -> dict[tuple[str, str], dict[str, Any]]:
        """``(kind, name lower) -> {writers, readers}`` over every response (once)."""
        if self._flags is not None:
            return self._flags
        flags: dict[tuple[str, str], dict[str, Any]] = {}

        def put(kind: str, name: str, side: str, line: Line, how: str) -> None:
            """Note one line writing or reading a flag."""
            entry = flags.setdefault(
                (kind, name.lower()), {"name": name, "writers": [], "readers": []}
            )
            entry[side].append(
                {"topic": line.topic, "id": line.key, "text": line.text[:140], "how": how}
            )

        for lines in self.lines.values():
            for line in lines:
                eff = line.effects
                for name, target in eff.sets:
                    put(
                        "local" if target else "variable",
                        name,
                        "writers",
                        line,
                        f"set {target + '->' if target else ''}{name}",
                    )
                for quest, index in eff.journal:
                    put("journal", quest, "writers", line, f"journal {quest} {index}")
                for item, verb in eff.items:
                    put("item", item, "writers", line, f"{verb} {item}")
                for kind, name, clause in reads_of(line.rec):
                    put(
                        "variable" if kind in ("global", "local") else kind,
                        name,
                        "readers",
                        line,
                        clause,
                    )
        self._flags = flags
        return flags

    def flags(self, name: str = "", kind: str = "") -> dict[str, Any]:
        """The blackboard: what the dialogue writes and tests.

        Args:
            name: One flag's name, for its writers and readers in full; empty for the
                list.
            kind: Its kind (``variable``, ``journal``, ``item``, ``dead``), when ``name``
                is given and could be more than one.

        Returns:
            ``{flags: [{kind, name, writers, readers}]}`` (counts), or ``{kind, name,
            writers, readers}`` (the lists) for one.
        """
        index = self._flag_index()
        if not name:
            rows = [
                {
                    "kind": k,
                    "name": v["name"],
                    "writers": len(v["writers"]),
                    "readers": len(v["readers"]),
                }
                for (k, _low), v in index.items()
            ]
            rows.sort(key=lambda r: (r["kind"], str(r["name"]).lower()))
            return {"flags": rows}
        low = name.strip().lower()
        hits = [(k, v) for (k, n), v in index.items() if n == low and (not kind or k == kind)]
        if not hits:
            raise ValueError(f"nothing in the dialogue writes or tests {name}")
        k, v = hits[0]
        return {"kind": k, "name": v["name"], "writers": v["writers"], "readers": v["readers"]}

    # -- the rehearsal ------------------------------------------------------------------------

    def _speaker_ok(
        self,
        rec: Mapping[str, Any],
        npc: Mapping[str, Any],
        cell: str,
        disposition: int,
        choice: int,
    ) -> tuple[bool, list[str]]:
        """Whether a response could be said by this NPC, and what could not be checked."""
        low = lambda v: str(v or "").strip().lower()  # noqa: E731 - local shorthand
        for k, mine in (
            ("speaker_id", npc.get("id")),
            ("speaker_race", npc.get("race")),
            ("speaker_class", npc.get("class")),
            ("speaker_faction", npc.get("faction")),
        ):
            if rec.get(k) and low(rec.get(k)) != low(mine):
                return False, []
        unchecked: list[str] = []
        if rec.get("speaker_cell"):
            if cell:
                if not low(cell).startswith(low(rec.get("speaker_cell"))):
                    return False, []
            else:
                unchecked.append(f"NPC is in cell {rec.get('speaker_cell')}")
        raw = rec.get("data")
        data: Mapping[str, Any] = raw if isinstance(raw, dict) else {}
        sex = data.get("speaker_sex")
        if sex in ("Male", "Female") and npc.get("sex") and sex != npc.get("sex"):
            return False, []
        disp = data.get("disposition")
        if isinstance(disp, int) and disp > disposition:
            return False, []
        if rec.get("player_faction"):
            unchecked.append(f"player faction {rec.get('player_faction')}")
        gate = choice_gate(rec)
        if gate is not None and gate != choice:
            return False, []
        for flt in rec.get("filters") or []:
            if not isinstance(flt, dict):
                continue
            ftype = flt.get("filter_type")
            clause = describe_filter(flt)
            if ftype in (None, "None") or not clause:
                continue
            if ftype == "Function" and flt.get("function") == "Choice":
                continue
            ident = low(flt.get("id"))
            ours = {
                "NotId": low(npc.get("id")),
                "NotFaction": low(npc.get("faction")),
                "NotClass": low(npc.get("class")),
                "NotRace": low(npc.get("race")),
            }
            if ftype in ours:
                # "NotX y = 0": the speaker is y. Its usual boolean forms, checked.
                is_it = ours[ftype] == ident
                if clause.startswith("NPC is not") and is_it:
                    return False, []
                if (
                    clause.startswith("NPC is ")
                    and not clause.startswith("NPC is not")
                    and not is_it
                ):
                    return False, []
                continue
            unchecked.append(clause)
        return True, unchecked

    def pick(
        self,
        topic: str,
        npc: Mapping[str, Any],
        *,
        cell: str = "",
        disposition: int = 50,
        choice: int = 0,
        strict: bool = False,
    ) -> tuple[Line, int, list[str]] | None:
        """The response the engine would give: the first whose conditions hold.

        Args:
            topic: The topic (lower).
            npc: ``{id, name, race, class, faction, sex}``.
            cell: Where the conversation is (empty: not checked).
            disposition: The NPC's disposition towards the player.
            choice: The ``Choice`` just made (0: none).
            strict: Skip responses with conditions only the running game knows.

        Returns:
            ``(line, position, unchecked conditions)``, or None.
        """
        lines = self.lines.get(topic, [])
        # A choice just made: the responses answering it first (authors put them above
        # the question, which would otherwise be asked again).
        order = list(enumerate(lines))
        if choice:
            order = [x for x in order if choice_gate(x[1].rec) == choice] + [
                x for x in order if choice_gate(x[1].rec) != choice
            ]
        for i, line in order:
            if "DELETED" in str(line.rec.get("flags") or "").upper():
                continue
            ok, unchecked = self._speaker_ok(line.rec, npc, cell, disposition, choice)
            if ok and not (strict and unchecked):
                return line, i, unchecked
        return None

    def play(
        self,
        npc: Mapping[str, Any],
        topic: str = "",
        *,
        choice: int = 0,
        cell: str = "",
        disposition: int = 50,
        strict: bool = False,
    ) -> dict[str, Any]:
        """The game's dialogue window, rehearsed: a greeting or a topic's answer.

        Args:
            npc: ``{id, name, race, class, faction, sex}`` - who is spoken to.
            topic: The topic asked (empty: the greeting).
            choice: The ``Choice`` made, re-asking ``topic`` with it.
            cell: Where (empty: cell conditions are not checked).
            disposition: The NPC's disposition (``disposition is at least`` tests).
            strict: Skip responses whose other conditions cannot be checked here.

        Returns:
            ``{topics, line}``: the topics this NPC answers (by name), and the line said
            - ``{topic, id, n, segments, choices, goodbye, conditions, unchecked,
            effects}`` - or None for a topic they have nothing on.
        """
        kw = {"cell": cell, "disposition": disposition, "strict": strict}
        answers: list[str] = []
        allowed: set[str] = set()
        for name in self.topic_names(TOPIC):
            low = name.lower()
            if self.pick(low, npc, **kw) is not None:  # type: ignore[arg-type]
                answers.append(name)
                allowed.add(low)
        got: tuple[Line, int, list[str]] | None = None
        asked = topic.strip().lower()
        if asked:
            got = self.pick(asked, npc, choice=choice, **kw)  # type: ignore[arg-type]
        else:
            for name in sorted(
                (n for n in self.topic_names(GREETING)),
                key=lambda s: [int(t) if t.isdigit() else t.lower() for t in re.split(r"(\d+)", s)],
            ):
                got = self.pick(name.lower(), npc, **kw)  # type: ignore[arg-type]
                if got is not None:
                    break
        line_out: dict[str, Any] | None = None
        if got is not None:
            line, pos, unchecked = got
            eff = line.effects
            line_out = {
                "topic": line.topic,
                "id": line.key,
                "n": pos + 1,
                "segments": self.segments(line.text, allowed),
                "choices": [{"label": label, "n": n} for label, n in eff.choices],
                "goodbye": eff.goodbye,
                "conditions": condition_lines(line.rec),
                "unchecked": unchecked,
                "addtopics": eff.addtopics,
                "journal": [{"quest": q, "index": i} for q, i in eff.journal],
            }
        return {"topics": answers, "line": line_out}


def npc_of(record: Mapping[str, Any]) -> dict[str, Any]:
    """An NPC record as :meth:`DialogueIndex.play` reads a speaker."""
    flags = str(record.get("npc_flags") or "").upper()
    return {
        "id": str(record.get("id") or ""),
        "name": str(record.get("name") or record.get("id") or ""),
        "race": str(record.get("race") or ""),
        "class": str(record.get("class") or ""),
        "faction": str(record.get("faction") or ""),
        "sex": "Female" if "FEMALE" in flags else "Male",
    }


__all__ = [
    "DialogueIndex",
    "Effects",
    "Line",
    "choice_gate",
    "npc_of",
    "reads_of",
    "script_effects",
    "speaker_line",
]
