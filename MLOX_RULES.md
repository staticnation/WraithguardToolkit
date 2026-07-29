# Writing mlox rules

A practical reference for the rule maker in this tool, and for hand-editing a
rules file.

This is written from scratch for MLOX Subset Sort. The *conventions* it
describes - citing sources, when to reach for which rule, what counts as good
practice - are the community's, set out in the mlox rule guidelines maintained
at [morrowind-modding.github.io](https://morrowind-modding.github.io/modding-tools/sorting-plugin-load-order/mlox/mlox-rule-guidelines ).
Read that page too if you intend to contribute rules upstream; it is the
authority, and this is a summary written in our own words. The *syntax* below is
simply the file format mlox reads.

---

## Start here: what are you trying to say?

Pick the rule from what you want to express, not from the list of rule names.

| You want to say | Use |
|---|---|
| "This mod has to load after that one" | `[Order]` |
| "This mod doesn't work without that one" | `[Requires]` |
| "These two shouldn't both be installed" | `[Conflict]` |
| "This patch exists to join these mods" | `[Patch]` |
| "Here's something worth knowing" | `[Note]` |
| "Drag this toward the front / back of the order" | `[NearStart]` / `[NearEnd]` |

Nearly everything is `[Order]`. If you find yourself reaching for `[NearStart]`
or `[NearEnd]`, the community guidance is to stop and use `[Order]` instead -
those two are blunt instruments and the rule-base keeps very few of them.

---

## The shape of a rules file

A rule begins with a label in square brackets and runs until the next label or
the end of the file. Plugin names are not quoted and are matched
case-insensitively, so you never have to worry about how a mod capitalised its
own filename.

```
; a comment -- stripped before anything else is read
@Some Mod Name

[Order]
Morrowind.esm
Some Mod.esp
```

Two conventions worth adopting from the start:

- `**;` comments** are for notes to whoever reads the file next. mlox removes
  them before parsing, so they cannot break a rule.
- `**@Section` headings** group rules, roughly one section per mod. They have no
  effect on behaviour - they exist so a file of thousands of rules stays
  navigable.

### Where to put your rules

Put them in **your own file**, not in `mlox_base.txt` or `mlox_user.txt`. Those
two are replaced wholesale when rules are updated, and your work goes with them.
The rule maker defaults to a personal file for this reason and adds it to the
rule list **last**, so that when your rule disagrees with the shipped rule-base,
yours wins.

---

## Ordering rules

### `[Order]`

Lists plugins in the order they must load. First line loads first.

```
[Order]
Big Overhaul.esm
Big Overhaul Patch.esp
```

The relationship carries through a chain: listing three plugins says the first
precedes the second *and* the second precedes the third, and therefore that the
first precedes the third.

Two things to know about how mlox resolves these:

- **Contradictions are dropped, not reported as errors.** If one rule puts A
  before B and another puts B before A, that is a cycle, and mlox discards
  whichever it meets second. A rule that contradicts something already in the
  rule-base may therefore do nothing at all, silently. The rule maker warns you
  when your rule fights the frozen curated order for this reason.
- **A plugin listed twice is a cycle with itself**, which is always a mistake.
  The rule maker refuses it.

### `[NearStart]` and `[NearEnd]`

Each listed plugin is pulled toward one end of the load order.

```
[NearEnd]
Mashed Lists.esp
```

`[NearEnd]` does not mean "put this last". It means "move this toward the end
where possible", and how far it gets depends on everything else. Use it only
when a plugin genuinely needs to be near an end regardless of what else is
installed - a merged-lists patch is the classic case.

---

## Rules that produce a message

These four warn the user instead of moving anything. Each takes a message and
one or more expressions.

### Two ways to write the message

Short messages go inside the label:

```
[Note check the readme before using this] Some Mod.esp
```

Longer ones go on the following lines, **each indented**:

```
[Note]
 This mod replaces the whole Balmora exterior, so anything else
 that edits those cells will be overridden.
Some Mod.esp
```

The indentation is what marks a line as message text rather than a plugin. This
matters more than it looks: a plugin name accidentally written with a leading
space is read as part of the message, and the rule quietly stops applying to
that plugin. The rule maker strips the whitespace for you.

One limit on the short form: the message cannot contain `]`, which would close
the label early. Use the indented form if you need one - the rule maker switches
automatically.

### `[Note]`

Prints its message when the listed expressions are satisfied. The
general-purpose rule; reach for it when nothing more specific fits.

### `[Requires]`

States that the first expression needs the second.

```
[Requires]
 Some Mod - Addon.esp needs the base mod installed
 (Ref: Some Mod readme.txt)
Some Mod - Addon.esp
Some Mod.esp
```

Warns in red when the first is present and the second is not. Exactly two
expressions: what depends, then what it depends on.

### `[Conflict]`

States that the listed things should not be active together. Warns in yellow
when two or more of them are.

```
[Conflict]
 These two both rebuild the Ascadian Isles and will fight each other.
 (Ref: forum thread, see below)
Isles Overhaul.esp
Ascadian Redone.esp
```

### `[Patch]`

States a mutual dependency, and it is worth understanding why this is not just
a `[Requires]`. A patch says **two** things: the patch is pointless without what
it patches, *and* the thing being patched wants the patch. mlox warns in both
directions - if you install the patch alone, and if you install the mods without
their patch.

```
[Patch]
 This patch makes the two mods below compatible.
 (Ref: patch readme)
Big Overhaul + Other Mod Patch.esp
[ALL Big Overhaul.esm Other Mod.esp]
```

`[Patch]` cannot express NOT. If you need "use this patch with X but not with
Y", the guidelines' answer is to write two rules instead - a `[Conflict]`
between the patch and Y, and a `[Requires]` between the patch and X. The rule
maker refuses a NOT inside a `[Patch]` and tells you this.

---

## Expressions

Anywhere a message rule takes a plugin, it will take a logical expression
instead.

```
[ALL A.esp B.esp]          both are active
[ANY A.esp B.esp]          at least one is active
[NOT A.esp]                it is not active
```

They nest, which is how you handle a mod that ships in several editions:

```
[Requires]
 Needs the arrows plugin, in whichever edition you installed.
My Archery Mod.esp
[ANY Arrows - Expanded.esp
 Arrows - Vanilla.esp]
```

### Testing something other than presence

Three predicates look at the plugin file itself rather than at whether it is
loaded. Each must be written on a single line.

**Version** - compares against the version in the plugin header, falling back to
one in the filename:

```
[VER < 1.2 Some Mod.esp]
```

The operators are `<`, `=` and `>`. Version numbers can be up to three
dot-separated or underscore-separated numbers with an optional trailing letter:
`1`, `1.0`, `1.2.3a`, `1_3a` and `77g` are all understood.

**Description** - matches a regular expression against the plugin header's
description field:

```
[DESC /beta build/ Some Mod.esp]
[DESC !/beta build/ Some Mod.esp]
```

The second form, with `!`, means the description does *not* match. Useful for
pinning a rule to one specific release when the author did not bump the version.

**Size** - matches the file size in bytes exactly, with the same `!` negation:

```
[SIZE 2476 Some Mod.esp]
```

Size is the bluntest of the three, and worth using only when nothing else
distinguishes two releases of a file.

---

## Wildcards in filenames

Three patterns expand against the plugins you actually have:

| Pattern | Matches |
|---|---|
| `?` | any single character |
| `*` | any run of characters |
| `<VER>` | anything that looks like a version number |

```
[NearEnd]
Merged Objects*.esp
```

Expansion costs time proportional to your load order, so the guidelines ask that
it be used sparingly. Prefer a literal filename when you know it. The rule maker
flags a rule that uses expansion, not to stop you but so the choice is
deliberate.

---

## Citing your source

Every rule should say where its claim came from, in a `(Ref: ...)` note in the
message:

```
(Ref: Some Mod readme.txt)
(Ref: https://example.invalid/thread/12345 )
```

This is the single most valuable convention in the rule-base. A rule without a
source cannot be checked, corrected or updated by anyone else - including you,
in a year, when you have forgotten why you wrote it.

Note the space before the closing parenthesis in the URL example. Some forums
and wikis auto-link URLs and will swallow a `)` into the link, producing a
citation nobody can follow. The rule maker adds that space for you.

---

## Priority markers

Beginning a message with exclamation marks raises it in mlox's message pane:

| Prefix | Color | Use it for |
|---|---|---|
| `!` | blue | worth knowing, little or no effect on play |
| `!!` | yellow | could affect the game; worth attending to |
| `!!!` | red | could break the mod or the game; should be fixed |

`[Requires]` is already shown in red, and `[Conflict]` and `[Patch]` in yellow,
so adding a marker to those three achieves nothing. The rule maker points this
out rather than silently ignoring it.

If a message would spoil something, it can be hidden behind
`<hide ...</hide` markers, and the reader chooses whether to reveal it.

---

## A worked example

Suppose you install a mod called `Vivec Expanded.esp`, and a compatibility patch
`Vivec Expanded - Guilds Patch.esp` that only makes sense alongside
`Guilds Redone.esp`. The patch's readme says so.

```
@Vivec Expanded

; The patch masters both mods, so the game already forces this order --
; writing it down makes the reason explicit for anyone reading the file.
[Order]
Vivec Expanded.esp
Vivec Expanded - Guilds Patch.esp

[Patch]
 Joins Vivec Expanded to Guilds Redone. Neither is much use with the
 other installed until this is too.
 (Ref: Vivec Expanded - Guilds Patch readme.txt)
Vivec Expanded - Guilds Patch.esp
[ALL Vivec Expanded.esp Guilds Redone.esp]
```

The rule maker will build both of these for you, check them, and append them to
your personal rules file. The **Check Conflicts** scan can also propose rules
like the `[Order]` above directly, because a plugin's masters are recorded in
its own header - that one is a fact, not a guess, and the tool labels it as
such.

---

## What the rule maker checks for you

Everything below is checked before a rule can be written, because mlox discards
a rule it cannot use *without saying so* - which makes the moment of writing the
only opportunity to find out.

**Refused outright**

- `[Order]` with fewer than two plugins, or the same plugin twice
- `[Requires]` or `[Patch]` without exactly two expressions
- `[Conflict]` with fewer than two things to conflict
- a `NOT` inside a `[Patch]`
- a name that is not a plugin filename, or that contains `[`, `]` or `;` -
  characters mlox reads as syntax, so the rule would silently refer to something
  other than what you typed
- a version that mlox would not recognise, or a comparison operator other than
  `<`, `=`, `>`
- a `[DESC]` regular expression that does not compile

**Warned about, but allowed**

- no `(Ref:)` citation
- `[NearStart]` / `[NearEnd]`, which the guidelines discourage
- a warning rule with no message
- filename expansion, which is slow
- a priority marker on a rule mlox already highlights
- a group like `[ALL x]` wrapped around a single item, which does nothing

---

## Credits

The conventions described here come from the mlox rule guidelines maintained by
the Morrowind modding community at
[morrowind-modding.github.io](https://morrowind-modding.github.io/modding-tools/sorting-plugin-load-order/mlox/mlox-rule-guidelines ),
and from mlox itself, originally by John Moonsugar. This page is an independent
summary written for this tool; the examples are our own. Where the two differ,
the community page is authoritative.
