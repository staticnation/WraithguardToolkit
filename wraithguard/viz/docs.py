"""Render the project's own Markdown documentation as a readable HTML page.

The README and QUICKSTART ship beside the app but are only useful if someone
finds them, so the GUI offers them directly. Handing the ``.md`` file to the
operating system was rejected: what opens is whatever happens to be associated
with ``.md`` on that machine -- Notepad with no wrapping, an IDE, or nothing at
all -- which is not a reading experience anyone would choose.

So the Markdown is rendered here, to the same kind of self-contained page the
maps produce: no CDN, no external stylesheet, no JavaScript at all. That matters
for the same reason it matters there (the tool runs offline and ships frozen)
and for one more: the help has to work when everything else is broken.

This is a **deliberately small** Markdown subset -- exactly what this project's
own documents use, verified against them: ATX headings, fenced code, tables,
nested lists, block quotes, rules, links, bold, italic, strikethrough and inline
code. It is not a CommonMark implementation and does not try to be. Anything it
does not recognise is emitted as escaped text rather than guessed at, so an
unsupported construct is visibly plain rather than silently mangled.

All input is escaped. The documents are ours and trusted, but the renderer is
general and the cost of escaping is nil, so raw HTML passthrough is not offered.
"""

from __future__ import annotations

import html
import re
from datetime import datetime
from typing import TYPE_CHECKING, Final

from wraithguard import _

if TYPE_CHECKING:
    from collections.abc import Iterator, Sequence

#: Reading-oriented styling: a measured column, generous line height, and code
#: that stands away from prose. Shares the palette with the other pages so a
#: help page opened from the app does not look like a different application.
DOCS_CSS: Final[str] = """
:root{--bg:#1e2127;--panel:#252a32;--ink:#d7dae0;--dim:#8b93a1;--line:#333945;
--accent:#7cc5ff;--mark:#ffd24a;--code:#1a1d23}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);
font:15px/1.65 "Segoe UI",system-ui,sans-serif}
header{position:sticky;top:0;z-index:5;background:var(--panel);
border-bottom:1px solid var(--line);padding:12px 22px}
header h1{margin:0;font-size:17px;font-weight:600}
header .sub{color:var(--dim);font-size:12.5px;margin-top:3px}
.wrap{display:flex;align-items:flex-start;gap:26px;
max-width:1180px;margin:0 auto;padding:22px}
nav{position:sticky;top:74px;flex:0 0 250px;max-height:calc(100vh - 96px);
overflow:auto;border:1px solid var(--line);border-radius:6px;
background:var(--panel);padding:12px 14px;font-size:13px}
nav b{display:block;color:var(--dim);font-weight:600;text-transform:uppercase;
letter-spacing:.05em;font-size:11px;margin-bottom:8px}
nav a{display:block;color:var(--ink);text-decoration:none;padding:3px 0;
border-left:2px solid transparent;padding-left:8px}
nav a:hover{color:var(--accent);border-left-color:var(--accent)}
nav a.sub2{padding-left:20px;color:var(--dim);font-size:12.5px}
main{flex:1 1 auto;min-width:0}
h1,h2,h3,h4,h5,h6{line-height:1.3;margin:1.6em 0 .6em;font-weight:600}
h1{font-size:25px;margin-top:0}
h2{font-size:20px;border-bottom:1px solid var(--line);padding-bottom:.3em}
h3{font-size:16.5px}
h4,h5,h6{font-size:15px;color:var(--dim)}
p{margin:.75em 0}
ul,ol{margin:.6em 0;padding-left:1.5em}
li{margin:.25em 0}
li>ul,li>ol{margin:.25em 0}
a{color:var(--accent)}
code{font-family:Consolas,"DejaVu Sans Mono",monospace;font-size:.88em;
background:var(--code);border:1px solid var(--line);border-radius:3px;
padding:.1em .35em}
pre{background:var(--code);border:1px solid var(--line);border-radius:6px;
padding:12px 14px;overflow:auto;margin:1em 0}
pre code{background:none;border:0;padding:0;font-size:12.5px;line-height:1.5}
blockquote{margin:1em 0;padding:.4em 1em;border-left:3px solid var(--mark);
background:var(--panel);color:var(--ink)}
blockquote p{margin:.35em 0}
hr{border:0;border-top:1px solid var(--line);margin:1.8em 0}
table{border-collapse:collapse;width:100%;margin:1em 0;font-size:13.5px;
display:block;overflow-x:auto}
th,td{border:1px solid var(--line);padding:6px 10px;text-align:left;
vertical-align:top}
th{background:var(--panel);color:var(--dim);font-weight:600}
del{color:var(--dim)}
strong{color:#fff}
@media (max-width:900px){.wrap{flex-direction:column}nav{position:static;
flex:1 1 auto;width:100%;max-height:none}}
"""

#: ``# Heading``. Trailing hashes are stripped, as every Markdown dialect does.
_HEADING = re.compile(r"^(?P<level>#{1,6})\s+(?P<text>.*?)\s*#*\s*$")

#: ```` ```lang ```` -- the language is captured but only recorded, never used to
#: highlight: syntax highlighting means shipping a grammar, and these pages have
#: no JavaScript.
_FENCE = re.compile(r"^\s*(?P<ticks>`{3,}|~{3,})\s*(?P<lang>[\w+-]*)\s*$")

#: A horizontal rule: three or more of the same mark, nothing else.
_RULE = re.compile(r"^\s{0,3}([-*_])(?:\s*\1){2,}\s*$")

#: ``- item`` / ``* item`` / ``+ item``, with leading indent captured for nesting.
_BULLET = re.compile(r"^(?P<indent>\s*)[-*+]\s+(?P<text>.*)$")

#: ``1. item``. The number itself is discarded -- HTML renumbers, and a document
#: that restarts at 1. mid-list is a typo rather than an instruction.
_NUMBER = re.compile(r"^(?P<indent>\s*)\d+[.)]\s+(?P<text>.*)$")

#: ``| a | b |``: any line that starts and ends with a pipe.
_TABLE_ROW = re.compile(r"^\s*\|.*\|\s*$")

#: ``|---|:--:|`` -- the row that turns the line above it into a header.
_TABLE_SEP = re.compile(r"^\s*\|(?:\s*:?-{2,}:?\s*\|)+\s*$")

#: Inline constructs, tried in this order. Code spans come first so nothing
#: inside them is interpreted -- ``**`` inside backticks is two asterisks.
#:
#: The two emphasis alternatives are spelled out separately rather than sharing
#: a back-reference because the delimiter needs lookarounds on both ends: in
#: ``*Export writes nothing while **Dry run** is checked*`` a shared ``[*_]``
#: closes the italic on the first asterisk of ``**Dry``, which leaves stray
#: asterisks in the output. Requiring that neither end of a single-mark run
#: touch another mark fixes it, and nesting still works because the captured
#: body is rendered recursively.
_INLINE = re.compile(
    r"(?P<code>`+)(?P<codetext>.+?)(?P=code)"
    r"|!\[(?P<alt>[^\]]*)\]\((?P<src>[^)\s]+)\)"
    r"|\[(?P<label>[^\]]+)\]\((?P<href>[^)\s]+)\)"
    r"|(?P<strike>~~)(?P<striketext>.+?)~~"
    r"|(?P<bold>\*\*|__)(?P<boldtext>.+?)(?P=bold)"
    r"|(?<!\*)\*(?!\*)(?P<emstar>[^\s*](?:.*?[^\s*])?)\*(?!\*)"
    r"|(?<![\w_])_(?!_)(?P<emund>[^\s_](?:.*?[^\s_])?)_(?!\w)",
    re.DOTALL,
)

#: Schemes a link may use. Anything else -- ``javascript:`` most of all -- is
#: rendered as plain text. These documents are ours, but a renderer that emits
#: whatever URL it is handed is a renderer that cannot safely be pointed at
#: anything else later.
_SAFE_SCHEMES: Final[tuple[str, ...]] = ("http://", "https://", "mailto:", "#")


def _slug(text: str, used: set[str]) -> str:
    """Build a unique, URL-safe anchor id for a heading.

    Args:
        text: The heading's plain text.
        used: Ids already issued for this document; mutated to record this one.

    Returns:
        The id, suffixed with ``-2``, ``-3`` and so on if the heading text
        repeats -- which it does (several sections have a "Notes" heading), and
        duplicate ids would make the contents list jump to the wrong place.
    """
    base = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-") or "section"
    slug = base
    counter = 2
    while slug in used:
        slug = f"{base}-{counter}"
        counter += 1
    used.add(slug)
    return slug


def _safe_href(url: str) -> str | None:
    """Vet a link target.

    Args:
        url: The raw href from the document.

    Returns:
        The escaped href, or ``None`` if the scheme is not one of
        :data:`_SAFE_SCHEMES` and the target is not a plain relative path.
    """
    lowered = url.strip().lower()
    if lowered.startswith(_SAFE_SCHEMES):
        return html.escape(url.strip(), quote=True)
    if ":" in lowered.split("/")[0]:
        return None
    return html.escape(url.strip(), quote=True)


def inline(text: str) -> str:
    """Render one line's inline markup.

    Everything outside a recognised construct is HTML-escaped, so an unmatched
    ``<`` is a less-than sign rather than the start of a tag.

    Args:
        text: One line of Markdown, without its block marker.

    Returns:
        HTML for that line.
    """
    out: list[str] = []
    pos = 0
    for match in _INLINE.finditer(text):
        out.append(html.escape(text[pos : match.start()]))
        pos = match.end()
        if match.group("code"):
            out.append(f"<code>{html.escape(match.group('codetext'))}</code>")
        elif match.group("src") is not None:
            href = _safe_href(match.group("src"))
            alt = html.escape(match.group("alt"))
            out.append(f'<img src="{href}" alt="{alt}">' if href else alt)
        elif match.group("label") is not None:
            href = _safe_href(match.group("href"))
            label = inline(match.group("label"))
            out.append(f'<a href="{href}">{label}</a>' if href else label)
        elif match.group("strike"):
            out.append(f"<del>{inline(match.group('striketext'))}</del>")
        elif match.group("bold"):
            out.append(f"<strong>{inline(match.group('boldtext'))}</strong>")
        else:
            emphasised = match.group("emstar")
            if emphasised is None:
                emphasised = match.group("emund")
            out.append(f"<em>{inline(emphasised)}</em>")
    out.append(html.escape(text[pos:]))
    return "".join(out)


class _Renderer:
    """Walks a document's lines once, emitting HTML blocks.

    A class only because block parsing is a small state machine over a cursor:
    the alternative is threading an index through eight free functions.
    """

    def __init__(self, lines: Sequence[str]) -> None:
        """Prepare to render.

        Args:
            lines: The document, split on newlines.
        """
        self.lines = lines
        self.index = 0
        self.slugs: set[str] = set()
        #: ``(level, text, id)`` per heading, for the contents list.
        self.headings: list[tuple[int, str, str]] = []

    def render(self) -> str:
        """Render every block in the document.

        Returns:
            The document body as HTML.
        """
        return "".join(self._blocks())

    def _blocks(self) -> Iterator[str]:
        """Yield each block's HTML in document order.

        Yields:
            One block of HTML at a time.
        """
        while self.index < len(self.lines):
            line = self.lines[self.index]
            if not line.strip():
                self.index += 1
                continue
            fence = _FENCE.match(line)
            if fence:
                yield self._code_block(fence.group("ticks"))
            elif _RULE.match(line):
                self.index += 1
                yield "<hr>"
            elif _HEADING.match(line):
                yield self._heading()
            elif line.lstrip().startswith(">"):
                yield self._quote()
            elif _TABLE_ROW.match(line) and self._is_table_start():
                yield self._table()
            elif _BULLET.match(line) or _NUMBER.match(line):
                yield self._list(indent=0)
            else:
                yield self._paragraph()

    def _heading(self) -> str:
        """Render an ATX heading and record it for the contents list.

        Returns:
            The heading markup, with an anchor id.
        """
        match = _HEADING.match(self.lines[self.index])
        assert match is not None  # noqa: S101 - guarded by the caller's dispatch
        self.index += 1
        level = len(match.group("level"))
        text = match.group("text")
        ident = _slug(re.sub(r"[`*_~]", "", text), self.slugs)
        self.headings.append((level, re.sub(r"[`*_~]", "", text), ident))
        return f'<h{level} id="{ident}">{inline(text)}</h{level}>'

    def _code_block(self, ticks: str) -> str:
        """Render a fenced code block verbatim.

        An unterminated fence runs to the end of the document rather than
        raising: a help page that renders most of a file beats one that refuses
        to render at all.

        Args:
            ticks: The opening fence, whose length the closing fence must match.

        Returns:
            The ``<pre>`` markup.
        """
        self.index += 1
        body: list[str] = []
        while self.index < len(self.lines):
            line = self.lines[self.index]
            closing = _FENCE.match(line)
            if closing and closing.group("ticks").startswith(ticks):
                self.index += 1
                break
            body.append(line)
            self.index += 1
        return f"<pre><code>{html.escape(chr(10).join(body))}</code></pre>"

    def _quote(self) -> str:
        """Render a block quote, rendering its contents as Markdown too.

        Returns:
            The ``<blockquote>`` markup.
        """
        body: list[str] = []
        while self.index < len(self.lines) and self.lines[self.index].lstrip().startswith(">"):
            body.append(self.lines[self.index].lstrip()[1:].lstrip())
            self.index += 1
        inner = _Renderer(body)
        rendered = inner.render()
        return f"<blockquote>{rendered}</blockquote>"

    def _is_table_start(self) -> bool:
        """Report whether the row at the cursor begins a real table.

        A lone pipe-containing line is prose, not a table; what makes it a table
        is the ``|---|`` separator underneath.

        Returns:
            ``True`` if the next line is a separator row.
        """
        return self.index + 1 < len(self.lines) and bool(
            _TABLE_SEP.match(self.lines[self.index + 1])
        )

    def _table(self) -> str:
        """Render a pipe table.

        Returns:
            The table markup.
        """
        header = _split_row(self.lines[self.index])
        self.index += 2  # header + separator
        rows: list[list[str]] = []
        while self.index < len(self.lines) and _TABLE_ROW.match(self.lines[self.index]):
            rows.append(_split_row(self.lines[self.index]))
            self.index += 1
        head = "".join(f"<th>{inline(cell)}</th>" for cell in header)
        body = "".join(
            "<tr>" + "".join(f"<td>{inline(cell)}</td>" for cell in row) + "</tr>" for row in rows
        )
        return f"<table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>"

    def _list(self, indent: int) -> str:
        """Render a list, recursing for indented sub-lists.

        Args:
            indent: The column this list's markers start at. A line indented
                further opens a nested list; a line indented less ends this one.

        Returns:
            The ``<ul>`` or ``<ol>`` markup.
        """
        ordered = _NUMBER.match(self.lines[self.index]) is not None
        items: list[str] = []
        current: list[str] = []
        while self.index < len(self.lines):
            line = self.lines[self.index]
            if not line.strip():
                self.index += 1
                if self.index < len(self.lines) and not self._continues_list(indent):
                    break
                continue
            match = _NUMBER.match(line) if ordered else _BULLET.match(line)
            other = _BULLET.match(line) if ordered else _NUMBER.match(line)
            marker = match or other
            if marker is None:
                if len(line) - len(line.lstrip()) > indent and current:
                    current.append(line.strip())
                    self.index += 1
                    continue
                break
            width = len(marker.group("indent"))
            if width > indent:
                nested = self._list(indent=width)
                if current:
                    current.append(nested)
                else:
                    items.append(nested)
                continue
            if width < indent:
                break
            if match is None:
                break
            if current:
                items.append(_item(current))
            current = [marker.group("text")]
            self.index += 1
        if current:
            items.append(_item(current))
        tag = "ol" if ordered else "ul"
        return f"<{tag}>{''.join(items)}</{tag}>"

    def _continues_list(self, indent: int) -> bool:
        """Report whether the line at the cursor belongs to a list still open.

        Args:
            indent: The open list's marker column.

        Returns:
            ``True`` if the cursor is on a list item at or beyond that column.
        """
        marker = _BULLET.match(self.lines[self.index]) or _NUMBER.match(self.lines[self.index])
        return marker is not None and len(marker.group("indent")) >= indent

    def _paragraph(self) -> str:
        """Render consecutive plain lines as one paragraph.

        Returns:
            The ``<p>`` markup.
        """
        body: list[str] = []
        while self.index < len(self.lines):
            line = self.lines[self.index]
            if not line.strip() or _HEADING.match(line) or _RULE.match(line):
                break
            if _FENCE.match(line) or line.lstrip().startswith(">"):
                break
            if _BULLET.match(line) or _NUMBER.match(line):
                break
            if _TABLE_ROW.match(line) and self._is_table_start():
                break
            body.append(line.strip())
            self.index += 1
        return f"<p>{inline(' '.join(body))}</p>"


def _item(parts: Sequence[str]) -> str:
    """Assemble one list item from its text and any nested list.

    Args:
        parts: The item's own lines, plus already-rendered nested lists.

    Returns:
        The ``<li>`` markup.
    """
    text = " ".join(part for part in parts if not part.startswith("<"))
    nested = "".join(part for part in parts if part.startswith("<"))
    return f"<li>{inline(text)}{nested}</li>"


def _split_row(line: str) -> list[str]:
    r"""Split a pipe-table row into its cells.

    Args:
        line: The raw row, with its leading and trailing pipes.

    Returns:
        The cell texts, stripped. Escaped pipes (``\\|``) stay inside a cell.
    """
    stripped = line.strip().strip("|")
    cells = re.split(r"(?<!\\)\|", stripped)
    return [cell.strip().replace("\\|", "|") for cell in cells]


def _contents(headings: Sequence[tuple[int, str, str]]) -> str:
    """Build the sidebar contents list.

    Args:
        headings: ``(level, text, id)`` in document order.

    Returns:
        The ``<nav>`` markup, or an empty string when a document has too few
        headings to be worth navigating.
    """
    entries = [(level, text, ident) for level, text, ident in headings if level in (2, 3)]
    if len(entries) < 3:
        return ""
    links = "".join(
        f'<a href="#{ident}" class="{"sub2" if level == 3 else ""}">{html.escape(text)}</a>'
        for level, text, ident in entries
    )
    return f"<nav><b>{html.escape(_('On this page'))}</b>{links}</nav>"


def render_markdown(text: str) -> tuple[str, list[tuple[int, str, str]]]:
    """Render Markdown to HTML.

    Args:
        text: The document source.

    Returns:
        The body HTML and the headings found, as ``(level, text, id)``.
    """
    renderer = _Renderer(text.replace("\r\n", "\n").replace("\r", "\n").split("\n"))
    body = renderer.render()
    return body, renderer.headings


def docs_page(
    title: str,
    text: str,
    source_name: str = "",
    generated_at: datetime | None = None,
) -> str:
    """Render a Markdown document as a complete, self-contained HTML page.

    Args:
        title: The page heading.
        text: The Markdown source.
        source_name: The file this came from, shown in the subtitle so a reader
            knows which document on disk they are looking at.
        generated_at: When the page was built, stamped into the header like
            every other generated page. Defaults to now.

    Returns:
        A complete HTML document -- no CDN, no external stylesheet, no script.
    """
    body, headings = render_markdown(text)
    stamped = generated_at or datetime.now()  # noqa: DTZ005 - local clock is what the user reads
    subtitle = (
        _("Rendered from %(name)s") % {"name": source_name}
        if source_name
        else _("Program documentation")
    )
    when = _("Generated %(when)s") % {"when": stamped.strftime("%Y-%m-%d %H:%M:%S")}
    return (
        "<!DOCTYPE html>\n"
        '<html lang="en"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        f"<title>{html.escape(title)}</title><style>{DOCS_CSS}</style></head><body>"
        f"<header><h1>{html.escape(title)}</h1>"
        f'<div class="sub">{html.escape(subtitle)} &nbsp;&middot;&nbsp; {html.escape(when)}</div>'
        "</header>"
        f'<div class="wrap">{_contents(headings)}<main>{body}</main></div>'
        "</body></html>"
    )
