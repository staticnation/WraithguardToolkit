"""A Morrowind script's control flow as a Mermaid flowchart."""

from __future__ import annotations

from wraithguard.mwscript.flowchart import flowchart, parse

SCRIPT = """begin t ; a comment
short a
if ( a == 1 ) ; c
  set a to 2
  return
elseif ( a > 2 )
  while ( a < 10 )
    set a to a + 1
  endwhile
else
  Disable
endif
MessageBox "hi; there"
end
"""


def test_blocks_nest_as_written() -> None:
    """if/elseif/else and while become nested blocks; declarations are left out."""
    name, root = parse(SCRIPT)
    assert name == "t"
    kinds = [item[0] for item in root.items]
    assert kinds == ["if", "stmt"]
    _if, arms, otherwise = root.items[0]
    assert [cond for cond, _body in arms] == ["a == 1", "a > 2"]
    assert otherwise is not None
    assert arms[1][1].items[0][0] == "while"


def test_the_chart_has_decisions_loops_and_returns() -> None:
    """Decisions with yes/no edges, the loop's way back, return to the frame's end."""
    chart = flowchart(SCRIPT)
    assert chart.startswith("flowchart TD")
    assert '{"if a == 1"}' in chart
    assert '{"while a &lt; 10"}' in chart
    assert '(["return"])' in chart
    assert "|yes|" in chart and "|no|" in chart
    assert "hi; there" in chart  # a ; inside quotes is not a comment


def test_an_unclosed_block_still_draws() -> None:
    """A missing endif is closed at the script's end, as the chart needs."""
    chart = flowchart("begin u\nif ( 1 == 1 )\nDisable\nend\n")
    assert '{"if 1 == 1"}' in chart and '["Disable"]' in chart
