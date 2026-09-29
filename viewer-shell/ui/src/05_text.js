/* =====================================================================================
   Everything the interface says that has no element of its own.

   For anything with an element, the markup is the source: the words are inside it, and a
   `data-tx="key"` names them so the rest of the page can ask for the same string with
   `T('key')`. `data-txt` does the same for a `title` tooltip and `data-txp` for a
   placeholder. Only the strings with nowhere to live — a label built in script, a hint
   whose wording depends on a switch — are written out in `TX_STRINGS` below, each with a
   note saying where it appears.

   These keys used to feed `gardenfell.text.toml`, a file beside the exe holding every
   string so the wording could be reworded or translated without editing the page. Robin
   asked for that to go (round 18aa) and it has: the file, the catalogue that built it, the
   engine command that carried it. What is left is the arrangement that was here before —
   the words in the code, in one place per string — and the keys, which is what any of it
   would be rebuilt on.
   ===================================================================================== */

const TX_STRINGS = {
  /* Round 17v: the report is the thing Robin pastes to me, and it was being selected by
     hand. Each section header carries its own copy button and the head carries one for
     the lot; both say so after they have run, because a clipboard write is silent. */
  "report.copysec":
    ["Copy",
     "Load-order report, the button on each section heading that copies that section."],
  "report.copysec_title":
    ["Copy this section to the clipboard",
     "Load-order report, that button's tooltip."],
  "report.copywait":
    ["Waiting for the GPU…",
     "The report's copy buttons while a GPU line is still being measured; the copy follows once it is in."],
  "report.copied":
    ["Copied",
     "Load-order report, what a copy button says for a moment after it has copied."],
  "report.copyfail":
    ["Could not copy \u2014 select the text and copy it by hand",
     "Load-order report, said when the clipboard refused the write."],
  /* Round 17y: a room without a sky holds the Atmosphere switch off; this is the reason
     on hover. Round 17y also: the door button in the object dialogue. */
  "preview.sky_room":
    ["Off in a room: an interior has no sky unless its cell is flagged to behave like an exterior. Your setting comes back when you leave.",
     "Left column, the tooltip on the dimmed Atmosphere row while an interior is up."],
  "avoid.door_open":
    ["Open cell door leads to",
     "The object dialogue, the green button on a teleporting door that loads the cell on the far side."],
  "avoid.door_open_title":
    ["Load {cell} \u2014 where this door goes",
     "The object dialogue, the door button's tooltip; {cell} is the destination's name."],
  /* Said in script, so it has no element to take a default from: the never-accept-paint
     button changes what it says depending on whether this mesh is already on the list. */
  "avoid.nopaint_remove":
    ["Let this mesh take paint again",
     "The object dialogue, that button when the mesh is already on the never-accept-paint list."],
  /* The paint verdict — one of three, picked in script from the pattern list and the
     per-reference switch, so the box has no single default to read from the markup. */
  /* Round 18n: the canopies section of the object dialogue. The verdict is one of two,
     picked in script from the canopy list, and the picker's last entry makes a new one. */
  "avoid.canopy_verdict_in":
    ["Part of {name}",
     "The object dialogue, the canopy verdict when a canopy's pattern matches this mesh; {name} is the canopy's."],
  "avoid.canopy_verdict_by":
    ["— by its pattern {pattern}",
     "The object dialogue, after the canopy's name in that verdict; {pattern} is the one that matched."],
  "avoid.canopy_verdict_none":
    ["Not part of any canopy.",
     "The object dialogue, the canopy verdict when no canopy's pattern matches this mesh."],
  "avoid.canopy_verdict_any":
    ["Cards asking for <i>any</i> canopy still see it if it hangs over the ground.",
     "The object dialogue, the plain text after that verdict."],
  "avoid.canopy_verdict_first":
    ["There are no canopies yet — adding this mesh makes the first.",
     "The object dialogue, the plain text after that verdict while the rules file has no canopies."],
  "avoid.canopy_new":
    ["New canopy…",
     "The object dialogue, the last entry of the canopy picker."],
  "avoid.canopy_unnamed":
    ["(unnamed canopy)",
     "The object dialogue, how a canopy with no name is listed."],
  "avoid.canopy_remove":
    ["Remove",
     "The object dialogue, the button beside a canopy this mesh is part of."],
  "avoid.canopy_remove_title":
    ["Takes the pattern {pattern} off {name}. Every mesh it matched leaves the canopy with it.",
     "The object dialogue, hovering that button."],
  "avoid.paint_verdict_ok":
    ["This mesh can be painted on.",
     "The object dialogue, the paint verdict when the brush may land here."],
  "avoid.paint_verdict_never":
    ["Nothing can be painted on this mesh.",
     "The object dialogue, the paint verdict when a never-paint pattern covers this mesh."],
  "avoid.paint_verdict_ref":
    ["This reference does not take paint.",
     "The object dialogue, the paint verdict when the switch below has turned this one reference off."],
  /* Said in script when the painting button is greyed out: Simplified mode has no
     real ground to paint, so the button is disabled there and its tooltip says why. */
  "paint.mode_simple_off":
    ["Painting needs a real cell. Switch to Real cell preview.",
     "Left column, hovering the disabled painting button in Simplified mode."],
  /* The button's label is set in script — it names the state you are in, and reading
     the "default" back out of a relabelled button is how a label eats itself. */
  /* The Advanced scope's rule-kind chip, same reason: its label names the state it is
     in, and a `data-tx` on an element whose text the script rewrites makes the first
     relabel become the "default" and eat the wording it replaced. */
  "esp.adv_rulekind_all":
    ["All rules",
     "Advanced scope, the rule-kind chip while it is showing every rule."],
  "esp.adv_rulekind_ground":
    ["Ground texture rules",
     "Advanced scope, the rule-kind chip while it is showing only those."],
  "esp.adv_rulekind_paint":
    ["Paint only rules",
     "Advanced scope, the rule-kind chip while it is showing only those."],
  "vp.cull_off":
    ["Highlight culling",
     "Viewport, the Highlight culling button while it is off."],
  "vp.empty_mesh_title":
    ["Show a marker at every position on this mesh an empty/placeholder or object-ID card won",
     "Viewport, hovering the mesh editor's Empty positions button (round 18m)."],
  "vp.cull_mesh_title":
    ["Shade the faces of this mesh that the rule’s steepest-slope limit keeps grass off. Slope only: a mesh in the editor stands at the origin and has no ground height.",
     "Viewport, hovering the mesh editor's Highlight culling button (round 18l)."],
  "vp.hitex_mesh_title":
    ["Hovering a texture tile below lights up the faces of this mesh that wear it",
     "Viewport, hovering the mesh editor's Highlight texture button (round 18l)."],
  "vp.cull_terrain":
    ["Highlight culling: Terrain only",
     "Viewport, the Highlight culling button in its second state."],
  "vp.cull_both":
    ["Highlight culling: Terrain + statics",
     "Viewport, the Highlight culling button in its third state."],
  "vp.hiobj_outline":
    ["Highlight objects: Outline",
     "Viewport, the Highlight objects button in its third state (round 18dq): an outline around the pointed-at object instead of the gold fill."],
  /* The two group headings in the rules list, and what hovering one says. They are built
     in script rather than written in the markup — the list is drawn from the config — so
     they need entries here to reach the wording file at all. */
  "left.group_view":
    ["In this view",
     "Left column, the heading over the rules the loaded cells use, shown in Real cell mode (round 15)."],
  "left.group_ground":
    ["Ground textures",
     "Left column, the heading over the rules that name a ground texture."],
  "left.group_paint":
    ["Paint only",
     "Left column, the heading over the rules that grow only where their colour is painted."],
  "left.group_fold":
    ["Fold these rules away",
     "Left column, hovering an open group heading in the rules list."],
  "left.group_show":
    ["Show these rules",
     "Left column, hovering a folded group heading in the rules list."],
  "left.group_empty":
    ["Nothing of this kind yet",
     "Left column, hovering a group heading in the rules list that has no rules under it."],
  "panel.sec_selector":
    ["Selector",
     "Right column, the heading of the first section."],
  "panel.tex":
    ["Land texture",
     "Right column, the Land texture label."],
  "panel.tex_tip":
    ["The LTEX id as it appears in the game data — matched without regard to case",
     "Right column, hovering the Land texture label."],
  "panel.tex_unknown":
    ["<b style=\"color:var(--err)\">This id matches no LTEX record</b> in the scanned load order, so this section will never place anything. Check the spelling and case, or enable the plugin that defines it.",
     "Right column, when the land texture is not in the load order."],
  "panel.scope_everywhere":
    ["Everywhere this texture appears",
     "Right column, the first Scope choice."],
  "panel.scope_named":
    ["Only cells that have a name (ANY_NAMED_CELL)",
     "Right column, the second Scope choice."],
  "panel.scope_specific":
    ["A specific cell or region…",
     "Right column, the third Scope choice."],
  "panel.scope":
    ["Scope",
     "Right column, the Scope label."],
  "panel.scope_tip":
    ["How this section is narrowed down",
     "Right column, hovering the Scope label."],
  "panel.bulk_identity_hint":
    ["The ground texture and scope stay with each rule — two rules on one texture is something to do on purpose, one at a time. Everything below this is edited for all of them at once.",
     "Right column, under the list of rules when several are selected."],
  "panel.bulk_slots_differ":
    ["Different grass slots, can't edit several at the same time",
     "Right column, in place of the grass cards when the selected rules carry different ones."],
  "panel.bulk_slots_hint":
    ["Select rules that carry the same cards and they become editable together.",
     "Right column, under that message."],
  /* Round 18bf: the multi-edit's "paste one list over every selected rule", offered both
     where the lists are identical and where they differ. */
  "panel.ban_all":
    ["Paste to all selected…",
     "The Exclusions section, in a multi-edit: the button that replaces every selected rule's exclusions with one pasted list."],
  "panel.ban_all_title":
    ["Paste one list of exclusions and give it to all {n} selected rules, replacing whatever each of them has now",
     "The Exclusions section: that button's tooltip. {n} is how many rules are selected."],
  "panel.ban_all_head":
    ["Exclusions for all {n} selected rules",
     "The paste-to-all dialogue's heading. {n} is how many rules are selected."],
  "panel.ban_all_note":
    ["Every one of the {n} selected rules ends up with exactly this list. What each of them has now is replaced, not added to — one undo step puts it all back.",
     "The paste-to-all dialogue's note under the box. {n} is how many rules are selected."],
  "panel.ban_all_apply":
    ["Replace in all {n}",
     "The paste-to-all dialogue's confirming button. {n} is how many rules are selected."],
  "panel.bans_replaced_all.one":
    ["{n} exclusion, on {rules} rules",
     "Toast after pasting one list of exclusions over a multi-edit selection, singular. {rules} is how many rules took it."],
  "panel.bans_replaced_all.other":
    ["{n} exclusions, on {rules} rules",
     "Toast after pasting a list of exclusions over a multi-edit selection. {rules} is how many rules took it."],
  "undo.bans_all.one":
    ["replace the exclusions of {n} rule",
     "The undo step's name after pasting exclusions over a multi-edit selection, singular."],
  "undo.bans_all.other":
    ["replace the exclusions of {n} rules",
     "The undo step's name after pasting exclusions over a multi-edit selection."],
  "panel.bulk_bans_differ":
    ["Different exclusions, can't edit several at the same time",
     "Right column, in place of the exclusions when the selected rules carry different ones."],
  "panel.paint_only":
    ["Paint only rule",
     "Right column, the switch that detaches a rule from any ground texture."],
  "panel.paint_only_tip":
    ["This rule has no land texture. It grows only on ground you paint with its colour.",
     "Right column, hovering the Paint only rule label."],
  "panel.rule_name":
    ["Rule name",
     "Right column, the name field that replaces Ground texture on a paint-only rule."],
  "panel.rule_name_tip":
    ["What to call this rule. Yours to choose — it is never looked for in your install.",
     "Right column, hovering the Rule name label."],
  "panel.paint_only_hint":
    ["No ground grows this by itself. Give it a colour under <b>Paint tools</b>, paint some ground with it, and this rule grows there — whatever land texture is underneath. Its own slope, height and exclusions still apply.",
     "Right column, under the name of a paint-only rule."],
  "panel.cell":
    ["Cell / region",
     "Right column, the Cell / region label."],
  "panel.cell_tip":
    ["Tried as a cell name first, then as a region name",
     "Right column, hovering the Cell / region label."],
  "panel.notes":
    ["Notes",
     "Right column, the Notes label."],
  "panel.notes_tip":
    ["Kept with the rule in the config, and read from the ; comments above a section when an .ini is imported",
     "Right column, hovering the Notes label."],
  "panel.notes_ph":
    ["e.g. bulk of the landscape, 43 uses",
     "Right column, the grey text in the Notes box."],
  "panel.sec_placement":
    ["Placement",
     "Right column, the heading of the Placement section."],
  "panel.place":
    ["Place grass",
     "Right column, the Place grass label."],
  "panel.place_tip":
    ["bPlaceGrass — on or off for this whole rule",
     "Right column, hovering the Place grass label."],
  "panel.place_off_hint":
    ["This is an <b>exclusion</b> rule: it places nothing, and it keeps a broader rule from placing anything here either. Useful as the narrow half of a pair — this one scoped to named cells and switched off, a plain one switched on.",
     "Right column, shown when Place grass is off."],
  "panel.gap":
    ["Gap",
     "Right column, the Gap label."],
  "panel.gap_tip":
    ["iGap — units between objects. Required.",
     "Right column, hovering the Gap label."],
  "panel.gap_hint":
    ["Smaller means denser. Keep it under <b>512</b>.",
     "Right column, under the Gap field."],
  "panel.zoff":
    ["Height offset",
     "Right column, the per-rule height offset label."],
  "panel.zoff_tip":
    ["iZPositionModifier \u2014 on top of the 16 units Groundcover Generator always adds",
     "Right column, hovering the Height offset label."],
  "panel.zoff_hint":
    ["Nudges this texture\u2019s grass up or down. <b>0</b> is not ground level: Groundcover Generator lifts everything it places by <b>16</b> units and this is the extra on top, so an imported .ini keeps meaning what it meant. Use \u221216 to sit exactly on the ground. A mesh drawn around its own middle needs a different value from one drawn from its base, which is why it is per rule rather than per file.",
     "Right column, under the Height offset field."],
  "panel.clump":
    ["Clump meshes",
     "Right column, the Clump meshes label."],
  "panel.clump_tip":
    ["bRandClump — one mesh per patch of ground instead of a fresh draw per blade",
     "Right column, hovering the Clump meshes label."],
  "panel.clump_hint":
    ["Pair it with a heavy <b>Empty</b> slot for sparse clusters — the flower-field trick — instead of even coverage.",
     "Right column, under the Clump meshes switch."],
  "panel.clumpsize":
    ["Clump size",
     "Right column, the Clump size label."],
  "panel.clumpsize_tip":
    ["iClumpArea — how much ground one mesh covers, in units",
     "Right column, hovering the Clump size label."],
  "panel.clumpsize_hint":
    ["A land texture tile is <b>512</b> units, which is why 512 is the faithful default. Clumps are anchored to the world, so they run across cell borders and stay put as you change the gap.",
     "Right column, under the Clump size field."],
  "panel.clumpnoise":
    ["Noisy clumps",
     "Right column, the Noisy clumps label."],
  "panel.clumpnoise_tip":
    ["Gardenfell’s own. Off by default so an imported .ini keeps its square tiling.",
     "Right column, hovering the Noisy clumps label."],
  "panel.clumpnoise_hint":
    ["Gives the clumps wandering outlines instead of square ones. Nothing else moves — same blades, same positions, only which mesh each takes.",
     "Right column, under the Noisy clumps switch."],
  "panel.align":
    ["Follow ground slope",
     "Right column, the Follow ground slope label."],
  "panel.align_tip":
    ["bAlignObjectNormalToGround — tilt each object to stand square to the ground",
     "Right column, hovering the Follow ground slope label."],
  "panel.sec_culling":
    ["Culling",
     "Right column, the heading of the Culling section."],
  "panel.slope":
    ["Steepest slope",
     "Right column, the Steepest slope label."],
  "panel.slope_tip":
    ["fMaximumAngle — in degrees",
     "Right column, hovering the Steepest slope label."],
  "panel.minh":
    ["Lowest ground",
     "Right column, the Lowest ground label."],
  "panel.minh_tip":
    ["fMinHeight — ground below this grows nothing",
     "Right column, hovering the Lowest ground label."],
  "panel.maxh":
    ["Highest ground",
     "Right column, the Highest ground label."],
  "panel.maxh_tip":
    ["fMaxHeight — ground above this grows nothing",
     "Right column, hovering the Highest ground label."],
  "panel.height_hint":
    ["The sea is at 0, so a lowest ground just above it keeps grass out of the water. Red tint marks ground these reject.",
     "Right column, under the Culling fields."],
  "panel.additive":
    ["Adds to the ground",
     "Right column, the Adds to the ground label under Scope, shown for a scoped rule."],
  "panel.additive_tip":
    ["Off: inside its scope this rule replaces the texture's other rules. On: it grows beside them, keeping to its scope but taking nothing away. Gardenfell's own.",
     "Right column, hovering the Adds to the ground label."],
  "panel.additive_hint_on":
    ["Adds: reeds on the coast, and the coast keeps its grass. Each rule packs on its own, so the ground gets denser where they overlap.",
     "Right column, under the Adds to the ground switch while it is on."],
  "rule.enabled":
    ["Enabled",
     "Right column, the power button at the very top while the rule is on (round 15)."],
  "rule.disabled":
    ["Disabled",
     "Right column, the power button while the rule is off."],
  "rule.enabled_tip":
    ["This rule is on. Press to switch it off: it keeps its ground, its paint and its grass, and scatters nothing until it is switched on again.",
     "Right column, hovering the power button while the rule is on."],
  "rule.disabled_tip":
    ["This rule is off. Press to switch it on again.",
     "Right column, hovering the power button while the rule is off."],
  "rule.disabled_hint":
    ["Off: nothing grows for this rule, and ground painted with it counts as painted with no rule at all.",
     "Right column, beside the power button while the rule is off."],
  "panel.additive_hint_off":
    ["Replaces: inside this scope, this rule is the only one the texture grows. The way a scoped rule has always worked.",
     "Right column, under the Adds to the ground switch while it is off."],
  "panel.jitter":
    ["Jitter",
     "Right column, the Jitter label under Gap."],
  "panel.jitter_tip":
    ["Nudges each object off the spot the packing chose, by up to this fraction of the gap. Gardenfell's own — no .ini key.",
     "Right column, hovering the Jitter label."],
  "panel.jitter_hint":
    ["Each object may wander up to {pct}% of the gap, but never closer than {floor}% of it to a neighbour, and never onto ground its rule would refuse — no roads, no rocks, no wrong texture. Breaks the rows a sparse scatter falls into; the count of objects does not change.",
     "Right column, under the Jitter slider while it is above zero."],
  "panel.jitter_hint_off":
    ["Off: every object stands exactly where the packing put it. At a high gap that reads as rows — turn this up to break them.",
     "Right column, under the Jitter slider while it is at zero."],
  "panel.jitter_hint_mixed":
    ["These rules disagree about jitter. Move the slider to set them all.",
     "Right column, under the Jitter slider when several rules are edited together and disagree."],
  "panel.slopemin":
    ["Shallowest slope",
     "Right column, the Shallowest slope label."],
  "panel.slopemin_tip":
    ["Ground flatter than this, in degrees, grows nothing — the floor to Steepest slope's ceiling. Second field: degrees of fade below it. Gardenfell's own.",
     "Right column, hovering the Shallowest slope label."],
  "panel.fade_tip":
    ["Fade: how far past the limit grass thins out and shrinks rather than stopping dead. 0 is a hard cut.",
     "Right column, hovering any fade field."],
  "panel.sec_terrain":
    ["Terrain",
     "Right column, the heading of the Terrain section."],
  "panel.terrain_hint":
    ["What the ground is doing where a blade would stand — which way it faces, whether it is a hollow or a ridge, how far the sea is. Each is off until switched on, and costs nothing while off.",
     "Right column, under the Terrain heading."],
  "panel.band":
    ["Between",
     "Right column, the Between label on a Terrain band."],
  "panel.band_tip":
    ["Low, high, fade — in {unit}. Leave a side empty to leave it open.",
     "Right column, hovering the Between label on a Terrain band."],
  "panel.gate_mixed":
    ["These rules disagree here. Switch it either way to set them all.",
     "Right column, under a Terrain switch when several rules are edited together and disagree."],
  "panel.facing":
    ["Facing",
     "Right column, the Facing label."],
  "panel.facing_tip":
    ["Only on slopes that face a chosen compass direction. Flat ground always passes — raise Shallowest slope to keep it off the flats.",
     "Right column, hovering the Facing label."],
  "panel.facing_at":
    ["Heading ± width",
     "Right column, the Heading label under Facing."],
  "panel.facing_at_tip":
    ["Compass degrees: 0 north, 90 east, 180 south, 270 west. Then how far either side still counts, and a fade in degrees.",
     "Right column, hovering the Heading label."],
  "panel.facing_reads":
    ["Reads as: slopes facing {dir}, from {from}° round to {to}°.",
     "Right column, the sentence under the Facing fields that says the heading in words."],
  "panel.facing_hint":
    ["Moss on the shaded side, scrub burning off the sunny one. A slope faces the way it drops.",
     "Right column, under the Facing fields while it is on."],
  "panel.facing_hint_off":
    ["Off: slopes facing any way.",
     "Right column, under the Facing switch while it is off."],
  "panel.curve":
    ["Curvature",
     "Right column, the Curvature label."],
  "panel.curve_tip":
    ["Hollow or ridge: how far the ground here stands above the average of the ground around it. Negative is a hollow, positive a ridge.",
     "Right column, hovering the Curvature label."],
  "panel.curve_radius":
    ["Measured over",
     "Right column, the Measured over label under Curvature."],
  "panel.curve_radius_tip":
    ["How far out to look, in units. 512 is one land texture tile; 128 is as fine as the landscape grid goes.",
     "Right column, hovering the Measured over label."],
  "panel.curve_hint":
    ["Grass where water would collect, bare where wind would scour it. \"Below −20\" keeps to the hollows; \"above 20\" to the ridges.",
     "Right column, under the Curvature fields while it is on."],
  "panel.curve_hint_off":
    ["Off: hollows and ridges alike.",
     "Right column, under the Curvature switch while it is off."],
  "panel.shore":
    ["Shoreline",
     "Right column, the Shoreline label."],
  "panel.shore_tip":
    ["Distance from where the land meets the sea, in units. Negative is under water. Exteriors only — a room's water is not a shoreline.",
     "Right column, hovering the Shoreline label."],
  "panel.shore_hint":
    ["\"0 to 300\" is a strip of reeds along the coast; \"100 and up\" keeps grass back from the water; \"below 0\" grows only under it. Height alone cannot follow a coastline; this does.",
     "Right column, under the Shoreline fields while it is on."],
  "panel.shore_hint_off":
    ["Off: any distance from the water.",
     "Right column, under the Shoreline switch while it is off."],
  "panel.shore_align":
    ["Rotate with shoreline",
     "Right column, the switch under the Shoreline fields (round 18k)."],
  "panel.shore_align_tip":
    ["Lay every blade along the water's edge instead of spinning it at random: its X axis runs along the shore and its +Y faces the water. For meshes that are longer than they are wide — reeds, driftwood — which look wrong at a random angle on a coast. Only where the ground slopes; flat ground keeps the spin.",
     "Right column, hovering the Rotate with shoreline switch."],
  "panel.shore_turn":
    ["Turn",
     "Right column, the Turn label under Rotate with shoreline."],
  "panel.shore_turn_tip":
    ["Degrees added on top, for a mesh modelled the other way round. 90 turns it a quarter, 180 faces it away from the water.",
     "Right column, hovering the Turn label."],
  "panel.obstacle":
    ["Near objects",
     "Right column, the Near objects label in the Terrain section (round 18k)."],
  "panel.obstacle_tip":
    ["How far from the nearest object grass keeps clear of — the same objects avoidance walks round, measured from their surface in three dimensions, so a bridge overhead counts as far as it is high. Answered with avoidance off as well.",
     "Right column, hovering the Near objects label."],
  "panel.obstacle_hint":
    ["\"0 to 64\" is moss round the rocks and along the walls; \"128 and up\" keeps flowers back from everything. A grass card can tighten this for itself.",
     "Right column, under the Near objects fields while it is on."],
  "panel.obstacle_hint_off":
    ["Off: any distance from objects.",
     "Right column, under the Near objects switch while it is off."],
  "canopy.title":
    ["Canopies",
     "Left column, the heading of the Canopies section (round 18k)."],
  "canopy.hint":
    ["Things that hang over the ground — trees, mushrooms, bridges. Name a family of meshes here (id or mesh, <code>*</code> matches anything after it — e.g. <code>flora_tree_ai*</code>), then tell a grass card to stand only under it, or to keep clear of it. Saved in the <b>rules</b> file. A card can also ask for <i>any</i> canopy, with no entry here.",
     "Left column, under the Canopies heading."],
  "canopy.add":
    ["+ Canopy",
     "Left column, the button that adds a canopy."],
  "canopy.name_ph":
    ["name it — Mushroom trees",
     "Left column, the placeholder of a canopy's name field."],
  "canopy.add_pattern":
    ["+ Add pattern",
     "Left column, the button that adds a pattern to a canopy."],
  "canopy.drop_title":
    ["Remove this canopy. Cards that named it stand nowhere until pointed elsewhere.",
     "Left column, hovering a canopy's delete cross."],
  "card.near":
    ["near objects",
     "A grass card, the near objects row (round 18k)."],
  "card.near_tip":
    ["Lowest and highest distance, in units, from the nearest object grass keeps clear of. Blank is the rule's; this only tightens it. Moss within 64 of the rocks while the grass carries on.",
     "A grass card, hovering the near objects label."],
  "card.canopy":
    ["canopy",
     "A grass card, the canopy row."],
  "card.canopy_tip":
    ["Stand only under the canopies named in the left column, or only clear of them — or under, or clear of, anything at all that hangs over the ground. Add as many rules as you like: they all apply, so one \"clear of\" keeps this mesh out however many it stands under. A canopy is a mesh's visible faces above the spot, so a tree's crown and not its trunk.",
     "A grass card, hovering the canopy label."],
  "card.canopy_under_any":
    ["under any canopy",
     "A grass card, the canopy picker: under anything that hangs over."],
  "card.canopy_clear_any":
    ["clear of any canopy",
     "A grass card, the canopy picker: nothing hanging over."],
  "card.canopy_under":
    ["under {name}",
     "A grass card, the canopy picker: under a named canopy."],
  "card.canopy_clear":
    ["clear of {name}",
     "A grass card, the canopy picker: clear of a named canopy."],
  "card.canopy_gone":
    ["a canopy that no longer exists",
     "A grass card, the canopy picker when the card names a canopy the set has lost."],
  "card.with":
    ["stands with",
     "A grass card, the stands-with row (round 18k)."],
  "card.with_tip":
    ["This mesh stands only where one of another card already stands within reach — small mushrooms round the big one, flowers round a stone. Where none is near, its share of the die goes to the other cards, so nothing is left bare.",
     "A grass card, hovering the stands-with label."],
  "card.with_none":
    ["nobody",
     "A grass card, the stands-with picker's first choice."],
  "card.with_card":
    ["card {index}: {name}",
     "A grass card, one choice in the stands-with picker."],
  "card.within":
    ["within",
     "A grass card, the within label beside stands with."],
  "card.within_tip":
    ["How near, in units. Blank is twice the rule's spacing.",
     "A grass card, hovering the within label."],
  /* Round 18o: three of the rule's own settings a card may state for itself - its
     height offset, its tilt and its shoreline turn. The pickers say what the rule does
     in their first entry, so "the rule's" is never a mystery. */
  "card.zoff":
    ["height offset",
     "A grass card, the height offset row (round 18o)."],
  "card.zoff_tip":
    ["This mesh's own nudge above the ground, in units, on top of the 16 Groundcover Generator always adds. A mesh whose origin sits at the middle of the tuft wants a different number from one whose origin is at its base. Blank is the rule's.",
     "A grass card, hovering the height offset label."],
  "card.tilt":
    ["tilt",
     "A grass card, the tilt row."],
  "card.tilt_tip":
    ["Whether this mesh leans with the ground under it or stands straight up whatever the slope - a mushroom or a flower stands, grass leans. The first entry is the rule's.",
     "A grass card, hovering the tilt label."],
  "card.tilt_rule":
    ["the rule's ({how})",
     "A grass card, the tilt picker's first entry; {how} is what the rule does."],
  "card.tilt_on":
    ["leans with the ground",
     "A grass card, the tilt picker: follows the slope."],
  "card.tilt_off":
    ["stands upright",
     "A grass card, the tilt picker: straight up whatever the slope."],
  "card.tilt_creature":
    ["A creature or a leveled creature never tilts with the ground. The card's own setting is kept for when it is a mesh again.",
     "A grass card, Overrides: the tilt picker's tooltip while the card is a creature or a leveled creature, greyed and shown as off."],
  "card.shore":
    ["shoreline",
     "A grass card, the shoreline row, shown while the rule's shoreline band is on."],
  "card.shore_tip":
    ["Whether this mesh lies along the shore instead of spinning at random, and how far it is turned on top. A rectangular reed lies along; the round tuft beside it spins. The first entry is the rule's; the turn is in degrees, blank the rule's.",
     "A grass card, hovering the shoreline label."],
  "card.shore_rule":
    ["the rule's ({how})",
     "A grass card, the shoreline picker's first entry; {how} is what the rule does."],
  "card.shore_on":
    ["along the shore",
     "A grass card, the shoreline picker: lies along the waterline."],
  "card.shore_off":
    ["random spin",
     "A grass card, the shoreline picker: keeps the random yaw."],
  /* Round 18p: the rest of the rule's terrain gates on the card. Robin: "moss on the
     north faces of a rule that grows everywhere", "mushrooms in the hollows of a rule
     that covers the ridges too" - so each is a plain low/high pair, blank meaning no
     gate, and a card can only narrow what the rule already allowed. */
  /* Round 18q: the card's two folded sections, its name, and the canopy list. */
  "card.sec_overrides":
    ["Overrides",
     "A grass card, the heading of the folded section of settings that replace the rule's."],
  "card.sec_conditions":
    ["Conditions",
     "A grass card, the heading of the folded section of settings that refuse ground."],
  "card.sec_drift":
    ["Drifts",
     "A grass card, the heading of the folded section where this mesh gathers into patches (round 18ac)."],
  "card.drift_note":
    ["on",
     "A grass card, beside the Drifts heading when the section is folded and drifting is on."],
  "card.drift_hint":
    ["Gathers this mesh into patches instead of spreading it evenly through the rule. It does not change how much grass grows: inside a patch this card wins more of the die, outside it the rule’s other cards do.",
     "A grass card, at the top of the Drifts section."],
  "card.drift_on":
    ["Gather into drifts",
     "A grass card, the switch in the Drifts section."],
  "card.drift_alone":
    ["This rule has only this card, so there is nobody to hand the ground to and drifting changes nothing. Add another card first.",
     "A grass card, under the Drifts switch when the rule has a single card."],
  "card.drift_edit":
    ["drifts",
     "The undo step's name when a grass card's drift settings change: “change a grass card’s drifts”."],
  "card.drift_size":
    ["patch size",
     "A grass card, the Drifts section, the size row."],
  "card.drift_size_tip":
    ["Units across a typical patch. Anchored to the world, so a patch runs across a cell border and stays put while you tune the spacing.",
     "A grass card, hovering the patch size label under Drifts."],
  "card.drift_cover":
    ["how much",
     "A grass card, the Drifts section, the coverage row."],
  "card.drift_cover_tip":
    ["Percent of the ground the patches cover. Low is a rare drift; high is this mesh nearly everywhere, with clearings.",
     "A grass card, hovering the how much label under Drifts."],
  "card.drift_edge":
    ["edge",
     "A grass card, the Drifts section, the edge softness row."],
  "card.drift_edge_tip":
    ["0 is a hard edge - this mesh or none. 100 fades out, so the patch thins into the rest of the rule instead of stopping.",
     "A grass card, hovering the edge label under Drifts."],
  "card.drift_warp":
    ["ragged",
     "A grass card, the Drifts section, the warp row."],
  "card.drift_warp_tip":
    ["0 is round blobs, 100 is ragged outlines.",
     "A grass card, hovering the ragged label under Drifts."],
  "card.name_ph":
    ["name it",
     "A grass card, the placeholder of its name field."],
  "card.name_tip":
    ["What you call this card. Its own name, not its mesh's — a rule mixing three grasses and a flower is easier to talk about when the flower says so, and the stands-with picker on the other cards lists these.",
     "A grass card, hovering its name field."],
  "card.canopy_add":
    ["+ Add canopy rule",
     "A grass card, the button that adds another canopy rule to it."],
  "card.canopy_drop":
    ["Remove this canopy rule",
     "A grass card, hovering the cross beside one of its canopy rules."],
  "card.slope":
    ["shallowest / steepest",
     "A grass card, the slope pair - 18j's ceiling with 18p's floor beside it."],
  "card.slope_tip":
    ["Degrees. This mesh keeps to ground between these while the rule's other meshes stand outside them. Blank is the rule's; a card can only narrow what the rule allows.",
     "A grass card, hovering the slope label."],
  "card.facing":
    ["facing",
     "A grass card, the facing row (round 18p)."],
  "card.facing_tip":
    ["Which way the ground faces where this mesh may stand: a compass heading (0 north, 90 east) and how far either side of it counts. Ground too flat to have a facing always passes. Blank is no gate on this mesh.",
     "A grass card, hovering the facing label."],
  "card.curve":
    ["hollow / ridge",
     "A grass card, the curvature row."],
  "card.curve_tip":
    ["How far the ground here sits below or above the ground around it, in units: negative is a hollow, positive a ridge. Mushrooms in the dips of a rule that covers the crests too. Measured at the rule's curvature radius. Blank is no gate on this mesh.",
     "A grass card, hovering the curvature label."],
  "card.shore_band":
    ["from the shore",
     "A grass card, the shoreline band row."],
  "card.shore_band_tip":
    ["How far from the sea's edge this mesh may stand, in units; negative is under water. Reeds in the first strip of a rule that reaches further up the beach. Blank is no gate on this mesh; the rule's own band still holds.",
     "A grass card, hovering the shoreline band label."],
  "panel.sec_random":
    ["Randomisation",
     "Right column, the heading of the Randomisation section."],
  "panel.sclrand":
    ["Vary the scale",
     "Right column, the Vary the scale label."],
  "panel.sclrand_tip":
    ["bSclRand — off places every object at the smallest size instead of drawing one",
     "Right column, hovering the Vary the scale label."],
  "panel.sclrange":
    ["Scale range",
     "Right column, the Scale range label."],
  "panel.sclrange_tip":
    ["fSclMin / fSclMax — smallest and largest",
     "Right column, hovering the Scale range label."],
  "panel.scl_off_hint":
    ["Every object is placed at the smaller of the two. A slot with a range of its own still uses it.",
     "Right column, under the scale range when varying is off."],
  "panel.scl_on_hint":
    ["Each blade takes a size from this range. A slot can override it. Both ends the same places at a fixed size.",
     "Right column, under the scale range when varying is on."],
  "panel.add_slot":
    ["+ Add slot",
     "Right column, the button that adds a grass card."],
  "panel.add_meshes":
    ["⊞ Add meshes…",
     "Right column, the button that adds several meshes at once."],
  "panel.add_meshes_tip":
    ["Pick several .nif files at once and add one slot per mesh",
     "Right column, hovering the Add meshes button."],
  "panel.even":
    ["≡ Even weights",
     "Right column, the button that levels every weight."],
  "panel.even_tip":
    ["Set every slot to the same chance",
     "Right column, hovering the Even weights button."],
  "panel.slots_hint":
    ["Weights, not percentages — they need not add up to anything. A slot is either a mesh or an object ID from another plugin, never both.",
     "Right column, under the grass cards."],
  "panel.clumping_hint":
    ["<b>A Gardenfell addition.</b> Groundcover Generator has no key for it, so it lives in the rules file and shapes both the preview and the export. A plain groundcover .ini read elsewhere gets the even scatter back.",
     "Right column, at the top of the Organic clumping section."],
  "panel.clumping_on":
    ["Enabled",
     "Right column, the Enabled label under Organic clumping."],
  "panel.clumping_on_tip":
    ["Gate placement through smooth noise",
     "Right column, hovering the Enabled label."],
  "panel.patchwidth":
    ["Patch width",
     "Right column, the Patch width label."],
  "panel.patchwidth_tip":
    ["How wide one patch of grass runs. 8192 is a whole cell.",
     "Right column, hovering the Patch width label."],
  "panel.cover":
    ["Coverage",
     "Right column, the Coverage label."],
  "panel.cover_tip":
    ["How much of the eligible ground keeps grass",
     "Right column, hovering the Coverage label."],
  "panel.edge":
    ["Edge softness",
     "Right column, the Edge softness label."],
  "panel.edge_tip":
    ["0 cuts a hard boundary; higher lets patches fray",
     "Right column, hovering the Edge softness label."],
  "panel.warp":
    ["Warp",
     "Right column, the Warp label."],
  "panel.warp_tip":
    ["0 gives round blobs; higher gives winding runs",
     "Right column, hovering the Warp label."],
  "panel.clumping_hint2":
    ["Patches are anchored to the world, so they run across cell borders rather than restarting at each one.",
     "Right column, at the foot of the Organic clumping section."],
  "panel.bans_hint":
    ["Ground more than half covered by a banned texture takes nothing; the offset keeps grass that many units clear.",
     "Right column, at the foot of the Exclusions section."],
  "panel.bans_hint_title":
    ["Land textures blend into each other, so a blade can land on ground that reads mostly as road. Turn on Ban strip in Simplified to watch a ban carve a road.",
     "Right column, hovering that hint."],
  "preview.mode_hint_simple":
    ["A synthetic patch: instant, good for judging density and clustering.",
     "Left column, under the mode picker while Simplified is chosen."],
  "preview.mode_hint_cell":
    ["The real landscape and every placed object. Slower; the only way to see clipping.",
     "Left column, under the mode picker while Real cell is chosen."],
  "vp.nav_wasd_text":
    ["WASD navigation",
     "Viewport, the navigation button's own words while the WASD scheme is on. The Orbit wording is the button's markup (vp.nav_orbit)."],
  "preview.fogd_from":
    ["fog from",
     "Left column, on the sources line after Reset to install — before the file the fog distances came from."],
  "preview.sky_src":
    ["Sky:",
     "Left column, under the Weather picker — the lead-in before the list of files the sky's colours were read from."],
  "preview.sky_src_none":
    ["Sky: the game's own colours, until an install is connected.",
     "Left column, under the atmosphere rows while nothing is connected."],
  "preview.sky_src_builtin":
    ["no weather in the ini, the game's own colours",
     "Left column, under the atmosphere rows — the ini or openmw.cfg had no [Weather] blocks."],
  /* Round 17m: the file is named, because it is no longer always MGE.ini — G7's fork of
     MGE XE keeps everything in mgeXE.toml — and because a detector whose guess is
     invisible is how the wrong file went unnoticed for three months. */
  "preview.sky_src_mge2":
    ["{file} fog",
     "Left column, under the atmosphere rows — fog distances, and the file they were read from (mgeXE.toml or mge3\\MGE.ini)."],
  "preview.sky_src_mge_how":
    ["how the fog file was chosen: {how}",
     "Left column, the title (hover text) on the sources line — why that MGE config file was the one read."],
  "preview.sky_src_mge_default":
    ["MGE's default fog",
     "Left column, under the atmosphere rows — no MGE.ini, MGE XE's built-in fog distances."],
  /* Round 18i: the renderer the install runs decides how the scene is lit - MGE XE's
     per-pixel lighting, the fixed pipeline, or OpenMW's shaders - and the sources line
     names it. Robin: "If G7's fork of MGE XE is installed, always check the mgeXE.toml as
     well for relevant entries [...] no matter what version of MGE XE they use, or OpenMW." */
  "preview.sky_src_renderer":
    ["{label} lighting",
     "Left column, under the atmosphere rows — the renderer whose lighting model the preview follows, by its own name (e.g. 'MGE-XE G7 Fork v0.20.0', 'OpenMW', 'Morrowind.exe, no MGE')."],
  "preview.renderer_model_mge":
    ["MGE XE per-pixel lighting: 1/(q·d² + c) per lamp, the linear term dropped as its shader drops it; the lit colour tonemapped, not clamped; the grass takes sun and ambient only.",
     "Hover text on the sources line — what the MGE lighting model is."],
  "preview.renderer_model_ffp":
    ["fixed pipeline (per vertex in the game): 1/(c + l·d + q·d²) per lamp, the sum clamped to white.",
     "Hover text on the sources line — what the fixed-pipeline lighting model is (vanilla, MGE with per-pixel lighting off, OpenMW's legacy method)."],
  "preview.renderer_model_openmw":
    ["OpenMW shaders: 1/(c + l·d + q·d²) per lamp, capped at 1, faded out between {bounds}× the radius and twice that.",
     "Hover text on the sources line — what the OpenMW shader lighting model is; {bounds} is its light bounds multiplier."],
  "preview.renderer_att":
    ["[LightAttenuation] from {file}: constant {c}, linear {l}, quadratic {q} (a 256-unit lamp: c {c256}, l {l256}, q {q256})",
     "Hover text on the sources line — the light attenuation settings read, the file they came from, and the coefficients they give a 256-unit lamp."],
  "preview.renderer_att_default":
    ["[LightAttenuation] not in {file}: the game's shipped values (linear 3/r)",
     "Hover text on the sources line — no [LightAttenuation] section was found, so the shipped values are used."],
  "preview.renderer_post":
    ["post shaders: {list}",
     "Hover text on the sources line — the post-process shaders the install's chain names that the preview has an answer for (SSAO, sunshafts…), or 'none'."],
  "avoid.remove":
    ["Yes, remove from list",
     "Never-avoid dialogue, the button when it will remove the pattern instead."],
  "panel.cell_hint_painted":
    ["The list is narrowed to where <code>{texture}</code> is actually painted: <b>{regions}</b> region(s) and <b>{cells}</b> named cell(s), read from the landscape. Anywhere else would place nothing.",
     "Right column, under Cell / region when the texture is painted somewhere. {texture}, {regions} and {cells} are filled in."],
  "panel.cell_hint_wide":
    ["The file format does not distinguish a cell from a region — the generator tries <b>cell name first, then region name</b>. The list offers {regions} region(s) and {cells} named exterior cell(s) from your load order{tail}",
     "Right column, under Cell / region when the texture is not painted anywhere yet. {regions} and {cells} count the whole load order; {tail} is one of the two lines below."],
  "panel.cell_hint_wide_done":
    [", though this texture was not found painted anywhere.",
     "Right column, the end of the line above once the landscape has been read."],
  "panel.cell_hint_wide_pending":
    ["; the landscape pass will narrow it per texture.",
     "Right column, the end of the same line while the landscape is still being read."],
  "panel.cell_hint_offline":
    ["The file format does not distinguish a cell from a region — the generator tries <b>cell name first, then region name</b>. Case does not matter, and most Morrowind regions end in <code>&nbsp;Region</code> (e.g. <code>Bitter Coast Region</code>). Connect your setup to get a checked list instead of guesswork.",
     "Right column, under Cell / region with no install connected."],
  /* Round 18ad: the two words beside the lock in the mesh editor. Both here rather than
     one in the markup, because `syncMeshLock` writes over that very element - which is
     the trap 18aa fell into with the connect buttons: a key whose wording is read back
     off an element a script rewrites answers with whatever the script last put there. */
  "statics.lock_open":
    ["Unlocked",
     "The mesh editor's corner, beside the open lock."],
  "statics.lock_shut":
    ["Locked",
     "The mesh editor's corner, beside the shut lock."],
  /* Round 18ap: keyed for the language packs (wording-plan.md) — what 11_events.js says. */
  "paint.swatch_repoint":
    ["Point it at another rule",
     "The colour dialogue for a rule colour: the option that changes which rule it stands for."],
  "paint.swatch_repoint_note":
    ["the ground you painted stays where it is",
     "Under that option."],
  "paint.swatch_move_to":
    ["Move to {layer}",
     "The colour dialogue: the option that moves the colour to another layer; {layer} is the layer's name."],
  "paint.swatch_move_note":
    ["takes the ground painted with it along",
     "Under that option."],
  "paint.layer_default":
    ["Layer {id}",
     "What a paint layer is called until it is renamed; {id} is its number."],
  "paint.swatch_clear_view_note":
    ["its paint comes off the loaded cells; the colour itself stays",
     "Under the clear-in-view option of a rule colour."],
  "paint.swatch_remove":
    ["Remove this colour",
     "The colour dialogue: the option that deletes the colour."],
  "paint.and_its_paint":
    ["and the paint with it",
     "Under that option, and under \"Remove it\" in the confirming dialogue."],
  "paint.swatch_body":
    ["A colour is attached to a rule, not to its name — so renaming the rule, or pointing it at other ground, changes nothing here. Pointing the <i>colour</i> at a different rule changes what grows on everything you have painted with it.<br><br>Moving it to another layer takes the ground painted with it — where that layer already has a colour on the same spot, this one wins, the same as painting over it would.",
     "The colour dialogue for a rule colour: what the options do."],
  "paint.swatch_remove_title":
    ["Remove this colour?",
     "Heading of the dialogue confirming a colour's removal."],
  "paint.swatch_remove_body":
    ["<b>{label}</b> — the ground painted with it goes back to whatever rule its texture would have used.",
     "That dialogue's body; {label} is the colour."],
  "paint.remove_it":
    ["Remove it",
     "The confirming option in the remove-colour and remove-layer dialogues."],
  "paint.swatch_clear_view_body":
    ["Every mark of <b>{label}</b> on {where} — the ground and the objects standing there. The colour itself stays on its layer, and so does its paint elsewhere.",
     "The clear-in-view dialogue's body for a rule colour; {label} is the colour, {where} the cells."],
  "paint.only_layer":
    ["This is the only layer — remove its colours instead.",
     "Toast when the last paint layer is asked to be removed."],
  "paint.layer_remove_title":
    ["Remove this layer?",
     "Heading of the dialogue confirming a paint layer's removal."],
  "paint.layer_remove_body.one":
    ["<b>{name}</b> — {n} colour, and every patch of ground painted with any of them. The other layers are not touched.",
     "That dialogue's body for a layer with one colour; {name} is the layer."],
  "paint.layer_remove_body.other":
    ["<b>{name}</b> — {n} colours, and every patch of ground painted with any of them. The other layers are not touched.",
     "The same for several colours; {n} is how many."],
  "dlg.keep_it":
    ["Keep it",
     "The option that changes nothing, in several confirming dialogues."],
  "paint.and_paint_on_it":
    ["and the paint on it",
     "Under \"Remove it\" in the remove-layer dialogue."],
  "paint.layer_unhidden":
    ["That colour’s layer was hidden — shown again so you can see what you paint.",
     "Toast when painting with a colour on a hidden layer turns the layer visible."],
  "paint.crowd_title":
    ["Warning, this object is approaching a limit of how much paint it can contain.",
     "Heading of the dialogue that offers to compact a heavily painted object."],
  "paint.crowd_body":
    ["<b>{name}</b> holds {marks} marks of its {room}. Past that, the paint stays in the mask and the grass still grows from all of it, but the colour on screen stops showing the oldest of it. Compacting drops marks that mostly repeat ones already there — it is not free, and fine detail can soften where strokes overlap.",
     "That dialogue's body; {name} is the mesh, {marks} how many it holds, {room} how many it can."],
  "paint.crowd_leave":
    ["Leave it as it is",
     "That dialogue's option to do nothing."],
  "paint.crowd_leave_note":
    ["Nothing changes; you will not be asked again about this object",
     "Under that option."],
  "paint.crowd_compact":
    ["Compact this object’s paint",
     "That dialogue's option to compact."],
  "paint.crowd_compact_note":
    ["Fewer marks, the same picture almost everywhere — undoable",
     "Under that option."],
  "paint.compacted":
    ["Compacted {name}: {was} marks to {now}.",
     "Toast after an object's paint was compacted; {name} is the mesh, {was} and {now} the mark counts."],
  "paint.cleared":
    ["Cleared {what}",
     "Toast after clearing the paint in view or everywhere; {what} is \"3 painted cells and 2 painted objects\"."],
  "paint.n_painted_cells.one":
    ["{n} painted cell",
     "Part of that toast."],
  "paint.n_painted_cells.other":
    ["{n} painted cells",
     "The same for several."],
  "paint.n_painted_objects.one":
    ["{n} painted object",
     "Part of that toast."],
  "paint.n_painted_objects.other":
    ["{n} painted objects",
     "The same for several."],
  "paint.no_paint_in_view":
    ["No paint in view",
     "Toast when Clear paint in view found nothing."],
  "paint.nothing_to_clear":
    ["Nothing to clear",
     "Toast when Clear every painted area found nothing."],
  "paint.clear_view_all_title":
    ["Clear the paint in view?",
     "Heading of the dialogue behind Clear paint in view."],
  "paint.clear_view_all_body":
    ["Everything painted on {where}. Ground you have painted elsewhere keeps it, and so do your colours and layers.",
     "That dialogue's body; {where} is the cells."],
  "dlg.nothing_changes":
    ["Nothing changes",
     "Under \"Keep it\" and \"Cancel\" in several confirming dialogues."],
  "paint.clear_all_in_view":
    ["Clear all paint in view",
     "That dialogue's confirming option."],
  "paint.paint_goes_rules_stay":
    ["The paint goes; the grass rules are untouched",
     "Under the clearing option in the two clear-paint dialogues."],
  "paint.clear_every_title":
    ["Clear every painted area?",
     "Heading of the dialogue behind Clear every painted area."],
  "paint.clear_every_body":
    ["Every cell you have painted in this profile, not just the one you are looking at.",
     "That dialogue's body."],
  "paint.clear_every_option":
    ["Clear ALL paint in every cell",
     "That dialogue's confirming option."],
  "dlg.sure_title":
    ["Are you sure?",
     "Heading of the second confirmation before clearing every painted area."],
  "paint.clear_every_sure_body":
    ["Every stroke in every cell of this profile. Undo can still take it back while the program is open.",
     "That second confirmation's body."],
  "paint.yes_clear":
    ["Yes, clear",
     "The second confirmation's confirming option."],
  "paint.every_painted_cell_goes":
    ["Every painted cell goes",
     "Under that option."],
  "dlg.cancel":
    ["Cancel",
     "The option that changes nothing, in the second confirmation and the paste dialogue."],
  "rules.copied":
    ["Copied",
     "Toast after the rules TOML was copied to the clipboard."],
  "file.saved":
    ["Saved {name}",
     "Toast after a file was written through the Save dialogue; {name} is the file."],
  "file.write_failed":
    ["Could not write that file: {err}",
     "Toast when a Save failed; {err} is the reason."],
  "top.connected_title":
    ["Connected — click to connect a different {kind} install",
     "Top bar, the tooltip on the connect button of the install that is live; {kind} is \"MO2\", \"OpenMW\" or \"Data Files\"."],
  "connect.switch_title":
    ["Connect a different setup?",
     "Heading of the dialogue asked before an install replaces the loaded one."],
  "connect.switch_body":
    ["This reads {what} from the beginning — every plugin, every landscape and every mesh it needs. Your grass rules and your profile stay exactly as they are; the cell you are looking at does not.",
     "That dialogue's body; {what} names the install kind."],
  "connect.switch_undo":
    ["<br><br><b>Your undo history goes with it</b> — {n} step(s) back. It is a history of edits made against the install you are leaving, so it cannot be replayed onto another one. Everything already saved stays saved.",
     "That dialogue's body, appended when connecting a different install would throw an undo history away; {n} is how many steps."],
  "connect.switch_yes":
    ["Choose a setup…",
     "That dialogue's option to go on."],
  "connect.switch_yes_note":
    ["Nothing is unloaded until you have picked one",
     "Under that option."],
  "connect.switch_no":
    ["Stay where I am",
     "That dialogue's option to keep the loaded install."],
  "connect.switch_no_note":
    ["Keep the setup that is loaded",
     "Under that option."],
  "intro.title":
    ["Connect your install",
     "The welcome card over the viewport at first start: its heading."],
  "intro.lead":
    ["Pick the one that matches how you play. Everything is read-only &mdash; nothing is written to your game folders. You can connect <b>one at a time</b>.",
     "The welcome card: the line under the heading."],
  "intro.vanilla":
    ["Connect a vanilla install",
     "The welcome card: the first button."],
  "intro.vanilla_sub":
    ["A plain Data Files folder — vanilla, MGE XE, or manually installed mods",
     "Under that button."],
  "intro.mo2":
    ["Connect an MO2 install",
     "The welcome card: the second button."],
  "intro.mo2_sub":
    ["Reads modlist.txt and the profile’s Morrowind.ini for the real overlay order",
     "Under that button."],
  "intro.omw":
    ["Connect an OpenMW install",
     "The welcome card: the third button."],
  "intro.omw_sub":
    ["Reads openmw.cfg for the data= directories and content= plugin order",
     "Under that button."],
  "intro.then":
    ["Then <b>open a .toml</b>, <b>import an .ini</b>, or add a rule, and pick a ground texture on the left. <b>Simplified</b> previews the selected rule on a test patch; <b>Real cell</b> draws every rule that matches the ground in that cell, which is what the export does.<br><br>Terrain shape, ground height, patch size and the ban strip are <b>preview stand-ins</b> &mdash; the generator reads your landscape instead. Everything under <b>Obstacle avoidance</b> changes the .esp; everything under <b>Preview</b> changes only what you are shown.",
     "The welcome card: the paragraph under the buttons."],
  "intro.skip":
    ["Skip for now",
     "The welcome card: the button that closes it."],
  "import.cancelled":
    ["Import cancelled",
     "Toast when the import dialogue was closed without a choice."],
  "rules.opened.one":
    ["Opened {name} — {n} rule",
     "Toast after a rules file was opened; {name} is the file, {n} how many rules it holds."],
  "rules.opened.other":
    ["Opened {name} — {n} rules",
     "The same for several rules."],
  "rules.opened_with_profile":
    [", including the settings it was saved with",
     "Added to that toast when the file carried a profile too."],
  "import.added":
    ["{n} added",
     "Part of the toast after an .ini import: rules added."],
  "import.layered":
    ["{n} layered onto existing rules",
     "Part of that toast: rules layered onto rules already there."],
  "import.replaced":
    ["{n} replaced",
     "Part of that toast: rules that replaced existing ones."],
  "import.skipped":
    ["{n} skipped",
     "Part of that toast: rules left out."],
  "import.nothing":
    ["nothing",
     "What stands in for the parts above when none applied."],
  "import.head.one":
    ["Imported {name} — {bits}. {n} rule in total",
     "The toast after an .ini import; {name} is the file, {bits} the parts above joined by commas, {n} the rules now loaded."],
  "import.head.other":
    ["Imported {name} — {bits}. {n} rules in total",
     "The same for several rules."],
  "import.objects_n.one":
    [". {n} slot names an object from the load order instead of a mesh — kept; its references go into the Objects .esp on export",
     "Added to that toast when one imported slot named an object ID."],
  "import.objects_n.other":
    [". {n} slots name an object from the load order instead of a mesh — kept; their references go into the Objects .esp on export",
     "The same for several slots."],
  "import.check_failed":
    ["{head}. The name check could not run: {err}",
     "The import toast when the load-order check could not run; {head} is the toast so far, {err} the reason."],
  "import.unknown":
    ["{head}, but {n} match nothing in your load order. Click the warning in the top bar.",
     "The import toast when {n} imported textures are not in the load order; {head} is the toast so far."],
  "import.plan_add_sub.one":
    ["spacing {gap} · {n} mesh",
     "The import dialogue, under a rule that will be added; {gap} is its spacing, {n} its meshes."],
  "import.plan_add_sub.other":
    ["spacing {gap} · {n} meshes",
     "The same for several meshes."],
  "import.plan_clash_sub":
    ["incoming: spacing {gap}, {slots} slot(s)   ·   here already: {existing} rule(s)",
     "The import dialogue, under a rule that clashes with one already loaded."],
  "esp.filter_none":
    ["Nothing here matches the filter.",
     "The export dialogue's rule list when the filter leaves nothing."],
  "esp.no_rules":
    ["No rules to export.",
     "The export dialogue's rule list when there are no rules."],
  "esp.and_more":
    ["…and {n} more",
     "The export dialogue's warning list, after the first ten; {n} is how many are not shown."],
  "esp.no_problems":
    ["No problems found. {rules} rule(s), {slots} slot(s).",
     "The export dialogue's check, when nothing is wrong."],
  /* Round 18bv: the export report. Robin: "Why not make a new report button that appears
     in the message box after an export ... On pressing it, we open up an export report
     dialogue which is similar in design and functionality as the report button opens from
     the top panel."

     Headings and the button only. The lines under them are developer diagnostics and stay
     English by design, the same rule the Loading and Performance sections already follow
     (wording-plan.md, "What is in scope") — and here there is a second reason for it: the
     same numbers come out of `GardenfellCLI bench` in English, and the whole point of the
     dialogue is that the two can be compared side by side. */
  "esp.report_btn":
    ["Export report",
     "The button on the export summary that opens the detailed report for the run just finished."],
  "esp.report_btn_title":
    ["Timings, counts and the plugin's fingerprint for this export",
     "That button's tooltip."],
  "report.export_title":
    ["Export report",
     "The title of the export report dialogue."],
  "report.export_timing":
    ["Timing",
     "Export report: the timings heading."],
  "report.export_output":
    ["What was written",
     "Export report: the heading over the reference and byte counts."],
  "report.export_darts":
    ["Placement",
     "Export report: the heading over how many candidate positions were tried and kept."],
  "report.export_rejected":
    ["Why positions were refused",
     "Export report: the heading over the per-reason rejection counts."],
  "report.export_obstacles":
    ["Obstacles",
     "Export report: the heading over what grass was kept clear of."],
  "report.export_none":
    ["No export has run yet in this session.",
     "Export report, when it is opened with nothing to show — which the button should make impossible."],
  "report.load_order":
    ["Plugin load order",
     "Load-order report: the first heading."],
  "report.masters":
    ["masters: {list}",
     "Load-order report, under a plugin: its masters; {list} is them, joined by dots."],
  "report.not_loaded":
    [" (NOT LOADED)",
     "After a master in that list that the order does not load."],
  "report.index_warnings":
    ["Could not read while indexing objects: {list}",
     "Load-order report: the warning box when some object records could not be read; {list} is the reasons."],
  "report.missing_plugins":
    ["<b>{n} plugin(s) listed but not found</b> through the current stack: {list}",
     "Load-order report: the warning box for plugins the order names but the install lacks; {list} is up to eight of them."],
  "report.assets":
    ["Assets",
     "Load-order report: the assets heading."],
  "report.assets_ok":
    ["Every referenced mesh and texture loaded cleanly.",
     "Load-order report, under Assets, when nothing was missing."],
  "report.loading":
    ["Loading",
     "Load-order report: the timings heading."],
  "report.performance":
    ["Performance",
     "Load-order report: the performance heading."],
  "report.tint":
    ["Ground tint",
     "Load-order report: the ground tint heading."],
  "report.tint_slider":
    ["tint slider {pct}%",
     "Load-order report, under Ground tint; {pct} is the slider."],
  "report.tint_off":
    [" — off, so nothing below is read by the grass",
     "After that line while the tint is off."],
  "report.textures":
    ["Textures",
     "Load-order report: the textures heading."],
  "vp.noengine_title":
    ["The cell viewer engine is not running",
     "The viewport when the page is opened in a plain browser: the big line."],
  "vp.noengine_body":
    ["This page is the front half of Wraithguard&rsquo;s cell viewer. Open it from Wraithguard&rsquo;s Cell Preview.",
     "Under that line."],
  "top.engine_title":
    ["native engine · {n} worker threads",
     "Top bar, the tooltip on the folder state; {n} is the engine's thread count."],
  "connect.reopen_failed":
    ["Could not reopen {path} — connect an install to carry on. {err}",
     "Toast at start-up when the last install could not be opened; {path} is it, {err} the reason."],
  "rules.start_failed":
    ["Could not open your grass rules — starting from an empty set. {err}",
     "Toast at start-up when the grass rules store could not be read."],
  "profiles.start_failed":
    ["Could not open your profiles — starting from defaults. {err}",
     "Toast at start-up when the profile store could not be read."],
  /* Round 18ap: keyed for the language packs (wording-plan.md) — what 11_events.js says, first half. */
  "vp.noctx_title":
    ["3D preview unavailable",
     "The viewport, the big line when WebGL could not start."],
  "vp.noctx_body":
    ["The rule editor and .toml export still work — only the viewport is affected.",
     "The viewport, under that line, after the browser's own reason."],
  "file.read_failed":
    ["Could not read that file: {err}",
     "Toast when a dropped or opened file could not be read; {err} is the reason."],
  "paint.surfaces_none":
    ["Nothing here can take paint yet — none of this {place}’s meshes could be read.",
     "Toast after \"Paint on statics too\" read the scene and found no mesh it could use; {place} is \"room\" or \"cell\"."],
  "paint.place_room":
    ["room",
     "The word that fills {place} in the toast above, inside an interior."],
  "paint.place_cell":
    ["cell",
     "The word that fills {place} in the toast above, in an exterior cell."],
  "paint.surfaces_unreadable.one":
    ["{n} mesh here could not be read, so the brush cannot land on it.",
     "Toast after reading the scene's meshes for painting: one could not be read."],
  "paint.surfaces_unreadable.other":
    ["{n} meshes here could not be read, so the brush cannot land on them.",
     "The same toast for several meshes; {n} is how many."],
  "paint.surfaces_ready.one":
    ["Ready — {n} mesh in view can take paint.",
     "Toast when the scene's meshes have been read for painting and one is ready."],
  "paint.surfaces_ready.other":
    ["Ready — {n} meshes in view can take paint.",
     "The same for several; {n} is how many."],
  "paint.surfaces_failed":
    ["Could not read the meshes here: {err}",
     "Toast when reading the scene's meshes for painting failed; {err} is the reason."],
  "paint.occlude_off":
    ["off",
     "Left column, Painting: the value beside the occlusion-reach slider while the switch is off."],
  "paint.needs_cell":
    ["Painting needs a real cell — switch to Real cell preview first.",
     "Toast when painting is asked for over the simplified patch."],
  "paint.colour_no_rule":
    ["These grass rules have no {rule} — the colour is waiting for it.",
     "Toast on clicking a colour whose rule is not in the loaded grass rules; {rule} is the rule's name."],
  "paint.such_rule":
    ["such rule",
     "What fills {rule} in the toast above when the colour remembers no name."],
  "paint.erase_rule_title":
    ["Take this layer’s colours off, and leave the other layers alone",
     "Left column, Painting: the tooltip on a layer's Erase rule button."],
  "paint.colour_needs_rule":
    ["Add a rule first — a colour stands for one.",
     "Toast on \"+ Colour\" with no grass rules loaded."],
  "paint.colour_add_failed":
    ["Could not add that colour: {err}",
     "Toast when the engine refused a new colour; {err} is the reason."],
  "paint.colour_no_rules":
    ["There are no rules to point it at.",
     "Toast on \"Point it at another rule\" with no grass rules loaded."],
  "paint.colour_same_rule":
    ["That is already the rule this colour stands for.",
     "Toast when the rule picked for a colour is the one it already has."],
  "paint.colour_repointed":
    ["{label} now grows {rule} — the ground you painted is untouched.",
     "Toast after a colour was pointed at another rule; {label} is the colour's, {rule} the rule's name."],
  "paint.colour_change_failed":
    ["Could not change that colour: {err}",
     "Toast when repointing a colour failed; {err} is the reason."],
  "paint.where_preview":
    ["Preview",
     "The colour dialogue's list of cells a colour is painted in: the button on each row."],
  "paint.where_preview_title":
    ["Load {cell} in Real cell preview",
     "That button's tooltip; {cell} is the row's cell."],
  "paint.colour_title":
    ["What should happen to {label}?",
     "The colour dialogue's heading, for a base colour or a rule colour; {label} is the colour's name."],
  "paint.cover_body_grow":
    ["<b>Grass</b> is painted ground that grows whatever its rule says <i>whatever else would stop it</i> — slope, height, objects, exclusions, and any No grass under it. It is one plane for the cell, not one per layer, so clearing it here clears it for every layer at once.",
     "The colour dialogue for the green base colour: what it is."],
  "paint.cover_body_bare":
    ["<b>No grass</b> is painted ground where nothing grows, whatever the rules say. It is one plane for the cell, not one per layer, so clearing it here clears it for every layer at once.",
     "The colour dialogue for the red base colour: what it is."],
  "paint.cover_clear_view":
    ["Clear all paint of this colour in view",
     "The colour dialogue, the option that clears this colour off the loaded cells."],
  "paint.cover_clear_view_note":
    ["the loaded cells and the objects standing in them; its paint elsewhere stays",
     "Under that option."],
  "paint.cover_clear_all":
    ["Clear all paint of this colour everywhere",
     "The colour dialogue, the option that clears this colour from every cell."],
  "paint.cover_clear_all_note":
    ["every mark of this colour, in every cell and on every object — not only what is in view",
     "Under that option."],
  "paint.load_cell_first":
    ["Load a cell first.",
     "Toast when a clear-in-view is asked for with no cell on screen."],
  "paint.this_interior":
    ["this interior",
     "The clear-in-view dialogues, standing in for an interior with no name."],
  "paint.where_cells":
    ["the <b>{n} cells</b> the viewport is showing",
     "The clear-in-view dialogues, the \"where\" when several cells are loaded; {n} is how many."],
  "paint.clear_view_title":
    ["Clear this colour in view?",
     "Heading of the dialogue that clears one colour off the loaded cells."],
  "paint.cover_clear_view_body":
    ["Every mark of <b>{label}</b> on {where} — the ground and the objects standing there. Its paint elsewhere stands, and so does every rule colour here.",
     "That dialogue's body for a base colour; {label} is the colour, {where} the cells."],
  "paint.these_cells":
    ["This colour, these cells",
     "Under the option that clears one colour in view."],
  "paint.cover_clear_all_title":
    ["Clear all {label} paint everywhere?",
     "Heading of the dialogue that clears a base colour from every cell; {label} is the colour."],
  "paint.cover_clear_all_body":
    ["Every mark of <b>{label}</b> in every cell and on every object, not only the ones in view. The rule colours are untouched.<br><br>It is one undo away if this was not what you meant.",
     "That dialogue's body."],
  "paint.every_cell_object":
    ["Every cell, every object",
     "Under the option that clears a colour everywhere."],
  "paint.cleared_from":
    ["Cleared {label} from {what}.",
     "Toast after clearing a colour; {label} is the colour, {what} is \"3 cells and 2 objects\" built from the two entries below."],
  "paint.n_cells.one":
    ["{n} cell",
     "Part of a \"Cleared … from …\" toast: how many cells."],
  "paint.n_cells.other":
    ["{n} cells",
     "The same for several."],
  "paint.n_objects.one":
    ["{n} object",
     "Part of a \"Cleared … from …\" toast: how many objects."],
  "paint.n_objects.other":
    ["{n} objects",
     "The same for several."],
  "paint.and":
    [" and ",
     "Joins the cells and the objects in a \"Cleared … from …\" toast."],
  "paint.nothing_to_clear_of":
    ["No {label} paint to clear.",
     "Toast when a colour had no paint anywhere; {label} is the colour."],
  "paint.no_paint_in_view_of":
    ["No {label} paint in view.",
     "Toast when a colour had no paint on the loaded cells; {label} is the colour."],
  "paint.clear_failed":
    ["Could not clear that: {err}",
     "Toast when a clear of paint failed; {err} is the reason."],
  "paint.colour_move_failed":
    ["Could not move that colour: {err}",
     "Toast when moving a colour to another layer failed."],
  "paint.colour_remove_failed":
    ["Could not remove that colour: {err}",
     "Toast when removing a colour failed."],
  "paint.layer_add_failed":
    ["Could not add that layer: {err}",
     "Toast when adding a paint layer failed."],
  "paint.layer_remove_failed":
    ["Could not remove that layer: {err}",
     "Toast when removing a paint layer failed."],
  "paint.could_not":
    ["Could not do that: {err}",
     "Toast when a layer switch could not be set."],
  "paint.layer_rename_failed":
    ["Could not rename that layer: {err}",
     "Toast when renaming a paint layer failed."],
  "paint.not_ready":
    ["Not ready yet.",
     "Toast on \"+ Paint rule\" before the page has finished starting."],
  "paint.paint_failed":
    ["Could not paint: {err}",
     "Toast when a stroke on a mesh in the editor failed."],
  "paint.stroke_failed":
    ["Could not record that stroke: {err}",
     "Toast when the end of a stroke could not be filed."],
  "paint.compact_failed":
    ["Could not compact that: {err}",
     "Toast when compacting an object's paint failed."],
  "paint.clear_failed_short":
    ["Could not clear: {err}",
     "Toast when clearing the paint in view or everywhere failed."],
  /* Round 18ap: the paint layer row's own strings, which had keys with inline fallbacks and no catalogue entries. */
  "paint.layer_name_title":
    ["Click to work on this layer; double-click to rename it",
     "Left column, Painting: the tooltip on a layer's name."],
  "paint.layer_count.one":
    ["{n} colour",
     "Left column, Painting: beside a layer's name, how many colours it has."],
  "paint.layer_count.other":
    ["{n} colours",
     "The same for several (and for none)."],
  "paint.layer_over_title":
    ["Replacing: this layer’s colours put their rule where the ground texture’s own rule would have grown. Click to have both grow instead.",
     "Left column, Painting: the tooltip on a layer's replace/add mark while it replaces."],
  "paint.layer_mode_add_title":
    ["Adding: the ground texture’s own rule keeps growing and this layer’s grows as well. Click to replace it instead.",
     "The same mark's tooltip while it adds."],
  "paint.layer_show_title":
    ["Whether this layer’s colour is drawn on the ground. Hiding it does not stop its grass — the preview and the exported plugin are unchanged.",
     "Left column, Painting: the tooltip on a layer's eye."],
  "paint.layer_drop_title":
    ["Remove this layer, its colours and the ground painted with any of them",
     "Left column, Painting: the tooltip on a layer's delete cross."],
  "paint.add_colour":
    ["+ Colour",
     "Left column, Painting: the button that adds a colour to a layer."],
  "paint.add_colour_title":
    ["Make a colour on this layer for one of your grass rules",
     "That button's tooltip."],
  "paint.erase_rule":
    ["Erase rule",
     "Left column, Painting: the button that takes a layer's colours off the ground."],
  /* Round 18ap: keyed for the language packs — the mesh editor, the twins, the statics export and import (18_cellpreview.js). */
  "mesh.unassign_title":
    ["Take the rule off this texture?",
     "Mesh editor: heading of the dialogue that removes a rule from one of the mesh's textures."],
  "mesh.unassign_body":
    ["<b>{texture}</b> stops scattering <b>{rule}</b> on this mesh. Paint is untouched, and other meshes using the texture keep their own assignments.",
     "That dialogue's body; {texture} is the texture, {rule} the rule."],
  "mesh.unassign_go":
    ["Take it off",
     "That dialogue's confirming option."],
  "mesh.unassign_note":
    ["This mesh only",
     "Under that option."],
  "mesh.locked_corner":
    ["This mesh is locked — the corner glyph opens it.",
     "Mesh editor: toast on trying to change a locked mesh's setup."],
  "mesh.no_textures":
    ["This mesh names no textures.",
     "Mesh editor: the texture panel when the mesh has none."],
  "connect.first":
    ["Connect an install first.",
     "Toast when something needs an install and none is connected."],
  "mesh.assign_failed":
    ["Could not assign that: {err}",
     "Mesh editor: toast when assigning a rule to a texture failed."],
  "mesh.stamped.one":
    ["Stamped onto {applied} of {n} ticked mesh",
     "Mesh editor, stamping a rule across the install: the toast; {applied} took it, {n} were ticked."],
  "mesh.stamped.other":
    ["Stamped onto {applied} of {n} ticked meshes",
     "The same for several."],
  "mesh.cleared_off.one":
    ["Cleared off {applied} of {n} ticked mesh",
     "The same toast when the stamp cleared a rule instead."],
  "mesh.cleared_off.other":
    ["Cleared off {applied} of {n} ticked meshes",
     "The same for several."],
  "mesh.stamp_skipped":
    [" — locked and skipped: {list}",
     "Added to that toast for the locked meshes the stamp left alone; {list} is them."],
  "retex.reading_n":
    ["Reading the install’s meshes — {done} of {total}",
     "Mesh editor, the retextures panel while the install is scanned; {done} of {total} meshes read."],
  "retex.reading":
    ["Reading the install’s meshes…",
     "The same before the count is known."],
  "retex.matching":
    ["Matching the meshes to this texture…",
     "The same while the scan's result is matched."],
  "retex.search_failed":
    ["Could not search: {err}",
     "Mesh editor: toast when the retexture search failed."],
  "retex.none":
    ["No mesh in your install names this texture.",
     "Mesh editor: toast when the retexture search found nothing."],
  "retex.locked":
    ["That mesh’s setup is locked — unlock it to stamp onto it.",
     "Mesh editor: toast on ticking a locked mesh in the stamp list."],
  "mesh.undid":
    ["Undid an edit on {mesh}.",
     "Mesh editor: toast after Ctrl+Z on a mesh setup; {mesh} is the file."],
  "mesh.redid":
    ["Redid an edit on {mesh}.",
     "The same after Ctrl+Y."],
  "mesh.more_undo":
    ["More to undo on this mesh.",
     "After that toast while steps remain."],
  "mesh.more_redo":
    ["More to redo on this mesh.",
     "The same for redo."],
  "mesh.oldest_step":
    ["That was the oldest step on this mesh.",
     "After that toast at the end of the history."],
  "mesh.newest_step":
    ["That was the newest step on this mesh.",
     "The same for redo."],
  "mesh.lock_failed":
    ["Could not change the lock: {err}",
     "Mesh editor: toast when the lock could not be toggled."],
  "mesh.change_failed":
    ["Could not change that: {err}",
     "Mesh editor: toast when the avoidance switch could not be set."],
  "mesh.clear_paint_title":
    ["Delete the colour on this mesh?",
     "Mesh editor: heading of the dialogue that deletes the mesh's own paint layer."],
  "mesh.clear_paint_body":
    ["Every painted mark on <b>{mesh}</b>’s own layer goes. Texture rules stay, and Undo can bring the paint back while the program is open.",
     "That dialogue's body; {mesh} is the file."],
  "mesh.clear_paint_go":
    ["Delete the colour",
     "That dialogue's confirming option."],
  "mesh.clear_paint_note":
    ["All of this mesh’s paint goes",
     "Under that option."],
  "mesh.clear_paint_failed":
    ["Could not delete the paint: {err}",
     "Mesh editor: toast when deleting the mesh's paint failed."],
  "mesh.clear_title":
    ["Clear all data on this mesh?",
     "Mesh editor: heading of the dialogue that clears the mesh's whole setup."],
  "mesh.clear_body":
    ["<b>{mesh}</b> loses its whole setup — paint, texture rules, insets. Other meshes are untouched, and Undo can bring it back while the program is open.",
     "That dialogue's body; {mesh} is the file."],
  "mesh.clear_go":
    ["Clear this mesh",
     "That dialogue's confirming option."],
  "mesh.clear_note":
    ["Its whole setup goes",
     "Under that option."],
  "statics.clear_all_head":
    ["Clear all stored mesh data?",
     "Heading of the dialogue that clears every mesh setup."],
  "statics.clear_all_body":
    ["Every mesh setup in the working set — paint, texture rules, insets and locks, for every mesh you have ever set up. The whole store, not just what is open. The next step can spare the locked meshes.",
     "That dialogue's body."],
  "statics.keep_everything":
    ["Keep everything",
     "That dialogue's option to do nothing."],
  "statics.clear_all_go":
    ["Clear the data",
     "That dialogue's confirming option."],
  "statics.clear_all_note":
    ["Every mesh setup goes",
     "Under that option."],
  "statics.certain_title":
    ["Absolutely certain?",
     "Heading of the second confirmation before clearing every mesh setup."],
  "statics.certain_body":
    ["All of it goes at once. <b>It is undoable</b>: open the mesh editor and press Ctrl+Z to bring the whole set back while the program is open — but the file on disk is rewritten now.",
     "That confirmation's body."],
  "statics.certain_no":
    ["No — keep it",
     "That confirmation's option to stop."],
  "statics.certain_go":
    ["Yes, clear it all",
     "That confirmation's confirming option."],
  "statics.certain_note":
    ["The store empties",
     "Under that option."],
  "statics.take_locked":
    ["Delete data for locked meshes too",
     "That confirmation's tick box."],
  "statics.take_locked_title":
    ["Unticked, meshes whose setup is locked keep everything.",
     "That tick box's tooltip."],
  "statics.kept_locked.one":
    [" Kept {n} locked mesh.",
     "After the clearing toast, for the locked mesh spared."],
  "statics.kept_locked.other":
    [" Kept {n} locked meshes.",
     "The same for several."],
  "statics.cleared_n.one":
    ["Cleared {n} mesh setup.",
     "Toast after clearing every mesh setup; {n} is how many went."],
  "statics.cleared_n.other":
    ["Cleared {n} mesh setups.",
     "The same for several."],
  "statics.nothing_all_locked":
    ["Nothing cleared — every setup is locked.",
     "That toast when the locks spared everything."],
  "statics.nothing_to_clear":
    ["There was nothing to clear.",
     "That toast when the store was empty."],
  "mesh.no_twins":
    ["No other mesh in your install has this geometry ({n} checked).",
     "Mesh editor, the twins panel when none was found; {n} is how many meshes were compared."],
  "mesh.copy_setup_title":
    ["Copy a setup between this mesh and {mesh} — pick the direction, and re-point any rule on the way over",
     "Mesh editor, the twins panel: the tooltip on a twin's copy button; {mesh} is the twin."],
  "mesh.read_setups_failed":
    ["Could not read the setups: {err}",
     "Mesh editor: toast when the two setups for a copy could not be read."],
  "copyset.title":
    ["Copy a setup across",
     "Heading of the dialogue that copies a setup between a mesh and its twin."],
  "copyset.go":
    ["Copy it across",
     "That dialogue's confirming button."],
  "copyset.give":
    ["Give ⇢ {mesh}",
     "That dialogue's direction button: this mesh's setup onto the twin; {mesh} is the twin."],
  "copyset.take":
    ["⇠ Take from it",
     "That dialogue's other direction button."],
  "copyset.give_title":
    ["This mesh’s setup lands on {mesh}",
     "The give button's tooltip; {mesh} is the twin."],
  "copyset.take_title":
    ["{mesh}’s setup lands on this mesh",
     "The take button's tooltip."],
  "copyset.pick_title":
    ["Pick which of your rules this becomes on the copy",
     "That dialogue: the tooltip on a row's rule picker."],
  "copyset.failed":
    ["Could not copy: {err}",
     "Toast when the copy across failed."],
  "copyset.gave":
    ["Copied onto {mesh}.",
     "Toast after this mesh's setup was copied onto the twin."],
  "copyset.took":
    ["Took {mesh}’s setup.",
     "Toast after the twin's setup was copied onto this mesh."],
  "stexp.nothing_setup":
    ["No mesh is set up yet — there is nothing to export.",
     "Toast on opening the statics export with no setups."],
  "stexp.none":
    ["No mesh setups to export.",
     "The statics export dialogue's list when empty."],
  "stexp.heading":
    ["Mesh setups",
     "The statics export dialogue: the list's heading."],
  "stexp.n_of":
    ["{on} of {total}",
     "Beside that heading: ticked of all."],
  "stexp.nothing_ticked":
    ["<b>Nothing ticked.</b> Tick at least one mesh &mdash; a statics file with no setups in it is not worth sending.",
     "The statics export dialogue's warning when nothing is ticked."],
  "stexp.build_failed":
    ["<b>Could not build the file.</b> {err}",
     "The statics export dialogue's warning when the engine refused; {err} is the reason."],
  "stexp.nothing":
    ["Nothing to export.",
     "Toast on Export with an empty file."],
  "stexp.exported.one":
    ["Exported {n} mesh setup to {name}",
     "Toast after the statics file was written; {name} is the file."],
  "stexp.exported.other":
    ["Exported {n} mesh setups to {name}",
     "The same for several."],
  "stimp.read_failed":
    ["Could not read those files: {err}",
     "Toast when the statics files to import could not be read."],
  "stimp.kept":
    ["{n} kept as yours",
     "Part of the toast after a statics import: setups yours won over."],
  "stimp.locked_kept":
    ["{n} locked and untouched",
     "Part of that toast: locked setups the import left alone."],
  "stimp.swatches.one":
    ["{n} paint colour matched",
     "Part of that toast: a colour made for a matched rule."],
  "stimp.swatches.other":
    ["{n} paint colours matched",
     "The same for several."],
  "stimp.nothing_changed":
    ["nothing changed",
     "What stands in for the parts when none applied."],
  "stimp.head.one":
    ["Imported — {bits}. {n} mesh setup in total.",
     "The toast after a statics import; {bits} is the parts joined by commas, {n} the setups now held."],
  "stimp.head.other":
    ["Imported — {bits}. {n} mesh setups in total.",
     "The same for several."],
  "stimp.failed":
    ["Import failed: {err}",
     "Toast when a statics import failed."],
  "stimp.title":
    ["Import statics rules",
     "Heading of the statics import dialogue."],
  "stimp.skip":
    ["Skip",
     "The statics import dialogue: the button that leaves a mesh's rules unmapped."],
  "stimp.skip_title":
    ["Leave this mesh’s rules as they are — it arrives naming rules you do not have, so give each of them one below.",
     "That button's tooltip."],
  "stimp.remap":
    ["Remap",
     "The statics import dialogue: the button that points a mesh's rules at the chosen ones."],
  "stimp.remap_title":
    ["Point this mesh’s rules at the ones chosen beside them.",
     "That button's tooltip."],
  "stimp.theirs_title":
    ["Theirs: {name} ({rule})",
     "The statics import dialogue: the tooltip on an incoming rule's name; {rule} is its id."],
  "stimp.unnamed":
    ["(unnamed)",
     "Stands in for an incoming rule with no name."],
  "stimp.becomes_title":
    ["Becomes {rule}",
     "The statics import dialogue: the tooltip on the rule an incoming one will become."],
  "stimp.pick_title":
    ["Choose which of your rules this becomes",
     "The statics import dialogue: the tooltip on a name-matched row's picker."],
  "stimp.pick_repair_title":
    ["Choose one of your rules for this",
     "The statics import dialogue: the tooltip on a repair row's picker."],
  "stimp.stays_yours":
    ["🔒 stays yours",
     "The statics import dialogue: the pill on a locked mesh's row."],
  "stimp.stays_yours_title":
    ["This mesh is locked — it takes no changes from any source.",
     "That pill's tooltip."],
  /* Round 18ap: keyed — the mesh editor's own lines, the stamp grid and the copy-setup dialogue. */
  "mesh.statics_switch_title":
    ["Always on while editing a mesh — its own layer is what the brush paints",
     "Left column, Painting: the tooltip on the greyed \"Paint on statics too\" switch while the mesh editor is open."],
  "mesh.unreadable":
    ["<b>Could not read that mesh.</b> {err}",
     "Mesh editor: the line under the viewport when the file would not read; {err} is the reason."],
  "mesh.not_in_install":
    ["It is not in your install.",
     "What fills {err} above when the file is simply absent."],
  "mesh.nothing_to_draw":
    ["<b>Nothing to draw.</b> That file has no visible geometry — some meshes are collision or markers only.",
     "Mesh editor: the line under the viewport for a file with no geometry."],
  "mesh.particles_only.one":
    ["<b>Particle systems only.</b> That file has no geometry of its own; what you see is its {n} system running.",
     "Mesh editor: the line under the viewport for a mesh that is one particle system."],
  "mesh.particles_only.other":
    ["<b>Particle systems only.</b> That file has no geometry of its own; what you see is its {n} systems running.",
     "The same for several."],
  "mesh.summary":
    ["<b>{mesh}</b> — {dims} units, {tris}, {parts}",
     "Mesh editor: the line under the viewport describing the mesh; {dims} is its size, {tris} and {parts} the two counts below."],
  "mesh.n_triangles.one":
    ["{count} triangle",
     "Part of that line; {count} is the number with its thousands separators."],
  "mesh.n_triangles.other":
    ["{count} triangles",
     "The same for several."],
  "mesh.n_parts.one":
    ["{n} part",
     "Part of that line."],
  "mesh.n_parts.other":
    ["{n} parts",
     "The same for several."],
  "mesh.collision_from":
    [" · collision from the {source}",
     "After that line: where the collision shape came from (\"hull\", \"visible mesh\", \"bounds\")."],
  "mesh.no_collision":
    [" · no collision shape",
     "After that line when the mesh has none."],
  "mesh.rule_missing":
    ["rule missing",
     "Mesh editor, a texture's rule strip: the pill when the assigned rule is not loaded."],
  "mesh.scatters_title":
    ["Scatters {rule}",
     "That pill's tooltip; {rule} is the rule."],
  "mesh.rule_missing_title":
    ["Assigned to \"{rule}\", which is not in the loaded rules — click to point it at one of yours",
     "That pill's tooltip when the rule is missing; {rule} is its id."],
  "mesh.unassign_x_title":
    ["Take this rule off the texture — asks first",
     "Mesh editor: the tooltip on the ✕ at a texture's rule."],
  "mesh.inset_none_title":
    ["Grows all the way to the texture’s edges — slide left to pull it back",
     "Mesh editor: the tooltip on a texture's inset slider at zero."],
  "mesh.inset_title":
    ["Pulled {units} units back from the texture’s edges",
     "That tooltip with an inset; {units} is it."],
  "mesh.pick_new":
    ["+ New paint-only rule…",
     "Mesh editor, the rule picker for a texture: the row that makes a new rule."],
  "mesh.pick_new_hint":
    ["name it, assign it, one go",
     "Under that row."],
  "mesh.pick_none":
    ["✕ No rule",
     "That picker: the row that takes the assignment off."],
  "mesh.pick_none_hint":
    ["take the assignment off",
     "Under that row."],
  "mesh.pick_title":
    ["What grows on {texture}?",
     "That picker's heading; {texture} is the texture."],
  "mesh.pick_why":
    ["Every placement of this mesh in the world scatters the chosen rule on the faces wearing this texture.",
     "That picker's explanation."],
  "mesh.pick_now":
    [" Now: {rule}.",
     "Added to that explanation when a rule is already assigned; {rule} is it."],
  "mesh.rule_gone":
    ["{rule} (missing)",
     "A rule id that is not among the loaded rules, wherever one is named."],
  "mesh.n_placements.one":
    ["{n} placement in the world",
     "Mesh editor, the stamp grid: a mesh's badge tooltip, how many times it is placed."],
  "mesh.n_placements.other":
    ["{n} placements in the world",
     "The same for several."],
  "mesh.badge_locked":
    ["its setup is locked",
     "Part of that tooltip."],
  "mesh.badge_setup":
    ["it already carries a setup",
     "Part of that tooltip."],
  "mesh.badge_assigned":
    ["this texture → {rule}",
     "Part of that tooltip; {rule} is what this texture is assigned on that mesh."],
  "dlg.filter_placeholder":
    ["filter…",
     "The placeholder in the filter field of several pickers."],
  "mesh.tick_all":
    ["Tick all",
     "The stamp dialogue: ticks every mesh."],
  "mesh.untick_all":
    ["Untick all",
     "The stamp dialogue: unticks every mesh."],
  "mesh.stamp_title":
    ["Which meshes take {rule}?",
     "The stamp dialogue's heading; {rule} is the rule being stamped."],
  "mesh.that_rule":
    ["that rule",
     "What fills {rule} above when the rule has no name."],
  "mesh.stamp_clear_title":
    ["Which meshes lose the rule?",
     "The stamp dialogue's heading when it clears a rule."],
  "mesh.wearing_title":
    ["Meshes wearing {texture}",
     "The heading of the list of meshes that use a texture; {texture} is it."],
  "mesh.stamp_why":
    ["Every mesh below names {texture}. The choice is stamped onto the ticked ones — a copy each, theirs to diverge from. Locked meshes refuse it.",
     "The stamp dialogue's explanation."],
  "mesh.wearing_why":
    ["Every mesh in your install naming {texture}. Click one to open it in the editor instead.",
     "The meshes-wearing list's explanation."],
  "mesh.index_note":
    ["Index generated first time, and kept thereafter.",
     "Under the progress bar while the install's meshes are indexed."],
  "mesh.stamp_go.one":
    ["Stamp onto {n} mesh",
     "The stamp dialogue's confirming button; {n} is how many are ticked."],
  "mesh.stamp_go.other":
    ["Stamp onto {n} meshes",
     "The same for several."],
  "mesh.stamp_clear_go.one":
    ["Clear off {n} mesh",
     "That button when the stamp clears a rule."],
  "mesh.stamp_clear_go.other":
    ["Clear off {n} meshes",
     "The same for several."],
  "dlg.filter_nothing":
    ["Nothing matches that filter.",
     "What a picker's list says when the filter leaves nothing."],
  "mesh.open_cue":
    ["Open {mesh} in the mesh editor",
     "The link on the undo toast for a mesh setup; {mesh} is the file."],
  "copyset.locked_why":
    ["<b>{mesh} is locked</b> — it cannot receive a copy until its corner glyph is opened. A locked mesh can still be copied <i>from</i>: flip the direction above.",
     "The copy-setup dialogue's explanation when the receiving mesh is locked; {mesh} is it."],
  "copyset.why":
    ["Assignments land on the texture standing in the same place. Re-point any rule below and its painted colour follows; the paint itself stays where it is.",
     "The copy-setup dialogue's explanation."],
  "copyset.only_paint":
    ["Only unrecognised paint travels — this setup names no rules.",
     "The copy-setup dialogue when the setup has paint but no rules."],
  "copyset.nothing":
    ["Nothing travels — this setup names no rules.",
     "The copy-setup dialogue when the setup is empty."],
  "copyset.empty_why":
    ["<b>{mesh} has nothing set up</b> — there is nothing to copy from it, and copying would only clear the other mesh. Flip the direction above, or set something up on it first.",
     "The copy-setup dialogue's explanation when the mesh being copied FROM has no setup at all; {mesh} is it."],
  "copyset.dir_empty":
    ["nothing to copy",
     "The copy-setup dialogue: the note on a direction whose source mesh has no setup."],
  "copyset.as_is":
    ["travels as it is",
     "The copy-setup dialogue: a rule row that is not re-pointed."],
  "copyset.become_title":
    ["What should {rule} become there?",
     "The copy-setup dialogue's rule picker heading; {rule} is the rule on the way over."],
  "copyset.become_why":
    ["The copy’s assignments — and its painted colour — come to stand for the rule you pick.",
     "That picker's explanation."],
  "copyset.keep":
    ["Let it travel as it is",
     "That picker's row that leaves the rule as it is."],
  "copyset.keep_hint":
    ["no re-pointing",
     "Under that row."],
  /* Round 18ap: keyed — the statics export and import dialogues' innards, the busy card. */
  "stexp.count":
    ["{on} of {total} ticked",
     "The statics export dialogue: the count beside the list heading."],
  "stexp.shown_by_filter":
    ["  ·  {n} shown by the filter",
     "After that count while a filter is typed; {n} is how many rows show."],
  "stexp.foot_all.one":
    ["the whole store — {n} mesh setup",
     "The statics export dialogue's foot when everything is ticked."],
  "stexp.foot_all.other":
    ["the whole store — {n} mesh setups",
     "The same for several."],
  "stexp.foot_some":
    ["{on} of {total} mesh setups",
     "That foot when some are ticked."],
  "stexp.foot_locked":
    ["  ·  {n} locked here, and locks do not travel",
     "After that foot when ticked setups are locked; {n} is how many."],
  "stexp.file_kind":
    ["Gardenfell statics rules",
     "The Save dialogue's file-type name for a statics file."],
  "stimp.skip_all":
    ["Skip all",
     "The statics import dialogue: skips every name-matched mesh."],
  "stimp.remap_all":
    ["Remap all",
     "The statics import dialogue: remaps every name-matched mesh."],
  "stimp.names_hint":
    ["These meshes name rules you do not have by id, but you have a rule of the same name. Matched, the setup lands on yours; unmatched, its rules stay unanswered and it cannot be imported.",
     "The statics import dialogue: the hint over the name-matched rows."],
  "stimp.missing_hint":
    ["These files name rules your loaded Grass Rules do not have, and nothing of yours has the same name and kind. Point each at one of your rules — or make a new one — before the import can land.",
     "The statics import dialogue: the hint over the repair rows."],
  "stimp.conflicts_hint":
    ["These meshes have a setup of yours already. Keep yours, or take the imported one.",
     "The statics import dialogue: the hint over the conflict rows."],
  "stimp.intro_new.one":
    ["{n} new mesh setup",
     "The statics import dialogue's summary line: setups that are new here."],
  "stimp.intro_new.other":
    ["{n} new mesh setups",
     "The same for several."],
  "stimp.intro_conflicts":
    [", {n} already set up here",
     "Added to that line for meshes that already have a setup."],
  "stimp.intro_matched":
    [", {n} matched by rule name",
     "Added to that line for meshes matched by rule name."],
  "stimp.intro_missing.one":
    [", {n} rule to point at yours",
     "Added to that line for a rule that needs pointing at one of yours."],
  "stimp.intro_missing.other":
    [", {n} rules to point at yours",
     "The same for several."],
  "stimp.file_skipped":
    ["skipped — {why}",
     "The statics import dialogue, beside a file that could not be read; {why} is the reason."],
  "stimp.file_meshes.one":
    ["{n} mesh",
     "Beside a file in that dialogue: how many setups it holds."],
  "stimp.file_meshes.other":
    ["{n} meshes",
     "The same for several."],
  "rule.kind_paint":
    ["paint only",
     "The kind of a paint-only rule, wherever a rule's kind is shown beside its name."],
  "rule.kind_texture":
    ["ground texture",
     "The kind of a ground-texture rule, the same places."],
  "stimp.which_becomes":
    ["Which rule does “{rule}” become?",
     "The statics import dialogue: the picker heading for a name-matched rule; {rule} is theirs."],
  "stimp.why_skipped":
    ["A skipped mesh names rules you do not have — Remap it, or give each of its rules one of yours.",
     "The statics import dialogue: why Import is greyed, when a skipped mesh is unanswered."],
  "stimp.why_point":
    ["Point every rule above at one of yours first.",
     "The same, when repair rows are unanswered."],
  "stimp.named_by":
    ["Named by: {name}",
     "The statics import dialogue, under a repair row's rule: which file named it; {name} is the file."],
  "stimp.choose_rule":
    ["— choose a rule —",
     "The statics import dialogue: a repair row before a rule is chosen."],
  "stimp.not_answered":
    ["Not answered yet",
     "That row's tooltip before a rule is chosen."],
  "stimp.which_stands_in":
    ["Which rule stands in for “{rule}”?",
     "The statics import dialogue: the picker heading for a repair row; {rule} is theirs."],
  "stimp.keep_mine":
    ["Keep mine",
     "The statics import dialogue: a conflict row's first choice."],
  "stimp.take_theirs":
    ["Take theirs",
     "The statics import dialogue: a conflict row's second choice."],
  "cell.stats_title":
    ["real cell preview",
     "Viewport, the statistics box: its heading in Real cell mode."],
  "cell.no_cell_chosen":
    ["no cell chosen",
     "Viewport, the statistics box, before a cell is chosen."],
  /* Round 18cy: the visit history's two arrows on the statistics box's heading row. */
  "cell.hist_back_title":
    ["Back to {cell}",
     "The statistics box: the left arrow's tooltip; {cell} is the cell visited before this one."],
  "cell.hist_fwd_title":
    ["Forward to {cell}",
     "The statistics box: the right arrow's tooltip; {cell} is the cell visited after this one."],
  "cell.hist_back_none":
    ["No earlier cell this session",
     "The statistics box: the left arrow's tooltip when there is no cell to go back to."],
  "cell.hist_fwd_none":
    ["No later cell",
     "The statistics box: the right arrow's tooltip when there is no cell to go forward to."],
  "cell.status_choose":
    ["Choose a cell to load.",
     "Left column, Preview: the status line under the cell button before one is chosen."],
  "connect.failed_partway":
    ["The install could not be opened: {err}. Nothing is loaded — try connecting again.",
     "Toast when a connect threw part-way through and the page was left empty."],
  "cell.status_failed":
    ["That cell would not load: {err}",
     "Left column, Preview: the status line when a cell could not be read at all."],
  "cell.status_none_loaded":
    ["Nothing loaded. {list}",
     "Left column, Preview: the status line when every cell asked for failed, listing why."],
  "busy.loading_cell":
    ["Loading cell",
     "The busy card over the viewport while a cell loads."],
  "busy.n_of":
    ["{done} of {total}",
     "Under the busy card's heading: progress through a count."],
  "busy.building_terrain":
    ["Building terrain",
     "The busy card while the terrain is built."],
  "busy.placing_objects":
    ["Placing objects",
     "The busy card while the objects are placed."],
  "busy.n_of_meshes":
    ["{done} of {total} meshes",
     "Under \"Placing objects\": progress through the meshes."],
  "busy.scattering":
    ["Scattering grass",
     "The busy card, and the bottom bar, while grass is scattered."],
  "busy.scattering_sub":
    ["Scattering grass — {sub}",
     "The bottom bar's form of that with progress; {sub} is \"cell 3 of 9\"."],
  "busy.cell_n_of":
    ["cell {done} of {total}",
     "The progress under \"Scattering grass\"."],
  "busy.building_grass":
    ["Building grass",
     "The busy card while the grass is built for drawing."],
  /* Round 18ap: keyed for the language packs — the cell status line, the statistics box, the land-texture list and the cell picker (18_cellpreview.js). */
  "cell.status_loaded":
    ["{name} — {cells}, {refs}",
     "Left column, Preview: the status line under the cell button once a cell is loaded. {name} is the cell, {cells} and {refs} are the two counts below."],
  "cell.n_cells":
    ["{n} cell(s)",
     "The status line: how many cells were loaded (the chosen one and its neighbours)."],
  "cell.n_references":
    ["{n} references",
     "The status line: how many object references those cells hold."],
  "cell.status_interior":
    ["interior, no landscape",
     "The status line, appended after \" · \" when the loaded cell is an interior."],
  "cell.status_no_land":
    ["{n} without a LAND record",
     "The status line, appended when some loaded exterior cells have no landscape record."],
  "cell.status_failed":
    ["{n} failed to load, so the blend at those edges is not what the export will use",
     "The status line, appended when neighbouring cells would not load."],
  "cell.status_unread":
    ["{n} plugin(s) could not be read: {list}",
     "The status line, appended when a plugin in the load order could not be read; {list} names them."],
  "cell.status_unresolved":
    ["{n} reference(s) point at a master that is not loaded, so they cannot be identified and may duplicate",
     "The status line, appended when references name a master that is not in the load order."],
  "vp.recentred":
    ["View recentred",
     "Toast after the recentre-view shortcut puts the camera back."],
  "cell.st_water":
    ["water",
     "Statistics box (viewport, Real cell mode): the row for an interior's water."],
  "cell.st_water_none":
    ["none",
     "Statistics box: the water row's value when the cell has no water level."],
  "cell.st_water_units":
    ["{n} units",
     "Statistics box: the water row's value — the level in game units."],
  "cell.st_water_tip":
    ["Level from {level} · water flag from {flag}",
     "Statistics box: the water row's tooltip — which plugin set the level and which set the flag."],
  "cell.st_water_nobody":
    ["nobody — the flag alone",
     "Statistics box: the water tooltip's {level} when no plugin set a level."],
  "cell.st_cell":
    ["cell",
     "Statistics box: the row naming the cell."],
  "cell.st_cells_loaded":
    ["cells loaded",
     "Statistics box: how many cells are loaded."],
  "cell.st_textures":
    ["textures",
     "Statistics box: how many land textures the loaded cells use."],
  "cell.st_references":
    ["references",
     "Statistics box: how many object references the loaded cells hold."],
  "cell.st_overridden":
    ["overridden by later",
     "Statistics box, under references: references a later plugin replaced."],
  "cell.st_deleted":
    ["deleted by later",
     "Statistics box, under references: references a later plugin deleted."],
  "cell.st_moved_out":
    ["moved out",
     "Statistics box, under references: references a later plugin moved to another cell."],
  "cell.st_moved_in":
    ["moved in",
     "Statistics box, under references: references a later plugin moved into this cell."],
  "cell.st_objects":
    ["objects found",
     "Statistics box: how many placed objects the loaded cells hold."],
  "cell.st_no_mesh_record":
    ["no mesh record",
     "Statistics box, under objects: objects whose record names no mesh. Shown red."],
  "cell.st_mesh_missing":
    ["mesh missing",
     "Statistics box, under objects: objects whose mesh file is not in the load order. Shown red."],
  "cell.st_editor_markers":
    ["editor markers",
     "Statistics box, under objects: editor markers the game never draws."],
  "cell.st_corpses":
    ["corpses",
     "Statistics box: bodies among the objects."],
  "cell.st_npc_markers":
    ["NPC markers",
     "Statistics box, under corpses: NPC markers."],
  "cell.st_actors_skipped":
    ["live actors skipped",
     "Statistics box: living actors left out of avoidance."],
  "cell.st_moths":
    ["moths",
     "Statistics box: how many lamps in the scene carry Nocturnal Moths (only when that mod is installed)."],
  "cell.st_moths_tip":
    ["MWSE - Nocturnal Moths is installed. {from} names {meshes} lantern mesh(es); {lamps} of the lamps in this scene wear one. They are drawn during the Lights picker’s night hours, and the Particles switch carries them.",
     "Statistics box: the moths row's tooltip. {from} is where the lantern list came from (the mod's own list, or the one it ships with)."],
  "cell.moths_from_own":
    ["the mod’s own list",
     "The moths tooltip's {from} when the installed mod's own lantern list was read."],
  "cell.moths_from_shipped":
    ["the list the mod ships with",
     "The moths tooltip's {from} when the mod's list could not be read and the built-in one is used."],
  "cell.st_triangles":
    ["triangles",
     "Statistics box: the triangle counts."],
  "cell.st_triangles_v":
    ["{grass} grass · {statics} statics",
     "Statistics box: the triangles row's value."],
  "rule.scope_everywhere":
    ["everywhere",
     "The rule row under a land texture (mesh editor and texture list): a rule with no scope."],
  "rule.scope_any_named":
    ["any named cell",
     "The rule row: a rule scoped to any named cell."],
  "rule.scope_unset":
    ["(no scope set)",
     "The rule row: a scoped rule whose scope value is empty."],
  "rule.unnamed":
    ["(unnamed)",
     "The rule row: a rule with no texture set."],
  "rule.open_title":
    ["Open this rule — {key}",
     "The rule row's tooltip; {key} is the rule's id."],
  "cell.no_textures":
    ["No land textures loaded.",
     "Left column, the land-texture list before a cell is loaded."],
  "cell.tex_uncovered":
    ["no section in this config covers this texture",
     "The land-texture list: a texture's tooltip when no rule covers it."],
  "cell.tex_layered":
    ["{n} rules layered here — {rules} — each placing its own grass",
     "The land-texture list: a texture's tooltip when several rules place on it."],
  "cell.tex_uses":
    ["uses [{rule}]",
     "The land-texture list: a texture's tooltip naming the rule that places on it."],
  "cell.tex_no_image":
    ["no image",
     "The land-texture list: the tag on a texture whose image file is not in the overlay."],
  "cell.tex_no_image_title":
    ["The LTEX record names {file}, which is not in the overlay as a loose file.",
     "The land-texture list: the \"no image\" tag's tooltip."],
  "cell.tex_no_file":
    ["no file",
     "The \"no image\" tooltip's {file} when the LTEX record names no file at all."],
  "cell.tex_add_another_title":
    ["Add another ground texture rule for [{tex}] and edit it — the narrowest scope wins where they overlap",
     "The land-texture list: the + button's tooltip on a texture that already has a rule."],
  "cell.tex_add_title":
    ["Add [{tex}] as a new ground texture rule and edit it",
     "The land-texture list: the + button's tooltip on a texture with no rule."],
  "cell.tex_edit_layered_title":
    ["Edit [{rule}], the rule that places here — {n} rules cover this texture and the narrowest wins",
     "The land-texture list: the pencil button's tooltip when several rules cover the texture."],
  "cell.tex_edit_title":
    ["Edit [{rule}], the rule for this ground",
     "The land-texture list: the pencil button's tooltip."],
  "cell.tex_note":
    ["{n} of {total} texture(s) have a matching section. Section choice follows the generator: exact cell, then any named cell, then region, then plain.",
     "The land-texture list: the note under it."],
  "cell.tex_added":
    ["Added [{tex}] — add a mesh slot to start placing grass",
     "Toast after the + button on a land texture creates a rule for it."],
  "cell.btn_none":
    ["<none>",
     "Left column, Preview: the cell button before a cell is chosen."],
  "cell.btn_interior":
    ["interior",
     "Left column, Preview: the cell button for an interior with no name."],
  "cell.btn_loaded_title":
    ["{cell} — loaded; click to choose another",
     "The cell button's tooltip once a cell is loaded."],
  "cell.btn_title":
    ["Choose a cell to load",
     "The cell button's tooltip before a cell is chosen."],
  "connect.setup_first":
    ["Connect a setup first.",
     "Toast when the cell picker is opened before a setup is connected."],
  "cell.pick_head":
    ["Choose a cell",
     "The cell picker dialogue's heading."],
  "cell.pick_filter":
    ["filter… (name, region, or grid like -3,12)",
     "The cell picker's filter box placeholder."],
  "cell.pick_all":
    ["All",
     "The cell picker: the filter button for every cell."],
  "cell.pick_named":
    ["Named",
     "The cell picker: the filter button for named cells."],
  "cell.pick_wild":
    ["Wilderness",
     "The cell picker: the filter button for unnamed exteriors."],
  "cell.pick_int":
    ["Interiors",
     "The cell picker: the filter button for interiors (shown when interiors are listed)."],
  "cell.pick_show_int_title":
    ["Interiors have no landscape, so nothing is scattered in one yet — this is the first step towards them",
     "The cell picker: the Show interiors switch's tooltip."],
  "cell.pick_show_int":
    ["Show interiors",
     "The cell picker: the label beside the switch that lists interiors."],
  "cell.pick_clear":
    ["Clear selection",
     "The cell picker: the button that unloads the chosen cell."],
  "cell.pick_count":
    ["{n} of {total}",
     "The cell picker: the pill in the heading — matches out of all cells."],
  "cell.pick_first_900":
    ["Showing the first 900 of {n}. Narrow the filter to see more.",
     "The cell picker: the note when more than 900 cells match."],
  "cell.pick_nothing":
    ["Nothing matches.",
     "The cell picker: shown when the filter matches no cell."],
  /* Round 18cp: the picker's map — the world painted from its ground textures, a cell a
     square, in place of the list (31_cellmap.js). */
  "cell.view_list":
    ["List",
     "The cell picker: the button that shows the cells as a list."],
  "cell.view_map":
    ["Map",
     "The cell picker: the button that shows the cells as a map of the world."],
  "cell.view_list_title":
    ["Every cell as a list, interiors included",
     "The cell picker: the List button's tooltip."],
  "cell.view_map_title":
    ["The exterior world as a map: wheel to zoom, middle button to pan, click a cell to load it, right-click to star it",
     "The cell picker: the Map button's tooltip."],
  "cell.map_reading":
    ["Reading the map… {done} of {total} cells",
     "The cell picker's map, while its tiles are being read: {done} and {total} are counts."],
  "cell.map_failed":
    ["The map could not be read: {err}",
     "The cell picker's map, when the engine could not paint it; {err} is the engine's message."],
  "cell.map_empty":
    ["Nothing to map — this load order has no exterior landscape.",
     "The cell picker's map, when the world has no cell with a landscape."],
  "cell.map_tip_where":
    ["({x}, {y}) · {region}",
     "The map's hover note, second line: the cell's grid position and its region. The first line is the cell's name, or its region when it has none."],
  "cell.map_tip_where_noregion":
    ["({x}, {y})",
     "The map's hover note, second line, for a cell in no region: its grid position alone."],
  "cell.map_wilderness":
    ["Wilderness",
     "The map's hover note: what an unnamed cell in no region is called."],
  "cell.map_star_note":
    ["Right-click to star it · starred",
     "The map's hover note, third line, on a starred cell."],
  "cell.map_star_note_off":
    ["Right-click to star it",
     "The map's hover note, third line, on a cell that is not starred."],
  "cell.map_current":
    ["loaded now",
     "The map's hover note: added after the name of the cell that is currently loaded."],
  /* Round 18ap: keyed for the language packs — the object dialogue (verdicts, paint, canopies) and the two pattern lists (18_cellpreview.js). */
  "connect.cell_preview":
    ["Connect a setup first — cell preview needs the plugin data.",
     "Toast when Real cell mode is chosen before a setup is connected."],
  "avoid.v_yes":
    ["Grass keeps clear of this.",
     "Object dialogue, the avoidance verdict: the green ruling, followed by the reason."],
  "avoid.v_no":
    ["Grass grows through this.",
     "Object dialogue, the avoidance verdict: the amber ruling, followed by the reason."],
  "avoid.v_off":
    ["Avoid objects is switched off, so nothing on the map is blocking grass.",
     "Avoidance verdict: the reason when Avoid objects is off."],
  "avoid.v_excluded":
    ["It matches {pattern} on the never-avoid list.",
     "Avoidance verdict: the object matches a never-avoid pattern. {pattern} is shown bold."],
  "avoid.v_marker":
    ["It is an editor marker — a travel destination, a door target or the north arrow. The game never draws it, so grass grows straight through.",
     "Avoidance verdict: the object is an editor marker."],
  "avoid.v_corpse":
    ["It is a body, and Corpses is switched off.",
     "Avoidance verdict: a corpse while Corpses is off."],
  "avoid.v_actor":
    ["It is a living actor. It walks away from where the plugin put it, so a permanent hole in the grass under its starting spot would be wrong.",
     "Avoidance verdict: a living actor."],
  "avoid.v_unknown":
    ["Nothing in the load order says what this object looks like, so there is no shape to keep grass off.",
     "Avoidance verdict: no record describes the object."],
  "avoid.v_mesh_missing":
    ["It names a mesh that is not in the load order at all — a mod that forgot to ship it, most likely.",
     "Avoidance verdict: the object's mesh file is missing."],
  "avoid.v_no_geometry":
    ["The mesh has no geometry to stand a shape around — unreadable, or an effect with nothing solid in it.",
     "Avoidance verdict: the mesh holds nothing solid."],
  "avoid.v_too_small":
    ["Its collision shape has a radius of {size} units where it stands, under the \"ignore under\" setting of {min}. Lower that to keep grass off it.",
     "Avoidance verdict: the object is under the ignore-under size."],
  "avoid.v_by_hull":
    ["Tested against its collision hull",
     "Avoidance verdict: how the shape was tested — its collision hull."],
  "avoid.v_by_visible":
    ["Tested against its visible mesh, since it has no collision hull",
     "Avoidance verdict: how the shape was tested — the visible mesh."],
  "avoid.v_by_bounds":
    ["It has no collision in game, so it is tested against a box half the size of its bounds — enough to keep grass off the plant itself, not off every leaf it spreads",
     "Avoidance verdict: how the shape was tested — a half-bounds box."],
  "avoid.v_by_box":
    ["Its mesh has too many triangles to test a blade against thousands of times a cell, so it is tested against its bounding box",
     "Avoidance verdict: how the shape was tested — the bounding box."],
  "avoid.v_by_shape":
    ["Tested against its shape",
     "Avoidance verdict: how the shape was tested, when the engine did not say which way."],
  "avoid.v_shape_tail":
    ["{how} (radius {size} units{clearance}). Anything still growing through it is standing where the shape is not.",
     "Avoidance verdict: the green reason. {how} is one of the \"Tested against\" strings, {clearance} the clearance clause or nothing."],
  "avoid.v_clearance":
    [", plus {n} units of clearance measured out from its surface",
     "Avoidance verdict: the clearance clause inside the radius bracket."],
  "avoid.placed_by":
    ["placed by {plugin}",
     "Object dialogue: under the heading, which plugin placed the object."],
  "avoid.v_ref_off":
    ["Never avoid is on for this one placed copy, so avoidance steps over it and grass may grow where it stands. Other references wearing this mesh are still avoided.",
     "Avoidance verdict: the reason while the per-reference Never avoid switch is on."],
  "avoid.asking":
    ["Asking the engine…",
     "Object dialogue: the verdict box while the engine is asked."],
  "avoid.no_verdict":
    ["Could not get a verdict: {err}",
     "Object dialogue: the verdict box when the engine could not answer."],
  "avoid.painted":
    ["Painted: {marks} marks{room}.",
     "Object dialogue, the paint section: how many marks this reference carries. {marks} is the bold count, {room} the \"of N\" clause or nothing."],
  "avoid.painted_of":
    [" of {room}",
     "Object dialogue, the paint section: the clause giving the room for marks on this reference."],
  "avoid.compacted":
    ["Compacted: {n} marks dropped.",
     "Object dialogue: said after Compact thins the paint on this reference."],
  "avoid.compact_failed":
    ["Could not compact that: {err}",
     "Object dialogue: said when Compact failed."],
  "avoid.clear_ref_head":
    ["Clear the paint on this object?",
     "The dialogue before clearing the paint on one reference: its title."],
  "avoid.clear_ref_body":
    ["Every mark on this one reference. The grass rules are untouched, and Undo can bring the paint back while the program is open.",
     "That dialogue's body."],
  "avoid.clear_ref_yes":
    ["Clear this reference",
     "That dialogue's confirming choice."],
  "avoid.clear_ref_yes_note":
    ["All of its paint goes",
     "The note under the confirming choice."],
  "avoid.cleared_ref":
    ["Cleared this reference.",
     "Object dialogue: said after the paint on this reference was cleared."],
  "avoid.nothing_to_clear":
    ["There was nothing to clear.",
     "Object dialogue: said when a clear found no paint."],
  "avoid.clear_failed":
    ["Could not clear that: {err}",
     "Object dialogue: said when a clear failed."],
  "avoid.clear_model_head":
    ["Clear the paint on every object of this type?",
     "The dialogue before clearing the paint on every reference of a mesh: its title."],
  "avoid.clear_model_body":
    ["Every reference in the world wearing {mesh} — not just this one, and not just the ones you can see. Undo can bring it back while the program is open.",
     "That dialogue's body; {mesh} is the mesh file name, bold."],
  "avoid.clear_model_yes":
    ["Clear every one of them",
     "That dialogue's confirming (red) choice."],
  "avoid.clear_model_yes_note":
    ["All their paint goes",
     "The note under the confirming choice."],
  "avoid.cleared_n.one":
    ["Cleared {n} reference.",
     "Object dialogue: said after clearing the paint on every reference of a mesh (one)."],
  "avoid.cleared_n.other":
    ["Cleared {n} references.",
     "Object dialogue: said after clearing the paint on every reference of a mesh."],
  "avoid.paint_why_never":
    ["It matches {pattern} on the never-paint-on list.",
     "Paint verdict: the reason when the mesh matches a never-paint pattern. {pattern} is bold."],
  "avoid.paint_why_ref":
    ["The switch below has turned this one placed copy off. Other references wearing this mesh still take paint.",
     "Paint verdict: the reason while the per-reference paint switch is off."],
  "avoid.ref_paint_on":
    ["This reference takes paint again.",
     "Object dialogue: said when the per-reference paint switch is turned on."],
  "avoid.ref_paint_off":
    ["The brush will pass this reference by.",
     "Object dialogue: said when the per-reference paint switch is turned off."],
  "avoid.ref_avoid_off":
    ["Avoidance passes this one reference by.",
     "Object dialogue: said when the per-reference Never avoid switch is turned on."],
  "avoid.ref_avoid_on":
    ["This reference is avoided again.",
     "Object dialogue: said when the per-reference Never avoid switch is turned off."],
  "avoid.nopaint_removed":
    ["This mesh can take paint again.",
     "Object dialogue: said when the mesh is taken off the never-paint list."],
  "avoid.type_pattern":
    ["Type a pattern first.",
     "Object dialogue: said when Add is pressed with an empty pattern."],
  "avoid.nopaint_added":
    ["The brush will no longer land on anything matching {pattern}.",
     "Object dialogue: said when a pattern joins the never-paint list."],
  "avoid.wide_head":
    ["This pattern covers more than this object.",
     "Object dialogue: the bold lead of the warning when removing a wildcard pattern would reach other objects."],
  "avoid.wide_body":
    ["{pattern} matches {n} object(s) in the cells you have loaded{across}. Removing it puts grass back around all of them.",
     "That warning's body. {pattern} is the pattern in code style, {n} the bold count, {across} the meshes clause or nothing."],
  "avoid.wide_across":
    [", across {n} different meshes: {list}{more}",
     "That warning's meshes clause; {list} names up to six, {more} is the \"and N more\" tail or nothing."],
  "avoid.wide_more":
    [" and {n} more",
     "That warning's tail when more than six meshes match."],
  "avoid.removed":
    ["Removed from the never-avoid list. Grass keeps clear of it again.",
     "Object dialogue: said when the pattern is taken off the never-avoid list."],
  "avoid.added":
    ["Added to the never-avoid list. Grass may sit inside anything matching {pattern}.",
     "Object dialogue: said when a pattern joins the never-avoid list."],
  "avoid.canopy_took_off":
    ["Took {pattern} off {name}.",
     "Object dialogue, canopies: said when a pattern is removed from a canopy."],
  "avoid.canopy_choose":
    ["Choose a canopy first.",
     "Object dialogue, canopies: said when Add is pressed with no canopy chosen."],
  "avoid.canopy_added":
    ["Added to {name}. Cards naming it stand under, or clear of, anything matching {pattern}.",
     "Object dialogue, canopies: said when a pattern is added to a canopy."],
  "avoid.list_add":
    ["+ Add pattern",
     "Left column, the never-avoid and never-paint lists: the button that adds an empty row."],
  "avoid.list_io":
    ["⧉ Copy / paste",
     "Left column, both pattern lists: the button that opens the copy/paste dialogue."],
  "avoid.list_io_title":
    ["Copy these patterns, or paste a set in",
     "Both pattern lists: the copy/paste button's tooltip."],
  "avoid.list_io_head":
    ["Avoidance exclusions",
     "The never-avoid list's copy/paste dialogue: its heading."],
  "avoid.list_io_hint":
    ["One pattern per line. <code>*</code> matches anything after it, and a pattern is tested against both the object id and its mesh path.",
     "The never-avoid list's copy/paste dialogue: the hint over the text area."],
  "avoid.list_replaced":
    ["Patterns replaced with {n} pattern(s)",
     "Toast after the copy/paste dialogue replaced a pattern list."],
  "avoid.list_added":
    ["Added {n} pattern(s)",
     "Toast after the copy/paste dialogue appended to a pattern list."],
  "paint.list_io_head":
    ["Never paint on these",
     "The never-paint list's copy/paste dialogue: its heading."],
  "paint.list_io_hint":
    ["One pattern per line. <code>*</code> matches anything after it, and a pattern is tested against the mesh path and its file name.",
     "The never-paint list's copy/paste dialogue: the hint over the text area."],
  /* Round 18ap: keyed for the language packs — the statics export list (18_cellpreview.js). */
  "stexp.list_failed":
    ["Could not read the setups: {err}",
     "Toast when the statics export could not list the mesh setups."],
  /* Round 18ap: keyed for the language packs — the rules list, the rule head, the paste dialogue, the pickers and the scope field (09_ui.js). */
  "left.n_rules.one":
    ["{n} rule",
     "A count of rules: the pill on the folded Rules heading, the group headings, the bulk bar."],
  "left.n_rules.other":
    ["{n} rules",
     "A count of rules: the pill on the folded Rules heading, the group headings, the bulk bar."],
  "right.bulk_note":
    ["every change below lands on all of them",
     "Right panel, the bar shown while several rules are selected."],
  "right.bulk_one":
    ["Just one",
     "Right panel, the bulk bar: the button that goes back to a single rule."],
  "right.bulk_one_title":
    ["Go back to editing the rule you selected first",
     "The Just one button's tooltip."],
  "left.no_rules":
    ["No rules yet.<br>Press <b>+ Add</b> or import an .ini.",
     "Left column, the rules list when there are none."],
  "left.rule_unknown_title":
    ["This rule matches nothing in your scanned load order",
     "Left column, a rule row's tooltip when its texture or scope is not in the load order."],
  "left.rule_off_title":
    ["Switched off — press to enable",
     "Left column, a rule row: the power button's tooltip while the rule is off."],
  "left.rule_on_title":
    ["Enabled — press to switch off",
     "Left column, a rule row: the power button's tooltip while the rule is on."],
  "left.rule_dup_title":
    ["Duplicate this rule",
     "Left column, a rule row: the duplicate button's tooltip."],
  "left.rule_del_title":
    ["Delete this rule",
     "Left column, a rule row: the delete button's tooltip."],
  "left.del_title":
    ["Delete this rule?",
     "The dialogue before deleting a rule: its title."],
  "left.del_body":
    ["{rule} — {slots}, {bans}. Its settings go with it.",
     "That dialogue's body; {rule} is the rule's name in bold, {slots} and {bans} the two counts."],
  "left.n_slots.one":
    ["{n} grass slot",
     "The delete-rule dialogue: how many grass slots the rule has."],
  "left.n_slots.other":
    ["{n} grass slots",
     "The delete-rule dialogue: how many grass slots the rule has."],
  "left.n_bans.one":
    ["{n} ban",
     "The delete-rule dialogue: how many bans the rule has."],
  "left.n_bans.other":
    ["{n} bans",
     "The delete-rule dialogue: how many bans the rule has."],
  "dlg.delete_it":
    ["Delete it",
     "A confirming choice in delete dialogues."],
  "left.del_yes_note":
    ["The rule and everything on it",
     "The delete-rule dialogue: the note under Delete it."],
  "right.copy_settings":
    ["⧉ Copy settings",
     "Right panel, under the rule: the button that copies the rule's settings."],
  "right.copy_settings_title":
    ["Copy every setting on this rule — placement, culling, randomisation, clumping, its grass and its bans — ready to paste onto another texture",
     "The Copy settings button's tooltip."],
  "right.settings_copied":
    ["Settings copied from {rule}",
     "Toast after Copy settings."],
  "right.paste_settings":
    ["⤵ Paste settings",
     "Right panel, under the rule: the button that pastes copied settings onto it."],
  "right.paste_settings_title":
    ["Apply the copied settings to this rule, keeping its own land texture and scope",
     "The Paste settings button's tooltip while something is copied."],
  "right.paste_settings_none":
    ["Copy a rule’s settings first",
     "The Paste settings button's tooltip while nothing is copied."],
  "right.pasted":
    ["Pasted {what} from {from}",
     "Toast after Paste settings; {what} is one of the three \"pasted\" strings, {from} the source rule."],
  "right.pasted_both":
    ["settings and grass",
     "The paste toast's {what} when both halves were pasted."],
  "right.pasted_settings":
    ["settings, grass left as it was",
     "The paste toast's {what} when only the settings were pasted."],
  "right.pasted_cards":
    ["grass, settings left as they were",
     "The paste toast's {what} when only the grass cards were pasted."],
  "right.copied_rule":
    ["the copied rule",
     "The paste dialogue and toast: what to call the source when it has no texture name."],
  "right.n_cards.one":
    ["{n} card",
     "The paste dialogue: a count of grass cards."],
  "right.n_cards.other":
    ["{n} cards",
     "The paste dialogue: a count of grass cards."],
  "right.paste_both":
    ["Settings and grass cards",
     "The paste dialogue: the choice that pastes everything."],
  "right.paste_both_replace":
    ["{bring} replace {has}",
     "The paste dialogue: the note under Settings and grass cards when the target has cards."],
  "right.paste_settings_only":
    ["Settings only",
     "The paste dialogue: the choice that pastes the tuning alone."],
  "right.paste_cards_stay":
    ["{cards} here stay",
     "The paste dialogue: the note under Settings only."],
  "right.paste_cards_only":
    ["Grass cards only",
     "The paste dialogue: the choice that pastes the cards alone."],
  "right.paste_cards_note":
    ["nothing else moves",
     "The paste dialogue: the note under Grass cards only."],
  "right.paste_cards_none":
    ["the copied rule has none — this would empty it",
     "The paste dialogue: the note under Grass cards only when the source has no cards."],
  "right.paste_title":
    ["What should be pasted?",
     "The paste dialogue's title."],
  "right.paste_body":
    ["From {from} onto {onto}. Its texture, scope and notes stay.",
     "The paste dialogue's body; both names are bold."],
  "dlg.pick_title":
    ["Choose",
     "The generic picker dialogue's heading when the caller gives none."],
  "dlg.pick_filter":
    ["Filter by name…",
     "The generic picker dialogue's filter placeholder."],
  "dlg.pick_chosen":
    ["chosen",
     "The generic picker: the pill on the entry that is currently chosen."],
  "dlg.pick_nothing":
    ["Nothing matches that.",
     "The generic picker when the filter matches nothing."],
  "dlg.pick_new":
    ["+ New…",
     "The generic picker: the make-a-new-one row when the caller gives no wording."],
  "dlg.pick_count":
    ["{n} of {total}",
     "The generic picker: matches out of all entries."],
  "rules.pick_title":
    ["Choose a rule",
     "The rule picker's heading."],
  "rules.pick_new_paint":
    ["+ New paint-only rule…",
     "The rule picker: the row that makes a new paint-only rule."],
  "rules.pick_nothing":
    ["No rule of yours matches that.",
     "The rule picker when the filter matches nothing."],
  "name.default_title":
    ["Name it",
     "The name-it dialogue's heading when the caller gives none."],
  "name.create":
    ["Create",
     "The name-it dialogue's confirming button when the caller gives no wording."],
  "name.taken":
    ["There is already a rule called that. Both will work — they are told apart by what they are, not by what they are called — but the two will be hard to tell apart in a list.",
     "The name-it dialogue: the warning under the field when the name is already in use."],
  "right.mixed_title":
    ["These rules have different values here. Type one to set them all.",
     "Right panel, with several rules selected: a field's tooltip where the rules differ."],
  "right.inherits_n":
    ["Blank inherits the rule’s: {n}",
     "A grass card field's tooltip: what a blank inherits from the rule, with the value."],
  "right.inherits":
    ["Blank inherits the rule’s.",
     "A grass card field's tooltip: a blank inherits from the rule."],
  "right.blank_is":
    ["Blank is",
     "A field's tooltip: what a blank value amounts to, followed by the number."],
  "right.mixed_switch_title":
    ["These rules do not agree. Setting it either way sets them all.",
     "Right panel, with several rules selected: a switch's tooltip where the rules differ."],
  "rule.tag_known":
    ["known",
     "Right panel, the land-texture field: the tag when the texture is in the load order."],
  "rule.tag_spelling":
    ["Your install spells it \"{canon}\" — matched either way.",
     "The texture and scope tags' tooltip when the typed name differs from the install's in case."],
  "rule.tag_defined_by":
    ["Defined by {plugin}",
     "The land-texture tag's tooltip: which plugin defines the texture."],
  "rule.tag_texture_file":
    [" — texture file: {file}",
     "The land-texture tag's tooltip: the texture's file, appended."],
  "rule.tag_no_match":
    ["no match",
     "The texture and scope tags when the name is not in the load order."],
  "rule.tag_no_ltex":
    ["No LTEX with this id exists in your current load order, in any case.",
     "The land-texture tag's tooltip when the texture is not in the load order."],
  "rule.tex_pick_title":
    ["Pick from the land textures in your load order",
     "The \"…\" button beside the land-texture field: its tooltip."],
  "copy.list_copy":
    ["⧉ Copy",
     "The list copy/paste dialogue (bans, patterns): the Copy button."],
  "copy.list_add":
    ["Add to list",
     "The list copy/paste dialogue: the button that appends the pasted lines."],
  "copy.list_replace":
    ["Replace list",
     "The list copy/paste dialogue: the button that replaces the list with the pasted lines."],
  "copy.list_title":
    ["List",
     "The list copy/paste dialogue's heading when the caller gives none."],
  "copy.list_copied":
    ["Copied — paste it into another section",
     "Toast after the list dialogue's Copy button."],
  "copy.no_clipboard":
    ["Could not reach the clipboard; the text is selected, press Ctrl+C",
     "Toast when the clipboard could not be written; the text is left selected."],
  "right.no_rule":
    ["No selector",
     "Right panel's heading when there is no rule to show."],
  "right.no_rules_body":
    ["Create or import a config to begin.",
     "Right panel's body when there are no rules."],
  "right.n_selected":
    ["{n} rules selected",
     "Right panel's heading while several rules are selected."],
  "rule.name_placeholder":
    ["Wildflower meadows",
     "Right panel, a paint-only rule's name field: the placeholder (an example name)."],
  "rule.tag_empty":
    ["empty",
     "Right panel, the scope field: the tag while the field is blank."],
  "rule.tag_region":
    ["region",
     "Right panel, the scope field: the tag when the name is a region."],
  "rule.tag_cell":
    ["cell",
     "Right panel, the scope field: the tag when the name is a cell."],
  "rule.tag_found":
    ["Found in your load order",
     "The scope tag's tooltip when the name is in the load order."],
  "rule.tag_no_scope":
    ["No cell or region with this name exists in your load order, in any case.",
     "The scope tag's tooltip when the name is not in the load order."],
  "rule.scope_placeholder":
    ["Bitter Coast Region",
     "Right panel, the scope field: the placeholder (an example region)."],
  "rule.scope_pick_title":
    ["Pick a region or a cell from your load order",
     "The \"…\" button beside the scope field: its tooltip."],
  "connect.no_plugins":
    ["No plugins scanned yet — connect a setup first.",
     "Toast when the scope picker is opened before a setup is connected."],
  "rule.scope_region_has":
    ["region · has this texture",
     "The scope picker: the pill on a region where the rule's texture is painted."],
  "rule.scope_cell_has":
    ["cell · has this texture",
     "The scope picker: the pill on a cell where the rule's texture is painted."],
  "rule.scope_pick_head":
    ["Where does this rule apply?",
     "The scope picker's heading."],
  "rule.scope_pick_filter":
    ["Filter regions and cells…",
     "The scope picker's filter placeholder."],
  "rule.scope_pick_nothing":
    ["No region or cell matches that.",
     "The scope picker when the filter matches nothing."],
  /* Round 18ap: keyed for the language packs — the slots and bans sections and the grass card (09_ui.js). */
  "panel.sec_slots":
    ["Grass slots",
     "Right panel: the grass slots section's heading (with several rules selected)."],
  "panel.sec_slots_n":
    ["Grass slots ({n})",
     "Right panel: the grass slots section's heading, with the count."],
  "panel.sec_clumping":
    ["Organic clumping ({state})",
     "Right panel: the clumping section's heading; {state} is on, off or — when the rules differ."],
  "panel.state_on":
    ["on",
     "A section heading's state word: the feature is on."],
  "panel.state_off":
    ["off",
     "A section heading's state word: the feature is off."],
  "panel.sec_bans":
    ["Exclusions / bans",
     "Right panel: the bans section's heading (with several rules selected)."],
  "panel.sec_bans_n":
    ["Exclusions / bans ({n})",
     "Right panel: the bans section's heading, with the count."],
  "panel.paste_slot":
    ["⤵ Paste {name}",
     "Right panel, the grass slots section: the button that pastes the copied card; {name} is its mesh or id."],
  "panel.slot_word":
    ["slot",
     "The paste-card button's {name} when the copied card has neither mesh nor id."],
  "panel.paste_slot_title":
    ["Add the copied grass card to this texture",
     "The paste-card button's tooltip."],
  "panel.slot_pasted":
    ["Added {name} to {rule}",
     "Toast after pasting a grass card onto a rule."],
  "panel.copied_slot":
    ["the copied slot",
     "The paste toast's {name} when the card has neither mesh nor id."],
  "panel.slots_added":
    ["Added {n} slot(s)",
     "Toast after Add meshes made a slot per chosen mesh."],
  "panel.ban_placeholder":
    ["banned land texture",
     "Right panel, a ban row: the texture field's placeholder."],
  "panel.ban_del_title":
    ["Delete this exclusion",
     "Right panel, a ban row: the delete button's tooltip."],
  "panel.ban_fade":
    ["Fade grass",
     "Right panel, under a ban: the switch that shrinks grass towards the cleared ring."],
  "panel.ban_fade_over":
    ["over",
     "Right panel, the fade row: the word before the distance field (\"Fade grass over N u, down to M\")."],
  "panel.ban_fade_dist_tip":
    ["How far the shrinking runs, in units, measured out from the edge of the cleared ring",
     "The fade row: the distance field's tooltip."],
  "panel.ban_fade_down_to":
    ["u, down to",
     "Right panel, the fade row: the words between the distance and the minimum size."],
  "panel.ban_fade_min_tip":
    ["The size a blade is at the very edge of the cleared ring, as a fraction of full",
     "The fade row: the minimum-size field's tooltip."],
  "panel.add_ban":
    ["+ Add ban",
     "Right panel, the bans section: the button that adds an empty ban."],
  "panel.ban_io":
    ["⧉ Copy / paste list",
     "Right panel, the bans section: the button that opens the ban list as text."],
  "panel.ban_io_title":
    ["Copy this ban list, or paste one in from another section or an .ini",
     "The ban list button's tooltip."],
  "panel.ban_io_head":
    ["Exclusions / bans — {rule}",
     "The ban list dialogue's heading; {rule} is the rule's texture."],
  "panel.ban_io_hint":
    ["Written in the Groundcover Generator .ini’s own spelling, so this pastes into another rule here or into one of those .ini files — not into a Gardenfell .toml, which writes bans differently. Bare texture names and <code>name = offset</code> lines are accepted too.",
     "The ban list dialogue: the hint over the text area."],
  "panel.ban_io_empty":
    ["This rule has no bans yet — paste some in.",
     "The ban list dialogue: the note when the rule has no bans."],
  "panel.bans_replaced.one":
    ["Ban list replaced with {n} entry",
     "Toast after the ban list dialogue replaced the list."],
  "panel.bans_replaced.other":
    ["Ban list replaced with {n} entries",
     "Toast after the ban list dialogue replaced the list."],
  "panel.bans_added.one":
    ["Added {n} entry",
     "Toast after the ban list dialogue appended to the list."],
  "panel.bans_added.other":
    ["Added {n} entries",
     "Toast after the ban list dialogue appended to the list."],
  "panel.ban_offer_cell":
    ["Also painted in this cell — click to ban:",
     "Right panel, the bans section: the label over the offered textures in Real cell mode."],
  "panel.ban_offer_patch":
    ["Also on this patch — click to ban:",
     "Right panel, the bans section: the label over the offered textures in Simplified mode."],
  "panel.ban_offer_pct":
    ["{pct}% of the cell —",
     "The offered-texture button's tooltip: how much of the cell the texture covers, before the \"add\" part."],
  "panel.ban_offer_title":
    ["add [{tex}] to this rule’s exclusions",
     "The offered-texture button's tooltip."],
  "card.tag_empty":
    ["empty",
     "A grass card's tag: the slot is empty (no mesh, or marked Empty / Placeholder)."],
  "card.tag_object_id":
    ["object ID",
     "A grass card's tag: the slot names an object id rather than a mesh."],
  "card.tag_no_folder":
    ["no folder",
     "A grass card's tag: no Data Files folder is connected, so the mesh cannot be checked."],
  "card.tag_missing":
    ["missing",
     "A grass card's tag: the mesh is not in the install."],
  "card.tag_invisible":
    ["invisible",
     "A grass card's tag: the mesh has no visible geometry."],
  "card.tag_tris":
    ["{n} tri",
     "A grass card's tag: the mesh's triangle count."],
  "card.mesh_not_installed":
    ["This mesh is not in your install.",
     "A grass card's swatch tooltip when the mesh is missing and the engine gave no reason."],
  "card.copy_title":
    ["Copy this grass card, to paste into another rule",
     "A grass card: the copy button's tooltip."],
  "card.copied":
    ["Copied {name}",
     "Toast after copying a grass card; {name} is its id or mesh."],
  "card.an_empty_slot":
    ["an empty slot",
     "The copy toast's {name} when the card has neither mesh nor id."],
  "card.del_title":
    ["Delete this grass card",
     "A grass card: the delete button's tooltip."],
  "card.the_empty_slot":
    ["the empty slot",
     "The delete-card dialogue's {name} for an Empty / Placeholder card."],
  "card.this_slot":
    ["this slot",
     "The delete-card dialogue's {name} for a card with neither mesh nor id."],
  "card.del_head":
    ["Delete this grass card?",
     "The dialogue before deleting a grass card: its title."],
  "card.del_body":
    ["{name} at weight {weight}. The other slots keep their weights, so the ones left take a larger share.",
     "That dialogue's body; {name} is bold."],
  "card.weight":
    ["weight",
     "A grass card: the weight field's label."],
  "card.empty":
    ["Empty / Placeholder",
     "A grass card: the switch that makes the slot take positions and place nothing."],
  "card.empty_note":
    ["Takes a position and places nothing — gaps without changing the spacing.",
     "A grass card: the note under Empty / Placeholder while it is on."],
  /* Round 18bf: the card names a mesh **or** an object, chosen here rather than inferred
     from whichever field you filled in last. */
  "card.source":
    ["Places",
     "A grass card: the label of the chooser that says whether the card names a mesh or an object from the load order."],
  "card.source_title":
    ["Whether this card places a mesh from your Data Files or a record from your load order. The other one is remembered while you try this one, so you can switch back without typing it again",
     "A grass card: the Places chooser's tooltip."],
  "card.source_mesh":
    ["a mesh",
     "A grass card: the Places chooser's first option."],
  "card.source_object":
    ["an object from the load order",
     "A grass card: the Places chooser's second option."],
  "card.mesh":
    ["mesh",
     "A grass card: the mesh field's label."],
  "card.mesh_ph":
    ["Grass\\azbc03.nif",
     "A grass card: the mesh field's placeholder (an example path)."],
  "card.browse_title":
    ["Browse meshes in your Data Files folder",
     "A grass card: the \"…\" button's tooltip beside the mesh field."],
  /* Round 18av: a card naming an object from the load order (20_objects.js). */
  "card.object":
    ["Object",
     "A grass card: the label of the object-id field, under the mesh row."],
  "card.object_ph":
    ["or an object from the load order, by id",
     "A grass card: the object-id field's placeholder."],
  "card.object_pick":
    ["From the load order…",
     "A grass card: the button that opens the object picker."],
  "card.object_pick_title":
    ["Pick a static, an activator, a creature or a leveled creature from your load order. The card is then that object rather than a mesh, and its references go into the Objects .esp on export",
     "A grass card: the object picker button's tooltip."],
  "card.object_note":
    ["{kind}: {name}, from {from}. Its references go into the Objects .esp.",
     "A grass card: the note under an object-id card that the load order defines. {kind} is the kind word, {name} the record's name or id, {from} the plugin that last defines it."],
  "card.object_unknown_note":
    ["Nothing in the load order defines {id} — check the id, or connect the setup that has it.",
     "A grass card: the note under an object-id card no plugin defines."],
  "card.object_unknown_title":
    ["No record with this id in the load order",
     "A grass card: the tag's tooltip when the object id is unknown."],
  "objects.kind_static":
    ["static",
     "The word for a STAT record — the object picker, a card's tag, the export dialogue."],
  "objects.kind_activator":
    ["activator",
     "The word for an ACTI record."],
  "objects.kind_creature":
    ["creature",
     "The word for a CREA record."],
  "objects.kind_leveled":
    ["leveled creature",
     "The word for a LEVC record — a list the game picks a creature from."],
  "objects.kind_unknown":
    ["object",
     "The word when the kind is not known."],
  "objects.pick_head":
    ["From the load order",
     "The object picker's title."],
  "objects.pick_hint":
    ["Every static, activator, creature and leveled creature your load order defines. Type part of an id or a name; the chips narrow the kinds. A leveled creature draws as its creatures' mesh when they all share one, and as the Construction Set's creature marker when they do not.",
     "The object picker's hint."],
  "objects.pick_ph":
    ["Type an id or a name…",
     "The object picker's search field placeholder."],
  "objects.pick_n":
    ["{n} of {total} shown",
     "The object picker's count line."],
  "objects.pick_more":
    ["The first {n} — type more to narrow it down",
     "The object picker's count line when the list was cut."],
  /* Round 18bg: the picker's two views and what a tile says when there is nothing to
     draw. Robin: "make the reference 'from the load order' picker have the option of
     either list view (what we have now) or a grid view like other pickers […] If the
     object misses something to render completely, write a message in the grid square
     'No visuals'." */
  "objects.list":
    ["List",
     "The object picker: the button for the list view."],
  "objects.list_title":
    ["One object a line, with its id, name, kind and plugin",
     "The object picker: the list view button's tooltip."],
  "objects.grid":
    ["Grid",
     "The object picker: the button for the grid of tiles."],
  "objects.grid_title":
    ["The objects themselves, drawn, so you can pick by looking",
     "The object picker: the grid view button's tooltip."],
  "objects.no_visuals":
    ["No visuals",
     "An object picker tile for a record with nothing to draw — no model, or a model the install does not ship."],
  "card.n_set":
    ["{n} set",
     "A grass card: the count on a folded Overrides or Conditions heading."],
  "card.scale":
    ["scale",
     "A grass card, Overrides: the scale range's label."],
  "card.scale_tip":
    ["The size range this mesh is drawn at. Blank inherits the rule’s.",
     "A grass card, Overrides: the scale range's tooltip."],
  "card.spacing":
    ["spacing",
     "A grass card, Conditions: the spacing field's label."],
  "card.spacing_tip":
    ["How close two of this mesh may stand, in the rule’s units. The rule’s spacing still holds between every blade - this is a cap on this mesh alone, so a stalk at 600 among grass at 100 is one stalk at most per 600, with the grass filling in between. Blank is the rule’s; never under it.",
     "A grass card, Conditions: the spacing field's tooltip."],
  "card.height":
    ["lowest / highest",
     "A grass card, Conditions: the ground-height pair's label."],
  "card.height_tip":
    ["Ground height, in units, this mesh keeps between - the rule’s other meshes stand outside it. Blank inherits the rule’s.",
     "A grass card, Conditions: the ground-height pair's tooltip."],
  "card.within_blank":
    ["Blank is twice the rule’s spacing:",
     "A grass card, the \"within\" field's tooltip: what a blank means, followed by the number."],
  /* Round 18ap: keyed for the language packs — painting tools, the import and export dialogues, the report load order (11_events.js). */
  "left.fold_show_title":
    ["Show the list of ground textures",
     "Left column: the fold button's tooltip while the rules list is folded."],
  "left.fold_hide_title":
    ["Fold the list of ground textures away",
     "Left column: the fold button's tooltip while the rules list is open."],
  "paint.rule_default_name":
    ["Paint rule {n}",
     "The name a new paint-only rule is offered: numbered after the ones that exist."],
  "paint.name_title":
    ["Name this paint rule",
     "The dialogue that names a new paint-only rule: its heading."],
  "paint.name_body":
    ["A paint rule belongs to no land texture: it grows where you paint its colour and nowhere else, so this name is simply what you want to call it.",
     "That dialogue's body."],
  "paint.name_go":
    ["Create the rule",
     "That dialogue's confirming button."],
  "preview.statics_forced_title":
    ["Always on in interiors",
     "Left column, Preview: the Draw objects row's tooltip while an interior forces it on."],
  "vp.back_to_preview":
    ["Back to preview",
     "Viewport: the button that leaves the mesh editor."],
  "paint.mode_on":
    ["Painting — click to stop",
     "Left column, Painting tools: the brush button while painting is on. Round 18bc: the ✓ left the string and became the .ic-done mark beside it."],
  "paint.mode_off":
    ["Start painting",
     "Left column, Painting tools: the brush button while painting is off. Round 18bc: the ✎ left the string and became the .ic-palette mark beside it."],
  "paint.colour_off_title":
    ["The rule {rule} is switched off: ground painted with this colour counts as painted with no rule until it is switched on again. The paint is kept.",
     "Painting tools, a colour button's tooltip while its rule is switched off."],
  "paint.colour_title_tip":
    ["Paint ground for {rule} — right-click for what else can be done with this colour",
     "Painting tools, a colour button's tooltip."],
  "paint.colour_orphan_title":
    ["These grass rules have no {rule}, so this colour places nothing until it is back. Right-click to point it at another rule, or to remove it.",
     "Painting tools, a colour button's tooltip when no rule matches it; {rule} is the missing rule's name."],
  "paint.matching_rule":
    ["matching rule",
     "The orphan colour tooltip's {rule} when the colour names no rule at all."],
  "paint.painted_in.one":
    ["Painted in {n} place:",
     "The colour menu: the heading over the cells a colour is painted in."],
  "paint.painted_in.other":
    ["Painted in {n} places:",
     "The colour menu: the heading over the cells a colour is painted in."],
  "paint.painted_nowhere":
    ["This colour is painted nowhere yet.",
     "The colour menu when the colour has no paint anywhere."],
  "paint.cover_grass":
    ["Grass",
     "The cover colours' names, used in their menus and dialogues: the one that forces grass."],
  "paint.cover_bare":
    ["No grass",
     "The cover colours' names, used in their menus and dialogues: the one that forbids it."],
  "paint.failed":
    ["Could not paint: {err}",
     "Toast when a brush stroke failed in the engine."],
  "paint.this_object":
    ["this object",
     "The crowded-mesh dialogue: what to call an object with no mesh path."],
  "paint.n_cells_painted.one":
    ["{n} cell painted, saved in your profile.",
     "Painting tools: the count line under the tools."],
  "paint.n_cells_painted.other":
    ["{n} cells painted, saved in your profile.",
     "Painting tools: the count line under the tools."],
  "paint.nothing_painted":
    ["Nothing painted yet.",
     "Painting tools: the count line before anything is painted."],
  "file.kind_rules":
    ["Gardenfell rules",
     "The save dialogue's file-type name for a rules file."],
  "top.folder_report_title":
    ["Click for the plugin load order and asset report",
     "Top bar: the connected-setup label's tooltip."],
  "connect.what_folder":
    ["a plain Data Files folder",
     "The switch-setup dialogue's {what}: a Data Files folder."],
  "connect.what_mo2":
    ["a Mod Organizer 2 setup",
     "The switch-setup dialogue's {what}: an MO2 setup."],
  "connect.what_openmw":
    ["an OpenMW configuration",
     "The switch-setup dialogue's {what}: an OpenMW configuration."],
  "connect.what_other":
    ["another setup",
     "The switch-setup dialogue's {what} when the kind is unknown."],
  "import.objects_note.one":
    ["{n} slot in {rules} points at an object from the load order instead of a mesh — a static, an activator, a creature or a leveled creature by id. It is kept as it is: the preview draws the object's own model, and the export writes its references into the Objects .esp, a plugin the game loads as a mod.",
     "The import dialogue: the note over the slots that name object ids (one slot)."],
  "import.objects_note.other":
    ["{n} slots in {rules} point at objects from the load order instead of meshes — statics, activators, creatures or leveled creatures by id. They are kept as they are: the preview draws each object's own model, and the export writes their references into the Objects .esp, a plugin the game loads as a mod.",
     "The import dialogue: the note over the slots that name object ids."],
  "import.n_rules.one":
    ["{n} rule",
     "The import dialogue: a count of rules."],
  "import.n_rules.other":
    ["{n} rules",
     "The import dialogue: a count of rules."],
  "import.skip":
    ["Skip",
     "The import dialogue, a clashing rule: the choice that keeps yours."],
  "import.skip_title":
    ["Leave the rule you already have; ignore the imported one",
     "The Skip choice's tooltip."],
  "import.replace":
    ["Replace",
     "The import dialogue, a clashing rule: the choice that takes the imported one."],
  "import.replace_title":
    ["Throw away the existing rule and use the imported one",
     "The Replace choice's tooltip."],
  "import.add_extra":
    ["Add as extra",
     "The import dialogue, a clashing rule: the choice that keeps both."],
  "import.add_extra_title":
    ["Keep both — each places its own grass, so this ground gets denser",
     "The Add as extra choice's tooltip."],
  "import.across_files":
    ["across these files",
     "The import dialogue's {where} when several files are imported."],
  "import.in_file":
    ["in this file",
     "The import dialogue's {where} for one file."],
  "import.intro_clash":
    ["{rules} rule(s) {where}. {fresh} cover ground you have no rule for and are added; {clash} land on ground you already cover, and those are yours to decide.",
     "The import dialogue's introduction when some rules clash with yours."],
  "import.intro_clean":
    ["{rules} rule(s) {where}, none of which collide with what you have. They are added to the set you have open — nothing is replaced.",
     "The import dialogue's introduction when nothing clashes."],
  "import.count_added":
    ["{n} added",
     "The import dialogue's footer: rules that will be added."],
  "import.count_replaced":
    ["{n} replaced",
     "The import dialogue's footer: rules that will replace yours."],
  "import.count_alongside":
    ["{n} added alongside",
     "The import dialogue's footer: rules added beside yours."],
  "import.count_skipped":
    ["{n} skipped",
     "The import dialogue's footer: rules that will be skipped."],
  "import.count_nothing":
    ["nothing to import",
     "The import dialogue's footer when every rule is skipped."],
  "esp.n_of":
    ["{n} of {total}",
     "The export dialogue's rule list: ticked out of all, on a group heading."],
  "esp.n_ticked":
    ["{n} of {total} ticked",
     "The export dialogue's rule list: how many rules are ticked."],
  "esp.n_shown":
    ["{n} shown by the filter",
     "The export dialogue's rule list: how many the filter shows, after \" · \"."],
  "esp.nothing_ticked":
    ["<b>Nothing ticked.</b> Tick at least one rule &mdash; a rules file with no rules in it is not worth sending.",
     "The export dialogue when no rule is ticked."],
  "esp.nothing_to_export":
    ["<b>There are no rules to export.</b> Add one first.",
     "The export dialogue when there are no rules at all."],
  "esp.foot_all":
    ["the whole set — {rules}",
     "The export dialogue's footer when every rule is ticked; {rules} is the count."],
  "esp.foot_some":
    ["{n} of {total} rules",
     "The export dialogue's footer when some rules are ticked."],
  "esp.show_first_10":
    ["Show the first 10 only",
     "The export dialogue's problem list: the button that folds a long list back."],
  "esp.problems_head":
    ["problem(s) that will break generation:",
     "The export dialogue: the heading over the problems, after their count."],
  "esp.read_all_problems":
    ["Read all problems",
     "The export dialogue: the button that unfolds every problem."],
  "esp.warnings_head":
    ["warning(s):",
     "The export dialogue: the heading over the warnings, after their count."],
  "esp.read_all_warnings":
    ["Read all warnings",
     "The export dialogue: the button that unfolds every warning."],
  "report.order_read":
    ["Read from {source} — {n} plugin(s), in the order the game loads them. Later entries override earlier ones. This should match MO2's right pane top-to-bottom.",
     "The report dialogue: where the load order came from; {source} is bold."],
  "report.order_none":
    ["<b>No plugin load order found.</b> Looked for the MO2 profile's Morrowind.ini [Game Files], then loadorder.txt / plugins.txt, then Morrowind.ini in Data Files. Texture and region checking stays off until one is found.",
     "The report dialogue when no load order was found."],
  /* Round 18ap: keyed for the language packs — profiles and the rebind dialogue (23_profiles.js). */
  "profiles.saving_into":
    ["Settings are being saved into {file}",
     "Settings dialogue: the profile name pill's tooltip."],
  "profiles.none_yet":
    ["No profile yet",
     "Settings dialogue: the profile name pill's tooltip before a profile exists."],
  "profiles.hint_long":
    ["Everything under <b>Generation</b> and <b>Preview</b> is <b>yours</b>, not the grass rules’ — it applies to whichever rules you load. Each profile is a <code>.profile.toml</code> beside the app, saved as you work and reopened on start; switching leaves the one you came from exactly as it was.",
     "Settings dialogue: the note under the profile row."],
  "profiles.welcome_title":
    ["Welcome — name your profile",
     "The first-run dialogue's heading."],
  "profiles.welcome_hint_alt":
    ["Gardenfell keeps your obstacle avoidance, placement, export and viewport settings in a profile, saved as you work. Name this one; you can add more later, and rename this whenever you like. Or, if you already have a folder of profiles and grass rules from before, load it — Gardenfell will keep everything there from now on.",
     "The first-run dialogue's body, with the offer to load an existing folder."],
  "profiles.default_name":
    ["Default",
     "The name the first profile is offered."],
  "profiles.create_go":
    ["Create profile",
     "The first-run dialogue's confirming button."],
  "profiles.welcome_alt":
    ["Load profiles and rules from existing folder",
     "The first-run dialogue's other button: point Gardenfell at an existing folder."],
  "profiles.folder_pick_title":
    ["The folder that holds your profiles and grass rules",
     "The folder picker's title when loading an existing folder on first run."],
  "profiles.folder_failed":
    ["Could not use that folder: {err}",
     "Toast when the chosen profiles folder could not be used."],
  "profiles.folder_empty":
    ["No profiles or grass rules in that folder. It is where Gardenfell will keep them from now on.",
     "Toast when the chosen folder holds nothing yet."],
  "profiles.folder_loaded":
    ["Loaded {profiles} and {sets} of grass rules from {dir}.",
     "Toast after loading an existing folder; {profiles} and {sets} are the two counts."],
  "profiles.n_profiles.one":
    ["{n} profile",
     "A count of profiles."],
  "profiles.n_profiles.other":
    ["{n} profiles",
     "A count of profiles."],
  "profiles.n_sets.one":
    ["{n} set",
     "A count of rules sets, before \"of grass rules\"."],
  "profiles.n_sets.other":
    ["{n} sets",
     "A count of rules sets, before \"of grass rules\"."],
  "profiles.welcome_hint":
    ["Gardenfell keeps your obstacle avoidance, placement, export and viewport settings in a profile, saved as you work. Name this one; you can add more later, and rename this whenever you like.",
     "The first-run dialogue's body when a folder is already set."],
  "profiles.no_engine":
    ["The engine is not running.",
     "Toast when the profiles dialogue is opened with no engine."],
  "profiles.kept_in":
    ["Kept in {dir}",
     "The profiles dialogue: the folder the profiles live in."],
  "profiles.none_listed":
    ["No profiles yet.",
     "The profiles dialogue when there are none."],
  "profiles.in_use":
    ["● in use",
     "The profiles dialogue: the button on the active profile."],
  "profiles.use":
    ["Use",
     "The profiles dialogue: the button that switches to a profile."],
  "profiles.now_using":
    ["Now using {name}",
     "Toast after switching profile."],
  "profiles.rename":
    ["Rename",
     "The profiles dialogue: the rename button, and the rename dialogue's confirming button."],
  "profiles.rename_title":
    ["Rename profile",
     "The rename-profile dialogue's heading."],
  "profiles.rename_hint":
    ["The file is renamed to match, so it still makes sense in the folder.",
     "The rename-profile dialogue's body."],
  "profiles.delete":
    ["Delete",
     "The profiles dialogue: the delete button."],
  "profiles.only_one":
    ["This is your only profile",
     "The delete button's tooltip when it is disabled because there is one profile."],
  "profiles.delete_title":
    ["Delete {file}",
     "The delete button's tooltip."],
  "profiles.delete_armed":
    ["Delete {file}?",
     "The delete button after one press: press again to confirm."],
  "profiles.deleted":
    ["Deleted {name} — {file} removed",
     "Toast after deleting a profile."],
  "profiles.new_title":
    ["New profile",
     "The new-profile dialogue's heading, and the name it offers."],
  "profiles.new_hint":
    ["Starts from the defaults. Your current profile is left as it is.",
     "The new-profile dialogue's body."],
  "profiles.not_a_profile":
    ["That does not read as a profile: {err}",
     "Toast when an added file does not parse as a profile."],
  "profiles.make_rules_failed":
    ["Could not make those rules: {err}",
     "Toast when the rules a profile's colours asked for could not be made."],
  "profiles.match_rules_failed":
    ["Could not match those rules: {err}",
     "Toast when repointing a profile's colours failed."],
  "profiles.add_title":
    ["Add this profile",
     "The add-profile dialogue's heading."],
  "profiles.add_hint":
    ["It is copied into your profiles folder, so it becomes one of yours.",
     "The add-profile dialogue's body."],
  "profiles.add_go":
    ["Add",
     "The add-profile dialogue's confirming button."],
  "profiles.added":
    ["Added {name}{matched}{made}",
     "Toast after adding a profile; {matched} and {made} are the clauses below or nothing."],
  "profiles.n_matched.one":
    ["{n} colour matched to your rules",
     "The added-profile toast: how many colours were repointed."],
  "profiles.n_matched.other":
    ["{n} colours matched to your rules",
     "The added-profile toast: how many colours were repointed."],
  "profiles.n_new":
    ["({n} new)",
     "The added-profile toast: how many rules were made for it."],
  "paint.rule_word":
    ["Paint rule",
     "The name a rule made for an unnamed profile colour gets."],
  "rebind.same_for_all":
    ["Do the same for all:",
     "The rebind dialogue (matching colours or mesh setups to rules): the label before the three all-rows buttons."],
  "rebind.keep_all":
    ["Keep all",
     "The rebind dialogue: sets every row to Keep."],
  "rebind.repoint_all":
    ["Repoint all",
     "The rebind dialogue: sets every row to Repoint."],
  "rebind.create_all":
    ["Create all",
     "The rebind dialogue: sets every row to Create new."],
  "rebind.title":
    ["Match this profile’s colours to your rules",
     "The rebind dialogue's heading when a profile is added."],
  "rebind.go":
    ["Add the profile",
     "The rebind dialogue's confirming button when a profile is added."],
  "rebind.intro.one":
    ["{n} rule your mesh setups and paint colours point at is not in the set you are opening.",
     "The rebind dialogue's introduction (one rule)."],
  "rebind.intro.other":
    ["{n} rules your mesh setups and paint colours point at are not in the set you are opening.",
     "The rebind dialogue's introduction."],
  "rebind.intro_matched.one":
    ["{n} of them has a rule of the same name here, already filled in.",
     "The rebind dialogue's introduction: how many rows start matched (one)."],
  "rebind.intro_matched.other":
    ["{n} of them have a rule of the same name here, already filled in.",
     "The rebind dialogue's introduction: how many rows start matched."],
  "rebind.n_repointed":
    ["{n} repointed",
     "The rebind dialogue's footer: rows set to Repoint."],
  "rebind.n_new_rules.one":
    ["{n} new rule",
     "The rebind dialogue's footer: rows set to Create new."],
  "rebind.n_new_rules.other":
    ["{n} new rules",
     "The rebind dialogue's footer: rows set to Create new."],
  "rebind.n_kept":
    ["{n} kept",
     "The rebind dialogue's footer: rows set to Keep."],
  "rebind.nothing_to_change":
    ["nothing to change",
     "The rebind dialogue's footer when every row is kept."],
  "rebind.stuck_title":
    ["Choose a rule for {name}, or set it to Keep",
     "The rebind dialogue: the confirming button's tooltip while a Repoint row has no rule chosen."],
  "rebind.n_mesh_setups.one":
    ["{n} mesh setup",
     "The rebind dialogue, under a rule's name: how many mesh setups lean on it."],
  "rebind.n_mesh_setups.other":
    ["{n} mesh setups",
     "The rebind dialogue, under a rule's name: how many mesh setups lean on it."],
  "rebind.n_colours.one":
    ["{n} paint colour",
     "The rebind dialogue, under a rule's name: how many paint colours lean on it."],
  "rebind.n_colours.other":
    ["{n} paint colours",
     "The rebind dialogue, under a rule's name: how many paint colours lean on it."],
  "rebind.on_layer":
    ["on layer {n}",
     "The rebind dialogue, under a rule's name: which paint layer the colour is on."],
  "rebind.no_id":
    ["no id",
     "The rebind dialogue: a row's tooltip when the rule has no id."],
  "rebind.to_new":
    ["new: {name}",
     "The rebind dialogue, a row set to Create new: what it becomes."],
  "rebind.to_new_title":
    ["A rule of this name is made in the set you are opening, copied from the one you are leaving, and this points at it",
     "That pill's tooltip."],
  "rebind.to_keep":
    ["stays as it is",
     "The rebind dialogue, a row set to Keep: what it becomes."],
  "rebind.to_keep_title":
    ["Nothing is repointed. It keeps the id it has and grows nothing until you open a set that has that rule again.",
     "That pill's tooltip."],
  "rebind.to_choose":
    ["— choose a rule —",
     "The rebind dialogue, a row set to Repoint with no rule chosen yet."],
  "rebind.to_becomes":
    ["Becomes {name}",
     "The rebind dialogue, a repointed row's pill tooltip."],
  "rebind.to_unanswered":
    ["Not answered yet",
     "The rebind dialogue: the pill's tooltip while no rule is chosen."],
  "rebind.pick_title":
    ["Choose which of your rules this becomes",
     "The rebind dialogue: the \"…\" button's tooltip."],
  "rebind.pick_head":
    ["Which rule does “{name}” become?",
     "The rebind dialogue: the rule picker's heading."],
  "rebind.keep":
    ["Keep",
     "The rebind dialogue, a row: the choice that leaves it alone."],
  "rebind.keep_title":
    ["Leave this one alone — it keeps the rule id it has now, and grows nothing until you open a set that has that rule.",
     "The Keep choice's tooltip."],
  "rebind.repoint":
    ["Repoint",
     "The rebind dialogue, a row: the choice that points it at a chosen rule."],
  "rebind.repoint_title":
    ["Point it at the rule chosen beside it.",
     "The Repoint choice's tooltip."],
  "rebind.create":
    ["Create new",
     "The rebind dialogue, a row: the choice that makes a rule of that name."],
  "rebind.create_title":
    ["Make a rule of this name in the set you are opening — copied from the one you are leaving, where there is one — and point this at it.",
     "The Create new choice's tooltip."],
  /* Round 18ap: keyed for the language packs — the rules sets (24_rules.js). */
  "rules.none_pill":
    ["no rules",
     "Top bar: the rules pill before any set exists."],
  "rules.saving_into":
    ["Grass rules are being saved into {file}",
     "Top bar: the rules pill's tooltip."],
  "rules.none_yet":
    ["No set of grass rules yet",
     "Top bar: the rules pill's tooltip before any set exists."],
  "rebind.rules_title":
    ["Match your setups and colours to the incoming rules",
     "The rebind dialogue's heading when a rules set is opened."],
  "rebind.rules_go":
    ["Open these rules",
     "The rebind dialogue's confirming button when a rules set is opened."],
  "rebind.rules_intro_matched.one":
    ["{n} of them has a rule of the same name there, already filled in.",
     "The rebind dialogue when opening rules: how many rows start matched (one)."],
  "rebind.rules_intro_matched.other":
    ["{n} of them have a rule of the same name there, already filled in.",
     "The rebind dialogue when opening rules: how many rows start matched."],
  "rebind.rules_intro_missing.one":
    ["{n} matches nothing there: keep it as it is, point it at a rule of yours, or make the rule here.",
     "The rebind dialogue when opening rules: how many rows match nothing (one)."],
  "rebind.rules_intro_missing.other":
    ["{n} match nothing there: keep them as they are, point them at a rule of yours, or make the rule here.",
     "The rebind dialogue when opening rules: how many rows match nothing."],
  "rules.matched":
    ["Matched {what} to the new rules{made}.",
     "Toast after opening a rules set repointed setups and colours; {made} is the clause below or nothing."],
  "rules.matched_and":
    [" and ",
     "The word joining \"N mesh setups\" and \"N paint colours\" in that toast."],
  "rules.matched_made.one":
    ["{n} of them to a rule made for the purpose",
     "That toast's clause: how many were pointed at rules made on the spot (one)."],
  "rules.matched_made.other":
    ["{n} of them to rules made for the purpose",
     "That toast's clause: how many were pointed at rules made on the spot."],
  "rules.default_name":
    ["Grass rules",
     "The name the first rules set is given."],
  "rules.not_saved_serialise":
    ["grass rules not saved — they could not be written out: {err}",
     "The report dialogue: a warning when the rules could not be turned into a file."],
  "rules.not_saved":
    ["grass rules not saved: {err}",
     "The report dialogue: a warning when the rules file could not be written."],
  "rules.none_listed":
    ["No grass rules yet.",
     "The rules dialogue when there are no sets."],
  "rules.left_as_was":
    ["Left as it was",
     "Toast when switching rules sets was cancelled in the matching dialogue."],
  "rules.deleted_kept_as":
    ["{name} (kept)",
     "The name this session's rules are saved under when the set they came from was deleted and the matching question was cancelled."],
  "rules.deleted_saved_new":
    ["the set was deleted — your rules were saved under a new name rather than written over another set",
     "Toast after deleting the open rules set and cancelling the matching question."],
  "rules.n_textures.one":
    ["{n} ground texture",
     "The rules dialogue, under a set's name: how many texture rules it has."],
  "rules.n_textures.other":
    ["{n} ground textures",
     "The rules dialogue, under a set's name: how many texture rules it has."],
  "rules.rename_title":
    ["Rename grass rules",
     "The rename-rules dialogue's heading."],
  "rules.only_one":
    ["This is your only set of grass rules",
     "The rules dialogue: the delete button's tooltip when there is one set."],
  "rules.new_title":
    ["New grass rules",
     "The new-rules dialogue's heading."],
  "rules.new_hint":
    ["Starts empty. The set you have open is left exactly as it is — add the first ground texture from a cell’s own list, or import an .ini.",
     "The new-rules dialogue's body."],
  "rules.new_name":
    ["New rules",
     "The name a new rules set is offered."],
  "rules.created":
    ["Created {name}",
     "Toast after creating a rules set."],
  "rules.add_title":
    ["Add these grass rules",
     "The add-rules dialogue's heading."],
  "rules.add_hint":
    ["They are added as a set of their own and opened. Nothing you have is changed.",
     "The add-rules dialogue's body."],
  "rules.added":
    ["Added {name}",
     "Toast after adding a rules set."],
  "rules.added_not_opened":
    ["Added {name} — not opened, so nothing was matched",
     "Toast after adding a rules set whose matching dialogue was cancelled."],
  "file.read_that_failed":
    ["Could not read that file: {err}",
     "Toast when a chosen file could not be read."],
  /* Round 18ap: keyed for the language packs — the export dialogue and its summaries (21_openmw.js). */
  "file.kind_plugin":
    ["Morrowind plugin",
     "The save and open dialogues' file-type name for a plugin."],
  "connect.export":
    ["Connect a setup first — export needs the landscape data.",
     "Toast when Export is pressed before a setup is connected."],
  "esp.reading_regions":
    ["Reading regions…",
     "The export dialogue: the region list while it is being read."],
  "esp.regions_failed":
    ["Could not read the regions: {err}",
     "The export dialogue: the region list when it could not be read."],
  "esp.no_landscape":
    ["This install has no exterior landscape, so there is nothing to split.",
     "The export dialogue: the region list when the install has no exteriors."],
  "esp.n_painted_rooms.one":
    ["{n} painted room",
     "The export dialogue's region list: the Interiors row's count."],
  "esp.n_painted_rooms.other":
    ["{n} painted rooms",
     "The export dialogue's region list: the Interiors row's count."],
  "esp.no_rooms_painted":
    ["no rooms painted yet",
     "The export dialogue's region list: the Interiors row before any room has paint."],
  "esp.n_cells.one":
    ["{n} cell",
     "The export dialogue: a count of cells."],
  "esp.n_cells.other":
    ["{n} cells",
     "The export dialogue: a count of cells."],
  "esp.n_plugins_written.one":
    ["{n} plugin will be written",
     "The export dialogue's region list: how many files the ticked regions make."],
  "esp.n_plugins_written.other":
    ["{n} plugins will be written",
     "The export dialogue's region list: how many files the ticked regions make."],
  "esp.no_region":
    ["(no region)",
     "The export dialogue's Advanced list: the group for cells outside every region."],
  "esp.adv_nothing":
    ["Nothing to choose from yet.",
     "The export dialogue's Advanced list when it is empty."],
  "esp.adv_unreached":
    ["no rule reaches it",
     "The export dialogue's Advanced list: under a cell none of the ticked rules place in."],
  "esp.adv_region_cells":
    ["{n} of {total} cells",
     "The export dialogue's Advanced list: under a region, ticked cells out of all."],
  "esp.adv_region_cells_rules":
    ["{n} of {can} with rules  ·  {total} cells",
     "The export dialogue's Advanced list: under a region, ticked cells out of those a rule reaches, and the total."],
  /* Round 18bf (§I22): the Advanced list's three tick counters. They were built by
     concatenation — `total.on+' of '+total.all+' ticked'` — so no language pack could
     reach them and `t_wording` could not see them. */
  /* Round 18bf (§I23): why Save is greyed when the engine could not write the file. It
     used to stay live and write a zero-byte one. */
  "esp.no_file_title":
    ["There is no file to save — the problems above are why. Fix one and this comes back.",
     "The export dialogue: the tooltip on Save and Copy while the .toml could not be generated."],
  "esp.adv_ticked":
    ["{n} of {total} ticked",
     "The export dialogue's Advanced list: the counter under each of the three filter boxes."],
  "esp.adv_shown":
    ["  \u00b7  {n} shown",
     "The export dialogue's Advanced list: appended to the tick counter while a filter is narrowing the rows. Begins with the separator."],
  "esp.adv_tick_cells":
    ["Tick at least one cell or room.",
     "The export dialogue's Advanced footer when no cell is ticked."],
  "esp.adv_tick_rules":
    ["Tick at least one rule.",
     "The export dialogue's Advanced footer when no rule is ticked."],
  "esp.and_n_rooms.one":
    ["and {n} room",
     "The Advanced footer: the rooms clause after the cell count."],
  "esp.and_n_rooms.other":
    ["and {n} rooms",
     "The Advanced footer: the rooms clause after the cell count."],
  "esp.adv_how_patch":
    ["merged into a copy of the chosen plugin",
     "The Advanced footer's {how} when patching a plugin."],
  "esp.adv_how_region":
    ["one .esp per region",
     "The Advanced footer's {how} when splitting by region."],
  "esp.adv_how_cell":
    ["one .esp per place",
     "The Advanced footer's {how} when splitting by cell."],
  "esp.adv_how_one":
    ["one .esp",
     "The Advanced footer's {how} for a single file."],
  "esp.adv_foot":
    ["{where}, {rules} — {how}.",
     "The Advanced footer: what will be exported, how."],
  "rule.unnamed_rule":
    ["(unnamed rule)",
     "What to call a rule with no texture and no name in lists."],
  "esp.adv_n_cards":
    ["{n} cards",
     "The export dialogue's Advanced rule list: under a rule, its grass card count."],
  "esp.adv_split_patch_title":
    ["A patch is one merged plugin, so it cannot be split.",
     "The Advanced split picker's tooltip while patching is on."],
  "esp.prefix_eg":
    ["e.g. {name}",
     "The export dialogue: the example file name under the prefix field."],
  "esp.tick_region":
    ["Tick at least one region, or export the whole world in one file.",
     "Toast when Export is pressed with the region split and nothing ticked."],
  "esp.adv_tick_cells_toast":
    ["Tick at least one cell or room to export.",
     "Toast when an Advanced export has no cell ticked."],
  "esp.adv_tick_rules_toast":
    ["Tick at least one rule — nothing would be placed.",
     "Toast when an Advanced export has no rule ticked."],
  "esp.adv_choose_patch":
    ["Choose the .esp to patch, or turn the switch off.",
     "Toast when patching is on but no plugin is chosen."],
  "busy.cancel":
    ["Cancel export",
     "The button under the loading bar that stops a running export."],
  "busy.gen_cells":
    ["{n} of {total} cells",
     "Under the export's bar, while it works; {n} is how many cells are done."],
  "busy.gen_at":
    ["{n} of {total} cells — {at}",
     "The same, naming the cell being scattered; {at} is its name or its grid."],
  "busy.stopping":
    ["stopping at the end of this cell…",
     "Under the export's bar once Cancel has been confirmed."],
  "busy.generating":
    ["Generating plugin",
     "The busy overlay while a plugin is generated."],
  "busy.walking_cells":
    ["walking cells",
     "The busy overlay's second line while a plugin is generated."],
  "busy.writing_files":
    ["Writing the plugin",
     "The busy overlay while the generated plugin is written to the folder you chose."],
  "esp.stop_title":
    ["Stop the export?",
     "The confirmation asked when Cancel export is pressed."],
  "esp.stop_body":
    ["The grass placed so far will be thrown away and no file will be written — nothing has been saved to disk yet, so there is nothing to clean up.",
     "Its body. Nothing is written until a folder has been chosen, which is why it can promise this."],
  "esp.stop_yes":
    ["Stop the export",
     "Its confirming button."],
  "esp.stop_yes_note":
    ["It will stop at the end of the cell it is on.",
     "Under that button."],
  "esp.stop_no":
    ["Keep going",
     "Its declining button."],
  "esp.stop_no_note":
    ["Leaves the export running.",
     "Under that button."],
  "esp.cancelled_summary":
    ["Export stopped. Nothing was written.",
     "The export dialogue's summary when the Save dialogue was closed without choosing a file."],
  "cell.tex_edit_none_title":
    ["No rule set up to edit",
     "The greyed-out pencil on a ground texture no rule covers."],
  "esp.stopped_summary":
    ["<b>Generation cancelled.</b> You stopped it before it finished, so the grass placed so far was thrown away and no file was written.",
     "The export dialogue's summary after Cancel export was pressed and confirmed."],
  "esp.nothing_placed":
    ["<b>Nothing was placed.</b> No cell in scope paints a texture your config covers, or every placement was culled.",
     "The export dialogue's summary when the run placed nothing."],
  /* Round 18av: the Objects .esp and its dependencies (21_openmw.js, `Deps`). */
  "esp.deps_masters":
    ["Masters",
     "Export dialogue, the objects block: the label before the master list."],
  "esp.deps_masters_title":
    ["Morrowind.esm, Tribunal.esm and Bloodmoon.esm always, and every other .esm a referenced record lives in. A master cannot be left out.",
     "Export dialogue, the objects block: the master list's tooltip."],
  "esp.dep_master":
    ["from {master} — a master",
     "Export dialogue, the objects block: a reference a master defines."],
  "esp.dep_copy":
    ["copied from {from} — only an .esp defines it, and a plugin cannot have an .esp as a master",
     "Export dialogue, the objects block: a reference copied from the one .esp that defines it."],
  "esp.dep_copy_choose":
    ["copied — several .esp files define it; copy from:",
     "Export dialogue, the objects block: a reference several .esp files define, before the picker."],
  "esp.dep_copy_choose_title":
    ["Which plugin's definition of {id} to copy into the Objects .esp",
     "Export dialogue, the objects block: the .esp picker's tooltip."],
  "esp.dep_missing":
    ["nothing in the load order defines it — its references will point at nothing",
     "Export dialogue, the objects block: a reference no plugin defines."],
  "esp.dep_item_master":
    ["{what} — from {from}, a master",
     "Export dialogue, the objects block: one dependency a master covers."],
  "esp.dep_item_copy":
    ["{what} — copied from {from}",
     "Export dialogue, the objects block: one dependency that would be copied."],
  "esp.dep_item_missing":
    ["{what} — not found in the load order",
     "Export dialogue, the objects block: one dependency nothing defines."],
  "esp.objects_summary":
    ["Objects .esp: {file} — {refs} references; masters {masters}.",
     "Export summary: the objects plugin's line. {file} is its name or count, {refs} its references, {masters} the master list."],
  "esp.objects_file":
    ["one file beside the grass",
     "Export summary: stands in for the objects plugin's name when there is one beside each grass file."],
  "esp.objects_n_files":
    ["{n} files beside the grass",
     "Export summary: stands in for the objects plugins' names in a split export."],
  "esp.objects_copied":
    ["Copied in: {list}.",
     "Export summary: the records copied from .esp files, each as id ← plugin."],
  "esp.objects_copied_deps":
    ["With {n} records they depend on.",
     "Export summary: how many dependency records were copied along."],
  "esp.objects_unmet":
    ["Not found in the load order, so not copied: {list}.",
     "Export summary: dependencies nothing defines."],
  "esp.objects_only_summary":
    ["No grass this time — the Objects .esp alone, in {took}.",
     "Export summary: the first sentence when only the objects plugin was written. {took} is the time taken."],
  "esp.patch_summary":
    ["<b>Merged into a copy of {source}</b> — {refs} object(s) added across {extended} of its own cell(s){added}. {statics}. Its own records, header and masters are untouched.",
     "The export summary's patch sentence; {source} is the patched plugin's file name, {added} the new-cells clause or nothing, {statics} the static-record clauses."],
  "esp.patch_new_cells":
    ["and {n} new one(s)",
     "The patch sentence's clause for cells the patch added."],
  "esp.patch_statics_added":
    ["{n} static record(s) added",
     "The patch sentence: static records added to the plugin."],
  "esp.patch_and":
    ["and ",
     "The patch sentence: joins the two static-record clauses."],
  "esp.patch_statics_shared":
    ["{n} it already had",
     "The patch sentence: static records the plugin already had."],
  "esp.one_summary":
    ["{name} — {refs} objects across {cells} cell(s), {statics} static record(s), {mb} MB.<br>Generated in {took}.",
     "The export summary for one plugin; {name} is bold, {took} the time taken."],
  "esp.exported":
    ["Exported {name}",
     "Toast after a plugin is written."],
  "esp.no_file_for":
    ["<b>No grass, so no file:</b> {list}.",
     "The export summary: the regions or cells that placed nothing and got no file."],
  "esp.spilled":
    ["<b>More than one file:</b> a plugin can number 16,777,215 objects, so the rest went beside it — {list}. Load them together with the first.",
     "The export summary: the continuation .esp files a huge export was split into."],
  "esp.many_summary":
    ["<b>{n} plugin(s)</b> — {refs} objects across {cells} cell(s), generated in {took}.",
     "The export summary for several plugins."],
  "esp.exported_n":
    ["Exported {n} plugin(s)",
     "Toast after several plugins are written."],
  "file.kind_profile":
    ["Gardenfell profile",
     "The save dialogue's file-type name for a profile."],
  /* Round 18ap: keyed for the language packs — the land-texture picker, the colour rule picker and the unmatched-rules dialogue (16_gamedata_ui.js). */
  "ltex.head":
    ["Land textures in your load order",
     "The land-texture picker's heading."],
  "ltex.filter":
    ["filter… (e.g. grass, BC_, T_Mw)",
     "The land-texture picker's filter placeholder."],
  "ltex.list":
    ["List",
     "The land-texture picker: the button for the list view."],
  "ltex.list_title":
    ["One texture a line, with its file and plugin",
     "The list view button's tooltip."],
  "ltex.grid":
    ["Grid",
     "The land-texture picker: the button for the grid of tiles."],
  "ltex.grid_title":
    ["The textures themselves, so you can pick by looking",
     "The grid view button's tooltip."],
  "paint.add_rule_first":
    ["Add a rule first — a colour stands for one.",
     "Toast when a paint colour is added before any rule exists."],
  "paint.pick_gap":
    ["gap {n}",
     "The colour's rule picker: a rule's spacing, after its slot count."],
  "rule.kind_ground":
    ["ground",
     "The colour's rule picker: the tag on a ground-texture rule."],
  "paint.pick_no_paint_rules":
    ["No paint-only rules yet. Tick \"Paint only rule\" on a rule to make one.",
     "The colour's rule picker, Paint only filter, when there are none."],
  "paint.pick_none_of_kind":
    ["No rules of this kind.",
     "The colour's rule picker when the chosen kind has no rules."],
  "paint.pick_rule":
    ["Which rule should this colour stand for?",
     "The colour's rule picker: its heading when the caller gives none."],
  "paint.pick_rule_why":
    ["Ground you paint with this colour grows the rule you pick, whatever land texture is under it.",
     "The colour's rule picker: the note under the heading."],
  "top.check_failed":
    ["⚠ name check failed",
     "Top bar: the warning pill when the rule names could not be checked."],
  "top.check_failed_title":
    ["The rule names could not be checked against your load order: {err}",
     "That pill's tooltip."],
  "top.n_unmatched":
    ["{n} unmatched",
     "Top bar, the warning pill: rules naming a texture or scope the load order lacks."],
  "top.n_meshes_missing.one":
    ["{n} mesh missing",
     "Top bar, the warning pill: grass cards whose mesh is not installed."],
  "top.n_meshes_missing.other":
    ["{n} meshes missing",
     "Top bar, the warning pill: grass cards whose mesh is not installed."],
  "top.unmatched_title":
    ["{n} rule(s) name something your install does not have — click for details",
     "The warning pill's tooltip."],
  "unmatched.head":
    ["Rules that match nothing",
     "The unmatched-rules dialogue's heading."],
  "dlg.close":
    ["Close",
     "A dialogue's Close button."],
  "unmatched.no_install":
    ["No install is connected, so there is nothing to check these names against.",
     "The unmatched-rules dialogue before a setup is connected."],
  "unmatched.all_good":
    ["Every rule resolves against your current load order.",
     "The unmatched-rules dialogue when every name is found."],
  "unmatched.names":
    ["<b>{n} name(s) will place nothing.</b> Nothing in your load order is called this, in any capitalisation — so it is a misspelling, or a plugin that defines it is not enabled. A rule that matches nothing places nothing and says nothing about it, which is why this list exists.",
     "The unmatched-rules dialogue: the box over the names that were not found."],
  "unmatched.meshes":
    ["<b>⚠ {n} mesh(es) are not in your install.</b> Those grass cards win positions and then draw nothing — in the viewport the positions are marked, and in the export the objects are written pointing at a file the game cannot load. Mesh paths are matched without regard to case, so this is a file that is not there rather than a name spelled wrong: install the mod it comes from, or point the card at something else.",
     "The unmatched-rules dialogue: the box over the missing meshes."],
  "unmatched.how":
    ["Each line is one rule and the name it could not find. <b>Take me to entry</b> closes this, selects that rule and flashes it in the list, and flashes the field the problem is in — the grass card for a missing mesh, the name or scope field otherwise. It only takes you there: nothing on this dialogue changes your config.",
     "The unmatched-rules dialogue: the note explaining the list."],
  "unmatched.go":
    ["Take me to entry",
     "The unmatched-rules dialogue: the button beside each row."],
  "unmatched.go_mesh_title":
    ["Closes this and selects [{rule}], then scrolls to the grass card that names {mesh} and flashes it. Nothing is changed.",
     "That button's tooltip for a missing mesh."],
  "unmatched.go_field_title":
    ["Closes this and selects [{rule}] in the rules list and flashes the {field} field. Nothing is changed.",
     "That button's tooltip for a name that was not found; {field} is the field's name (texture, scope…)."],
  /* Round 18ap: keyed for the language packs — the files folder and its move (19_settings.js). */
  "profiles.not_saved_read":
    ["profile not saved — settings could not be read: {err}",
     "The report dialogue: a warning when the settings could not be gathered for saving."],
  "profiles.not_saved":
    ["profile not saved: {err}",
     "The report dialogue: a warning when the profile file could not be written."],
  "settings.kept_in":
    ["Your files are kept in {dir}",
     "Top bar: the Settings button's tooltip, naming the files folder."],
  "settings.kept_beside":
    ["Your files are kept beside the program",
     "Top bar: the Settings button's tooltip when the files sit beside the program."],
  "settings.btn_title":
    ["Settings",
     "Top bar: the Settings button's tooltip before the folder is known."],
  "settings.store_readonly_note":
    ["The program’s own folder cannot be written to, so the default is your user folder.",
     "Settings dialogue: the note under the files folder when the program folder is read-only."],
  "settings.folder_read_failed":
    ["Could not read that folder: {err}",
     "Toast when the chosen files folder could not be read."],
  "settings.folder_same":
    ["That is already where your files are kept.",
     "Toast when the chosen files folder is the current one."],
  "settings.move_n_files.one":
    ["{n} file moves there from {from}.",
     "The move-files dialogue: how many files move (one)."],
  "settings.move_n_files.other":
    ["{n} files move there from {from}.",
     "The move-files dialogue: how many files move."],
  "settings.move_nothing":
    ["There is nothing to move — the folder you are leaving has no files of Gardenfell&rsquo;s in it.",
     "The move-files dialogue when nothing moves."],
  "settings.move_n_taken.one":
    ["{n} name is already taken there",
     "The move-files dialogue: names that clash (one), shown in amber."],
  "settings.move_n_taken.other":
    ["{n} names are already taken there",
     "The move-files dialogue: names that clash, shown in amber."],
  "settings.move_theirs":
    ["That folder also has mesh setups of its own{meshes}.",
     "The move-files dialogue: the folder has mesh setups; {meshes} is the count in brackets or nothing."],
  "settings.n_meshes_paren.one":
    ["({n} mesh)",
     "The mesh-setup count in brackets."],
  "settings.n_meshes_paren.other":
    ["({n} meshes)",
     "The mesh-setup count in brackets."],
  "settings.clash_title":
    ["There are already Gardenfell files in this folder",
     "The dialogue when the chosen files folder already holds Gardenfell files: its title."],
  "settings.clash_names":
    ["These names are taken there: {list}.",
     "That dialogue: the clashing file names."],
  "settings.clash_theirs":
    ["It also has mesh setups of its own{meshes}.",
     "That dialogue: the folder has mesh setups; {meshes} is the count in brackets or nothing."],
  "settings.merge":
    ["Merge",
     "That dialogue: the choice that merges."],
  "settings.merge_note":
    ["The mesh setups are put together through the same dialogue an imported .statics.toml goes through — you answer for each one. Profiles and grass rules cannot be merged into one another, so both are kept: the folder’s stay as they are and yours arrive beside them under a name of their own.",
     "The note under Merge."],
  "settings.overwrite":
    ["Overwrite",
     "That dialogue: the (red) choice that replaces the folder's files."],
  "settings.overwrite_note":
    ["Your files replace the ones in that folder. A copy of each replaced file is kept under backups / <its kind>, along with the last ten versions of everything else.",
     "The note under Overwrite."],
  "settings.move_title":
    ["Keep your files here?",
     "The move-files dialogue's title."],
  "settings.move_go":
    ["Move them there",
     "The move-files dialogue's confirming choice when files move."],
  "settings.use_folder_go":
    ["Use this folder",
     "The move-files dialogue's confirming choice when nothing moves."],
  "settings.move_failed":
    ["Could not move the files: {err}",
     "Toast when moving the files folder failed."],
  "settings.moved_n_files.one":
    ["{n} file moved",
     "The toast after moving the files folder: files moved (one)."],
  "settings.moved_n_files.other":
    ["{n} files moved",
     "The toast after moving the files folder: files moved."],
  "settings.n_not_moved":
    ["{n} could not be moved",
     "The move toast: files that could not be moved."],
  "settings.n_replaced.one":
    ["{n} file replaced there — a copy of each is kept under backups",
     "The move toast, Overwrite: files replaced (one)."],
  "settings.n_replaced.other":
    ["{n} files replaced there — a copy of each is kept under backups",
     "The move toast, Overwrite: files replaced."],
  "settings.n_renamed.one":
    ["{n} arrived under a name of its own: {list}",
     "The move toast, Merge: files renamed to avoid a clash (one)."],
  "settings.n_renamed.other":
    ["{n} arrived under a name of their own: {list}",
     "The move toast, Merge: files renamed to avoid a clash."],
  "settings.setups_merged":
    ["the mesh setups were merged",
     "The move toast, Merge: the two sets of mesh setups were merged."],
  "settings.merge_cancelled":
    ["merge cancelled — the mesh setups already there are the ones in use{count}; yours are still in {from}",
     "The move toast, Merge: the merge dialogue was cancelled; {count} is the mesh count in brackets or nothing."],
  "settings.n_left_behind":
    ["{n} left behind — the name was taken there",
     "The move toast: files not moved because their names were taken."],
  "settings.n_setups_adopted.one":
    ["{n} mesh setup taken from the folder",
     "The move toast: the folder's own mesh setups are in use (one)."],
  "settings.n_setups_adopted.other":
    ["{n} mesh setups taken from the folder",
     "The move toast: the folder's own mesh setups are in use."],
  "settings.setups_unreadable":
    ["the mesh setups already there could not be read — they are untouched",
     "The move toast: the folder's mesh setups could not be read."],
  "settings.profile_adopted":
    ["the profile already there is the one in use",
     "The move toast: the folder's profile of the same name is in use now."],
  "settings.rules_adopted":
    ["the grass rules already there are the ones in use",
     "The move toast: the folder's rules set of the same name is in use now."],
  "settings.rules_before_move":
    ["{name} (before the move)",
     "The name this session's rules are saved under when the folder's set of the same name is kept."],
  "settings.rules_saved_new":
    ["your grass rules were saved under a new name — the set already there was left alone",
     "The move toast: this session's rules were saved under a new name."],
  "settings.now_kept_in":
    ["Your files are now kept in {dir}.",
     "The move toast's first sentence."],
  "settings.store_pick_title":
    ["Where should Gardenfell keep your .toml files?",
     "The folder picker's title when choosing the files folder."],
  /* Round 18ap: keyed for the language packs — the viewport legend (06_gl.js). */
  "legend.shallower":
    ["shallower than {deg}",
     "Viewport legend, the \"No grass placed\" note: ground under the rule's lowest slope."],
  "legend.steeper":
    ["steeper than {deg}",
     "Viewport legend, the \"No grass placed\" note: ground over the rule's steepest slope."],
  "legend.facing_away":
    ["facing away from {deg}",
     "Viewport legend, the \"No grass placed\" note: ground facing away from the rule's direction."],
  "legend.this_card":
    ["(this card)",
     "Viewport legend: appended to the facing note when it is the card's own facing, in the mesh editor."],
  "legend.below":
    ["below {n}",
     "Viewport legend, the \"No grass placed\" note: ground under the rule's lowest height."],
  "legend.above":
    ["above {n}",
     "Viewport legend, the \"No grass placed\" note: ground over the rule's highest height."],
  "legend.wrong_shore":
    ["the wrong distance from the sea",
     "Viewport legend, the \"No grass placed\" note: the shoreline band."],
  "legend.wrong_curve":
    ["the wrong shape of ground",
     "Viewport legend, the \"No grass placed\" note: the curvature band."],
  "legend.wrong_object":
    ["the wrong distance from an object",
     "Viewport legend, the \"No grass placed\" note: the near-objects band."],
  "legend.canopy":
    ["what does or does not hang over it",
     "Viewport legend, the \"No grass placed\" note: a canopy condition."],
  "legend.no_grass":
    ["No grass placed",
     "Viewport legend: the cull overlay's entry; its note lists the reasons."],
  "legend.painted_bare":
    ["Painted: no grass",
     "Viewport legend: the No grass cover colour."],
  "legend.painted_grass":
    ["Painted: grass here",
     "Viewport legend: the Grass cover colour."],
  "legend.a_rule":
    ["a rule",
     "Viewport legend: what to call a painted rule with no name."],
  "legend.painted_rule":
    ["Painted: {rule}",
     "Viewport legend: a rule colour's entry."],
  "legend.brush":
    ["Brush",
     "Viewport legend: the brush ring while painting."],
  "legend.brush_note":
    ["where it will land",
     "The brush entry's note."],
  "legend.hover_texture":
    ["Hovered texture",
     "Viewport legend: the highlight of a hovered land texture."],
  "legend.hover_texture_note":
    ["the ground it covers",
     "The hovered-texture entry's note."],
  "legend.hover_slot":
    ["Hovered grass slot",
     "Viewport legend: the highlight of a hovered grass card."],
  "legend.hover_slot_note":
    ["every blade it placed",
     "The hovered-slot entry's note."],
  "legend.mesh_missing":
    ["Mesh not installed",
     "Viewport legend: the orange spikes where a missing mesh would stand."],
  "legend.mesh_missing_note":
    ["a position won and nothing to draw",
     "The mesh-not-installed entry's note."],
  "legend.reserved":
    ["Reserved, nothing drawn",
     "Viewport legend: the markers for empty and object-id slots."],
  "legend.reserved_note":
    ["empty or object-id slots",
     "The reserved entry's note."],
  /* Round 18ap: keyed for the language packs — connecting a setup and its log (13_mo2.js). */
  "busy.reading_install":
    ["Reading the install",
     "The busy overlay while a setup is connected."],
  "busy.resolving_overlay":
    ["resolving the overlay…",
     "The busy overlay's second line: working out which mod folders win."],
  "busy.drawing_scene":
    ["drawing the scene",
     "The connect's busy card, while the viewport rebuilds after the world has been read."],
  /* Round 18cx: the cell picker's map is read at the connect, so the picker opens with it. */
  "busy.reading_map":
    ["reading the map…",
     "The connect's busy card, while the cell picker's map is read ahead, before its cell count is known."],
  "busy.reading_map_n":
    ["reading the map… {done} of {total} cells",
     "The connect's busy card, while the cell picker's map is read ahead: {done} and {total} are cell counts."],
  "connect.log_map":
    ["map: {cells} cells read ahead in {ms} ms",
     "The connect log: the cell picker's map, read at the connect so the picker opens at once."],
  "connect.log_map_failed":
    ["map: not read ahead — {err}",
     "The connect log, when the cell picker's map could not be read at the connect; {err} is the reason. The picker reads it again when opened."],
  "busy.indexing":
    ["indexing cells and assets…",
     "The busy overlay's second line: reading the plugins and listing the assets."],
  "connect.log_plugins":
    ["{plugins} plugins, {size}, {records} records",
     "The report dialogue, the connect log: what was read."],
  "connect.log_times":
    ["read {read} ms · parse {parse} ms · merge {merge} ms",
     "The connect log: how long each step took."],
  "connect.log_overlay_times":
    ["overlay: layout {layout} ms · index {index} ms ({files} files in {roots} folders) · archives {archives} ms · plugin list {plugins} ms · headers {headers} ms",
     "The connect log: how long the engine took to work the install out, step by step, before any plugin was read."],
  "connect.log_page":
    ["this window: load order {probe} ms · reading the world {open} ms · listing {listing} ms · first scene {scene} ms",
     "The connect log: how long the window waited at each step of the connect, on its own clock; the first scene is counted from the connect's start to the first cell scene standing."],
  "connect.log_cells":
    ["{cells} cells, {lands} landscapes",
     "The connect log: cells and landscapes found."],
  "connect.log_meshes":
    ["{loaded} of {wanted} meshes loaded, {collision} with a collision shape",
     "The connect log: meshes read for avoidance."],
  "connect.log_meshes_unresolved":
    ["{n} not in the overlay",
     "The connect log: appended when meshes were not found."],
  "connect.log_meshes_unreadable":
    ["{n} unreadable",
     "The connect log: appended when meshes could not be read."],
  "connect.log_meshes_late":
    ["{n} read after the merge",
     "The connect log: appended when the mesh read that runs beside the merge missed some, and the step after it read those itself."],
  "connect.log_indexed":
    ["{meshes} mesh files and {textures} textures indexed",
     "The connect log: the asset index."],
  "connect.log_archives.one":
    ["{n} archive read — {names} — {packed} packed files behind the loose ones",
     "The connect log: the .bsa archives read (one)."],
  "connect.log_archives.other":
    ["{n} archives read — {names} — {packed} packed files behind the loose ones",
     "The connect log: the .bsa archives read."],
  "connect.log_no_archives":
    ["no .bsa archives read — the load order names none and the install has none",
     "The connect log when no archive was read."],
  "connect.log_order":
    ["load order from {file}",
     "The connect log: which file the load order came from."],
  "connect.log_mods":
    ["{n} active mods in the overlay",
     "The connect log (MO2): active mods."],
  "connect.log_mods_off":
    ["{n} switched off and not read",
     "The connect log (MO2): appended for disabled mods."],
  "connect.log_overlay":
    ["overlay, later overrides earlier: {roots}",
     "The connect log: the mod folders whose files win, listed the way MO2 lists them — the last one named wins."],
  "connect.log_overlay_more":
    ["{n} more before these",
     "The connect log: the count of lower-priority folders, shown at the head of the overlay line."],
  "connect.log_refs":
    ["{kept} references kept, {overridden} overridden, {deleted} deleted, {moved} moved",
     "The connect log: how the plugins' references merged."],
  "connect.log_mcp":
    ["Morrowind Code Patch: {n} features applied",
     "The connect log: the Code Patch was found."],
  "connect.log_mcp_banners":
    ["\"Improved animation support\" is on, under which the game's scripted banners hang still; here they wave outdoors all the same",
     "The connect log: appended when that Code Patch feature is on."],
  "connect.log_missing":
    ["plugin not found in the overlay: {list}",
     "The connect log: plugins in the load order that were not found."],
  "connect.toast_mods":
    ["{n} active mods",
     "The toast after connecting (MO2): the mod count, before the plugin count."],
  "connect.toast":
    ["{plugins} plugins, {cells} cells — {ms} ms",
     "The toast after connecting a setup."],
  /* Round 18bi: the plugin chooser. Robin asked for it so a mod creator can see their own
     work without the rest of a 562-plugin order underneath it. */
  "plugins.head":
    ["Load these plugins",
     "The plugin chooser's heading."],
  "plugins.hint":
    ["Every plugin your install has is listed, switched on or not. Untick what you do not want loaded — a plugin something else needs as a master stays, shown with a dash, until the thing that needs it is unticked too. Nothing here is saved; you are asked each time.",
     "The plugin chooser's note, under the heading."],
  "plugins.from_install":
    ["Ticked to match your install.",
     "The end of that note on a fresh connect, when the ticks came from the load order."],
  "plugins.from_loaded":
    ["Ticked to match what is loaded right now — Back to install returns to your load order.",
     "The end of that note when the chooser was reopened on an install already loaded."],
  "plugins.not_on":
    ["not switched on",
     "The plugin chooser, beside a plugin the install has but its load order does not name."],
  "plugins.filter_ph":
    ["Filter by name…",
     "The plugin chooser's filter box."],
  "plugins.all":
    ["Tick all",
     "The plugin chooser: ticks everything the filter is showing."],
  "plugins.none":
    ["Untick all",
     "The plugin chooser: unticks everything the filter is showing."],
  "plugins.reset":
    ["Back to install",
     "The plugin chooser: ticks exactly what the install has switched on, ignoring the filter."],
  "plugins.reset_title":
    ["Tick exactly what your install has switched on — the state this dialogue opened in. Unlike Tick all, this ignores the filter.",
     "That button's tooltip."],
  "plugins.probe_failed":
    ["Could not read the load order to ask which plugins to load, so all of them are being loaded: {err}",
     "A warning when the plugin chooser's probe failed; the install is opened whole instead."],
  "plugins.go":
    ["Load these",
     "The plugin chooser's confirming button."],
  "plugins.needed_by":
    ["needed as a master",
     "The plugin chooser, beside a row that is loaded only because something else rests on it."],
  "plugins.not_found":
    ["not found",
     "The plugin chooser, beside a plugin the load order names and the install does not have."],
  "plugins.unreadable":
    ["will not read",
     "The plugin chooser, beside a plugin that is there but whose header could not be read."],
  "plugins.n_of":
    ["{n} of {total} will load",
     "The plugin chooser's count; {n} is how many will load, {total} how many it could."],
  "plugins.n_as_masters.one":
    ["{n} of them only because something else needs it",
     "Under the plugin chooser's list, for exactly one."],
  "plugins.n_as_masters.other":
    ["{n} of them only because something else needs them",
     "Under the plugin chooser's list; {n} is how many are loaded only as masters."],
  "plugins.none_as_masters":
    ["Every one of them ticked in its own right.",
     "Under the plugin chooser's list when nothing is loaded only as a master."],
  "plugins.n_shown":
    ["{n} shown",
     "Under the plugin chooser's list when a filter is hiding some; {n} is how many are visible."],
  "top.plugins":
    ["Plugins",
     "The top panel button that reopens the plugin chooser."],
  "top.plugins_title":
    ["Choose which of the install's plugins to load, and read the load order again",
     "That button's tooltip."],
  "connect.mo2_profile_head":
    ["Which MO2 profile?",
     "The MO2 profile dialogue's heading."],
  "connect.mo2_profile_hint":
    ["Profiles can enable different mods and a different plugin order. The one MO2 has selected is preticked.",
     "The MO2 profile dialogue's note."],
  "connect.mo2_profile_go":
    ["Use this profile",
     "The MO2 profile dialogue's confirming button."],
  "connect.mo2_selected":
    ["(selected in MO2)",
     "The MO2 profile dialogue: the tag on the profile MO2 has selected."],
  "connect.pick_folder":
    ["Choose your Morrowind Data Files folder",
     "The folder picker's title when connecting a Data Files folder."],
  "connect.pick_mo2":
    ["Choose your Mod Organizer 2 folder — the one with mods\\ and profiles\\",
     "The folder picker's title when connecting an MO2 install."],
  "connect.pick_openmw":
    ["Choose your openmw.cfg",
     "The file picker's title when connecting OpenMW, and the OpenMW button's tooltip when no cfg was found."],
  "file.kind_openmw_cfg":
    ["OpenMW config",
     "The open dialogue's file-type name for openmw.cfg."],
  "connect.openmw_title":
    ["Connects to {path} — hold Shift to choose a different one",
     "Top bar: the OpenMW button's tooltip when its cfg was found."],
  /* Round 18ap: keyed for the language packs — the simplified preview and mesh loading (10_preview.js). */
  "report.mesh_missing":
    ["mesh missing — {path}",
     "The report dialogue: a mesh the engine could not read."],
  "report.not_a_mesh":
    ["the engine sent something that is not a mesh",
     "A grass card's swatch tooltip, and the report: the mesh data was not a mesh."],
  "report.no_geometry":
    ["{path}: no visible geometry (filler or placeholder mesh)",
     "The report dialogue: a mesh with nothing to draw."],
  "report.missing_from_bundle":
    ["missing from the bundle",
     "A grass card's swatch tooltip: the engine's mesh bundle had no entry for it."],
  "connect.preview":
    ["Connect an install — the preview needs the engine.",
     "The status line under the preview before an install is connected."],
  "preview.strip_no_ban":
    ["This rule bans no texture, so there is no strip to draw. Add one under Bans.",
     "Toast (once) when the road strip is on but the rule has no bans."],
  "preview.strip_other_plugin":
    ["The banned texture comes from a different plugin than this one, so the two cannot share a cell. The strip is left out.",
     "Toast (once) when the banned texture cannot be drawn on the patch."],
  /* Round 18ap: keyed for the language packs — the asset browser (12_browser.js). */
  "browse.fav_failed":
    ["Could not save that favourite: {err}",
     "Toast when starring an entry could not be saved."],
  "browse.star_on_title":
    ["Starred — kept at the top of every picker of this kind",
     "The star on a picker row: its tooltip while starred."],
  "browse.star_off_title":
    ["Star this, to keep it at the top of every picker of this kind",
     "The star on a picker row: its tooltip while not starred."],
  "connect.data_files_first":
    ["Connect your Data Files folder first.",
     "Toast when the asset browser is opened before a setup is connected."],
  "browse.filter":
    ["filter… (e.g. azbc, filler, grass\\az)",
     "The asset browser's filter placeholder."],
  "browse.grass":
    ["Grass folder",
     "The asset browser: the switch that shows only Meshes\\grass."],
  "browse.grass_title":
    ["Show only what is under Meshes\\grass",
     "The Grass folder switch's tooltip."],
  "browse.edited":
    ["Edited",
     "The asset browser: the switch that shows only meshes with a setup."],
  "browse.edited_title":
    ["Show only meshes that carry a setup — texture rules, their own paint, or a lock",
     "The Edited switch's tooltip."],
  "browse.list_title":
    ["One entry a line, with its whole path",
     "The asset browser: the list view button's tooltip."],
  "browse.grid_title":
    ["Thumbnails, so you can pick by what it looks like",
     "The asset browser: the grid view button's tooltip."],
  "browse.use":
    ["Use selection",
     "The asset browser's confirming button."],
  "browse.head_mesh":
    ["Choose mesh (.nif)",
     "The asset browser's heading for meshes."],
  "browse.head_texture":
    ["Choose texture",
     "The asset browser's heading for textures."],
  "browse.n_found":
    ["{n} found",
     "The asset browser's pill before a filter is typed."],
  "browse.more":
    ["{n} of {total} — keep scrolling for the rest.",
     "The asset browser: the note at the bottom of a long list."],
  "browse.n_selected":
    ["{n} selected",
     "The asset browser's footer: how many entries are ticked."],
  "browse.none_edited":
    ["No set-up mesh matches — nothing edited yet?",
     "The asset browser, Edited on, when nothing matches."],
  "browse.none_grass":
    ["Nothing under Meshes\\grass matches that filter.",
     "The asset browser, Grass folder on, when nothing matches."],
  "browse.indexing":
    ["Still indexing your Data Files…",
     "The asset browser while the index is being built."],
  "browse.none_found":
    ["No assets found — is Meshes/ present?",
     "The asset browser when the install has no assets."],
  /* Round 18ap: keyed for the language packs — the undo toasts (22_history.js). */
  "undo.on":
    ["On {name}",
     "The undo toast: where the step happened — a mesh or a rule."],
  "undo.open_mesh":
    ["Open {name} in the mesh editor",
     "The undo toast's cue line: opens the mesh the step touched."],
  "undo.in":
    ["In {name}",
     "The undo toast: which cell the step happened in."],
  "undo.show_where":
    ["Show where it happened in {name}",
     "The undo toast's cue line: aims the camera at the stroke."],
  "undo.load":
    ["Load {name}",
     "The undo toast's cue line: loads the cell the step happened in."],
  "undo.on_card":
    ["On grass card {n} of {rule}",
     "The undo toast: the step touched a grass card."],
  "undo.show_card":
    ["Show that card in {rule}",
     "The undo toast's cue line: reveals the grass card."],
  "undo.show_rule":
    ["Show {rule} in the rules list",
     "The undo toast's cue line: reveals the rule."],
  "undo.that_change":
    ["that change",
     "The undo toast: what to call a step nothing named."],
  "undo.n_back.one":
    ["{n} step further back",
     "The undo toast: how many steps remain behind."],
  "undo.n_back.other":
    ["{n} steps further back",
     "The undo toast: how many steps remain behind."],
  "undo.n_forward.one":
    ["{n} step further forward",
     "The redo toast: how many steps remain ahead."],
  "undo.n_forward.other":
    ["{n} steps further forward",
     "The redo toast: how many steps remain ahead."],
  "undo.undid":
    ["Undid “{label}”.",
     "The undo toast's first sentence; {label} names the step."],
  "undo.redid":
    ["Redid “{label}”.",
     "The redo toast's first sentence; {label} names the step."],
  "undo.nothing_back":
    ["Nothing left to undo — this is as far back as the history goes.",
     "Toast when Undo has nothing to step back to."],
  "undo.nothing_forward":
    ["Nothing to redo — you are at the newest step.",
     "Toast when Redo has nothing to step forward to."],
  "undo.paint_too_far":
    ["That step is too far back to restore the painted ground: {err}",
     "Toast when an undo step's paint is no longer held by the engine."],
  /* Round 18ap: keyed for the language packs — units and the toast cue (03_core.js). */
  "toast.show_it":
    ["Show it",
     "A toast's cue line when the caller gives no wording."],
  "unit.mb":
    ["{n} MB",
     "A size in megabytes."],
  "unit.s":
    ["{n} s",
     "A time in seconds."],
  "unit.min_s":
    ["{m} min {s} s",
     "A time in minutes and seconds."],
  /* Round 18ap: keyed for the language packs — leftovers found by the markup walk. */
  "left.rule_additive_title":
    ["Additive: grows beside what the texture would otherwise grow here, rather than replacing it",
     "Left column, a rule row: the tooltip on the \" +\" after an additive rule's scope."],
  "right.holding":
    ["holding {rule}",
     "Right panel, the copy/paste settings row: which rule's settings are on the clipboard."],
  "canopy.pattern_ph":
    ["flora_tree_ai*",
     "Left column, Canopies: a pattern field's placeholder (an example pattern)."],
  "preview.grassfar_cells.one":
    ["{n} cell",
     "Left column, Preview: the grass-distance slider's value."],
  "preview.grassfar_cells.other":
    ["{n} cells",
     "Left column, Preview: the grass-distance slider's value."],
  "preview.grassfar_off":
    ["Off",
     "Left column, Preview: the grass-distance slider's value at zero."],
  "copy.copied":
    ["Copied",
     "Toast after the statics export text is copied."],
  "profiles.created":
    ["Created {name}",
     "Toast after creating a profile."],
  /* Round 18aq: the language picker in Settings (19_settings.js). */
  /* Round 18cg: the colour themes, by name. Names rather than descriptions — the row a
     name sits in is painted in that theme's own colours, so what it looks like is shown
     rather than said, and a translator is left with five words instead of five sentences
     about oxide and vellum. They are palettes named after the houses they came from
     (`theming-plan.md`), so a pack will most likely keep them as they are; the keys exist
     because a pack that transliterates its own alphabet should be able to. */
  "settings.theme_wraithguard":
    ["Wraithguard (Default)",
     "Settings, Theme: Wraithguard's own palette, as the toolkit is wearing it."],
  "settings.theme_default":
    ["Gardenfell Green",
     "Settings, Theme: the palette the program ships with — sage on true grey."],
  "settings.theme_velothi":
    ["Of Ash and Blight",
     "Settings, Theme: a palette — ember on ashfall."],
  "settings.theme_redoran":
    ["Redoran Bone",
     "Settings, Theme: a palette — oxide on vellum, the light one."],
  "settings.theme_telvanni":
    ["Telvanni Bio",
     "Settings, Theme: a palette — spore green on aubergine."],
  "settings.theme_hlaalu":
    ["Hlaalu Clay",
     "Settings, Theme: a palette — coin gold on yellow clay."],
  "settings.theme_firmament":
    ["The Firmament",
     "Settings, Theme: a palette — dark greys and starlight, Masser's rust for the warm colours; the quiet one."],
  "settings.theme_unknown":
    ["A theme this version has not got",
     "Settings, Theme: the entry for a theme the profile names and this build has no colours for; its code is shown beside it."],
  "settings.lang_english":
    ["English (built in)",
     "Settings, Language: the picker's first entry — the wording built into the program."],
  "settings.lang_pack_option":
    ["{name} — {translated} of {total} translated",
     "Settings, Language: a pack in the picker, by its own name, with its coverage."],
  "settings.lang_pack_broken":
    ["{name} — could not be read",
     "Settings, Language: a pack file the engine could not read, greyed in the picker."],
  "settings.lang_pack_missing_option":
    ["{code} — not found",
     "Settings, Language: the picker entry for the language the profile asks for when no pack provides it."],
  "settings.lang_note":
    ["Packs are gardenfell.<code>.toml files in {dir} — the languages folder under where your files are kept, so they move with the rest. Drop one there and choose it here; the choice is saved in your profile. Text the program draws as you work catches up as it is redrawn.",
     "Settings, Language: the note under the picker; {dir} is the languages folder."],
  "settings.lang_missing_note":
    ["This profile asks for {file}, which is not in that folder — English until it is.",
     "Settings, Language: the note when the profile's pack is not on this machine."],
  "settings.lang_broken_note":
    ["{file} could not be read: {err}",
     "Settings, Language: the note when the chosen pack file will not parse."],
  "settings.lang_pack_note":
    ["{name} by {author}, made for the cell viewer {madeFor} — {translated} of {total} strings translated; the rest show English.",
     "Settings, Language: the note describing the chosen pack."],
  "settings.lang_orphans_note":
    ["{n} of its keys are not in this version of the cell viewer any more — kept in the file, not used. Refresh this pack to sort them to the end.",
     "Settings, Language: appended when the pack has keys the catalogue lacks."],
  "settings.lang_problems_note.one":
    ["{n} line could not be used:",
     "Settings, Language: the heading over the pack's refused lines (one)."],
  "settings.lang_problems_note.other":
    ["{n} lines could not be used:",
     "Settings, Language: the heading over the pack's refused lines."],
  "settings.lang_problem_line":
    ["line {line}, {key}: {what}",
     "Settings, Language: one refused line of the pack — its line number, key and reason."],
  "settings.lang_problems_toast":
    ["{name}: {n} line(s) could not be used — Settings says which.",
     "Toast when a pack with refused lines is applied."],
  "settings.lang_missing_toast":
    ["This profile asks for the language pack {file}, which is not in {dir}. English until it is.",
     "Toast at start-up (and on switching profile) when the profile names a pack the machine lacks."],
  "settings.lang_switched":
    ["Now speaking {name}. Text the program draws as you work catches up as it is redrawn.",
     "Toast after a language pack is chosen."],
  "settings.lang_switched_english":
    ["Back to English.",
     "Toast after English is chosen."],
  /* Round 18as: Create new language file, and the two Open folder buttons. */
  "settings.lang_new_head":
    ["New language file",
     "Settings, Language: the title of the two questions Create new language file asks."],
  "settings.lang_new_code_body":
    ["The language's code, as in BCP 47 — sv, de, pt-BR, zh-Hans. It names the file: gardenfell.<code>.toml.",
     "Settings, Language: the first question — the code."],
  "settings.lang_new_next":
    ["Next",
     "Settings, Language: the button that takes the code and asks for the name."],
  "settings.lang_new_taken":
    ["A pack with that code is already in the languages folder — choose it in the picker, or pick another code.",
     "Settings, Language: the warning under the code field when a pack for that code exists."],
  "settings.lang_new_name_body":
    ["What the language calls itself — Svenska, Deutsch, Português do Brasil. This is what the picker shows for {file}.",
     "Settings, Language: the second question — the name; {file} is the pack file about to be made."],
  "settings.lang_created":
    ["Created {file} — every string listed with its English above. Fill it in, then choose it here; Refresh this pack brings it up to date after a new version of Gardenfell.",
     "Toast after Create new language file."],
  "settings.lang_create_failed":
    ["Could not create the language file: {err}",
     "Toast when Create new language file failed; {err} is the engine's reason."],
  "settings.open_failed":
    ["Could not open the folder: {err}",
     "Toast when Open folder or Open languages folder could not start the file browser; {err} is the engine's reason."],
  "settings.lang_updated":
    ["Refreshed {file} — {added} new strings added empty, {orphans} no longer used; the old file is kept as {backup}.",
     "Toast after Refresh this pack."],
  "settings.lang_update_failed":
    ["Could not refresh the pack: {err}",
     "Toast when Refresh this pack failed."],
  /* Round 18ar: engine messages worded from the catalogue — the rules validation (validate.rs). */
  "eng.val_no_rules":
    ["No ground texture rules defined.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs): there are no rules."],
  "eng.val_paint_unnamed":
    ["A paint-only rule has no name. Give it one — a paint colour points at a rule by name.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs): a paint-only rule without a name."],
  "eng.val_texture_unnamed":
    ["A rule has an empty texture name.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs): a texture rule without a texture."],
  "eng.val_texture_whitespace":
    ["[{rule}] texture name has leading or trailing whitespace.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning)."],
  "eng.val_scope_empty":
    ["[{texture}:] has an empty cell or region name.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs): a scoped rule with no scope."],
  "eng.val_spacing_zero":
    ["[{rule}] spacing must be greater than 0.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs)."],
  "eng.val_spacing_large":
    ["[{rule}] spacing {spacing} is 512 or more; the tool is only known to behave below that.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning)."],
  "eng.val_scope_whitespace":
    ["[{rule}] the scope has a space at one end. It still works, but the file reads oddly — trim it.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs): a scope with stray whitespace."],
  "eng.val_not_finite":
    ["[{rule}] {field} is not a number ({value}) — nothing will grow from this rule until it is one.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs): a field holding infinity or NaN."],
  "eng.val_scale_inverted":
    ["[{rule}] scale minimum is above its maximum.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs)."],
  "eng.val_height_inverted":
    ["[{rule}] height floor is above its ceiling.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs)."],
  "eng.val_no_usable_slot":
    ["[{rule}] has no usable slot: one needs a mesh or an id, and a chance above 0.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs)."],
  "eng.val_slot_mesh_and_id":
    ["[{rule}] slot {slot} sets both a mesh and an id; they are mutually exclusive.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs)."],
  "eng.val_slot_no_mesh":
    ["[{rule}] slot {slot} has a chance but no mesh or id.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning)."],
  "eng.val_slot_negative":
    ["[{rule}] slot {slot} has a negative chance.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs)."],
  "eng.val_slot_slashes":
    ["[{rule}] slot {slot} uses forward slashes; Morrowind paths use backslashes.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning)."],
  "eng.val_all_slots_empty":
    ["[{rule}] every slot is empty, so this rule places nothing at all.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning)."],
  "eng.val_ban_twice":
    ["[{rule}] bans {texture} twice.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning)."],
  "eng.val_layered":
    ["[{rule}] has {n} rules layered on it. Each places its own grass, so this ground carries all {n} passes and will be denser than any one of them.",
     "The export dialogue, a problem the engine found in the rules (crates/gdn_core/src/validate.rs) (a warning): two rules on one texture and scope."],
  /* Round 18ar: engine messages worded from the catalogue — profiles, rules sets, connecting, the preview patch, language packs. */
  "eng.not_profile_file":
    ["{file} is not a profile filename",
     "An engine error (profiles.rs): a file name that is not a .profile.toml."],
  "eng.profile_unreadable_save":
    ["refusing to save an unreadable profile: {err}",
     "An engine error (profiles.rs): the profile text would not parse, so it is not written."],
  "eng.not_a_profile":
    ["that does not read as a profile: {err}",
     "An engine error (profiles.rs): an added file is not a profile."],
  "eng.profile_needs_name":
    ["a profile needs a name",
     "An engine error (profiles.rs): renaming a profile to nothing."],
  "eng.only_profile":
    ["this is your only profile — make another one first",
     "An engine error (profiles.rs): deleting the last profile."],
  "eng.not_rules_file":
    ["{file} is not a grass rules filename",
     "An engine error (ruleset.rs): a file name that is not a .rules.toml."],
  "eng.rules_need_name":
    ["a set of grass rules needs a name",
     "An engine error (ruleset.rs): renaming a rules set to nothing."],
  "eng.only_rules":
    ["this is your only set of grass rules — make another one first",
     "An engine error (ruleset.rs): deleting the last rules set."],
  "eng.not_mo2_folder":
    ["{path} does not look like an MO2 folder — expected mods\\ and profiles\\ inside it",
     "An engine error (layout.rs, main.rs): connecting MO2 from the wrong folder."],
  "eng.mo2_dir_missing":
    ["ModOrganizer.ini points {key} at {path}, which is not there",
     "An engine error (layout.rs, main.rs): ModOrganizer.ini names a mods or profiles folder that does not exist."],
  "eng.mo2_profiles_unreadable":
    ["cannot read profiles: {err}",
     "An engine error (layout.rs, main.rs): the MO2 profiles folder could not be listed."],
  "eng.mo2_no_profiles":
    ["no profiles found in MO2",
     "An engine error (layout.rs, main.rs): the MO2 folder has no profiles."],
  "eng.mo2_profile_missing":
    ["profile \"{wanted}\" is missing; using \"{used}\"",
     "A connect warning (layout.rs): the remembered MO2 profile is gone."],
  "eng.mo2_no_game_path":
    ["could not find the game's Data Files from ModOrganizer.ini gamePath — vanilla assets and the DLC will be missing",
     "A connect warning (layout.rs)."],
  "eng.mo2_modlist_unreadable":
    ["cannot read {path}\\modlist.txt: {err}",
     "An engine error (layout.rs): the MO2 profile's modlist could not be read."],
  "eng.mo2_mods_missing":
    ["{n} enabled mod(s) listed in modlist.txt are not in mods\\",
     "A connect warning (layout.rs)."],
  "eng.mo2_no_order":
    ["no plugin load order found — looked for [Game Files] in {path}\\Morrowind.ini and in the game folder",
     "An engine error (layout.rs): MO2 connect found no load order."],
  "eng.openmw_no_data":
    ["no usable data= directories in {path}",
     "An engine error (layout.rs): openmw.cfg names no data folder that exists."],
  "eng.openmw_data_missing":
    ["{n} data= director(ies) in openmw.cfg do not exist",
     "A connect warning (layout.rs)."],
  "eng.not_a_folder":
    ["{path} is not a folder",
     "An engine error (layout.rs): the chosen Data Files path is not a folder."],
  "eng.no_meshes_folder":
    ["no Meshes\\ folder here — is this really Data Files?",
     "A connect warning (layout.rs)."],
  "eng.texture_undefined":
    ["no plugin in the load order defines the texture {texture}",
     "An engine error (patch.rs): the simplified preview asked for a texture no plugin defines. {texture} is quoted."],
  "eng.pack_no_language":
    ["no [language] table — a pack starts with [language] and its code",
     "An engine error (lang.rs): a language pack without its header."],
  "eng.pack_no_code":
    ["[language] has no code — \"sv\", \"pt-BR\", \"zh-Hans\"",
     "An engine error (lang.rs): a language pack whose header has no code."],
  "eng.pack_bad_code":
    ["[language] code {code} is not a language tag — letters, digits and hyphens only",
     "An engine error (lang.rs): a language pack with a malformed code. {code} is quoted."],
  "eng.pack_code_en":
    ["en is English itself, the wording built in — a pack needs a code of its own, such as en-GB",
     "An engine error (lang.rs): Create new language file was given the code en."],
  "eng.pack_not_found":
    ["no language pack {file} in {dirs}",
     "An engine error (lang.rs): the profile's language pack is not in the language folders."],
  "eng.pack_not_string":
    ["not a string",
     "A language pack line refused (lang.rs): the value is not a string."],
  "eng.pack_placeholders":
    ["placeholders differ — the English has {english}, this has {pack}",
     "A language pack line refused (lang.rs): its placeholders differ from the English's."],
  "eng.pack_stray_lt":
    ["a stray < — write &lt; for a less-than sign",
     "A language pack line refused (lang.rs): a < that is not a tag."],
  "eng.pack_tag":
    ["uses <{tag}>, which the English does not — a pack may use only the tags its English uses",
     "A language pack line refused (lang.rs): a tag the English does not use."],
  /* Round 18ar: engine messages worded from the catalogue — the desktop shell commands (src-tauri/src/main.rs). */
  "eng.no_install":
    ["no install connected",
     "An engine error (src-tauri/src/main.rs): a command that needs a setup was called before one was connected."],
  "eng.no_install_is":
    ["no install is connected",
     "An engine error (src-tauri/src/main.rs): the same, from the export path."],
  "eng.no_interior":
    ["no interior called {name}",
     "An engine error (src-tauri/src/main.rs): an interior the load order does not have."],
  "eng.world_busy":
    ["the world is busy; try again",
     "An engine error (src-tauri/src/main.rs): the loaded world could not be taken for a swap in time."],
  "eng.ini_no_sections":
    ["no [sections] found in that file",
     "An engine error (src-tauri/src/main.rs): an .ini import found no rules."],
  "eng.pack_exists":
    ["{file} is already there — choose it in Settings, or give the new one another code",
     "An engine error (src-tauri/src/main.rs): Create new language file found the pack already on disk."],
  "eng.pack_missing_file":
    ["no language pack {file}",
     "An engine error (src-tauri/src/main.rs): Refresh this pack found no such file."],
  "eng.open_failed":
    ["could not open {dir} in the file browser: {err}",
     "An engine error (profiles.rs): the system's file browser would not start for Open folder."],
  "eng.no_landscape":
    ["no landscape for that cell",
     "An engine error (src-tauri/src/main.rs): a cell with no LAND record was asked for."],
  "eng.patch_cannot_split":
    ["patching produces one merged plugin, so it cannot be split",
     "An engine error (src-tauri/src/main.rs): the Advanced export asked to patch and split at once."],
  "eng.patch_too_many":
    ["too many references to merge into one plugin: export without merging and the files split themselves",
     "An engine error (src-tauri/src/main.rs): a merge was asked for a run that needs more than one .esp to number its references."],
  "eng.adv_nothing_chosen":
    ["nothing chosen: tick at least one cell or room",
     "An engine error (src-tauri/src/main.rs): the Advanced export had nothing ticked."],
  "eng.nothing_generated":
    ["nothing generated yet",
     "An engine error (src-tauri/src/main.rs): Save was pressed before a plugin was generated."],
  "eng.not_a_reference":
    ["not a reference: {key}",
     "An engine error (src-tauri/src/main.rs): a paint command named something that is not a placed object."],
  "eng.rule_not_loaded":
    ["that rule is not in the loaded set",
     "An engine error (src-tauri/src/main.rs): a colour points at a rule the loaded rules do not have."],
  "eng.no_room_for_colour":
    ["there is no room for another colour",
     "An engine error (src-tauri/src/main.rs): every paint slot is taken."],
  "eng.layer_not_there":
    ["that layer is not there, or it is the only one left",
     "An engine error (src-tauri/src/main.rs): removing a layer that does not exist or is the last."],
  "eng.no_layer":
    ["there is no layer {layer}",
     "An engine error (src-tauri/src/main.rs)."],
  "eng.colour_or_layer_not_there":
    ["that colour or that layer is not there",
     "An engine error (src-tauri/src/main.rs)."],
  "eng.no_swatch":
    ["there is no swatch {slot}",
     "An engine error (src-tauri/src/main.rs)."],
  "eng.paint_version_gone":
    ["that painted ground is no longer kept (version {rev})",
     "An engine error (src-tauri/src/main.rs): an undo step reached back past the paint the engine still holds."],
  "eng.stroke_no_viewpoint":
    ["the stroke carried no viewpoint",
     "An engine error (src-tauri/src/main.rs): a brush stroke arrived without its camera."],
  "eng.mesh_locked":
    ["{key} is locked — unlock it to paint",
     "An engine error (src-tauri/src/main.rs): painting on a locked mesh setup."],
  "eng.mesh_unreadable_paint":
    ["that mesh could not be read, so the brush cannot land on it",
     "An engine error (src-tauri/src/main.rs)."],
  "eng.mesh_unreadable_grow":
    ["that mesh could not be read, so nothing can grow on it",
     "An engine error (src-tauri/src/main.rs)."],
  "eng.stexp_nothing_chosen":
    ["nothing chosen — pick at least one mesh to export",
     "An engine error (src-tauri/src/main.rs): the statics export had nothing ticked."],
  "eng.install_changing":
    ["the install keeps changing while its meshes are being read — try again",
     "An engine error (src-tauri/src/main.rs)."],
  "eng.mesh_unreadable_match":
    ["that mesh could not be read, so its geometry cannot be matched",
     "An engine error (src-tauri/src/main.rs): the retexture panel's search."],
  "eng.not_in_load_order":
    ["not found in the load order: {path}",
     "An engine error (src-tauri/src/main.rs): a mesh or texture path no root has."],
  "eng.could_not_read":
    ["could not read {path}",
     "An engine error (src-tauri/src/main.rs): a file in the load order that would not read."],
  "eng.order_no_plugins":
    ["{file} lists no plugins — expected [Game Files] GameFile0=… or content=… lines",
     "An engine error (src-tauri/src/main.rs): connecting a setup whose load-order file names no plugins."],
  "eng.openmw_no_content.one":
    ["{file} names {n} data directory but lists no game files: not one content= line naming an .esm or .esp, so nothing says which plugins to load or in what order. A data directory is only somewhere to look for files — even Morrowind.esm is not loaded until a content= line asks for it. Tick your plugins in the OpenMW Launcher's Data Files tab and it writes them into this same file; if a mod manager exported this config, export it again with the plugins enabled.",
     "An engine error (src-tauri/src/main.rs): an openmw.cfg with data folders and no content= lines (one folder)."],
  "eng.openmw_no_content.other":
    ["{file} names {n} data directories but lists no game files: not one content= line naming an .esm or .esp, so nothing says which plugins to load or in what order. A data directory is only somewhere to look for files — even Morrowind.esm is not loaded until a content= line asks for it. Tick your plugins in the OpenMW Launcher's Data Files tab and it writes them into this same file; if a mod manager exported this config, export it again with the plugins enabled.",
     "An engine error (src-tauri/src/main.rs): an openmw.cfg with data folders and no content= lines."],
};

/* -------------------------------------------------------------------------------------
   The store.

   `T(key)` is what the rest of the page calls, and the answer is whatever the app ships
   saying: the markup for anything with an element of its own, `TX_STRINGS` for the rest.

   **There was a file here.** `gardenfell.text.toml` beside the exe held every string, so
   the wording could be reworded or translated without editing a 380 KB page — the app
   handed the engine a catalogue at start-up and took back what the file said. Robin,
   round 18aa: "Lets remove the gardenfell.text.toml for now, and just have the text in
   the code instead as it were before we moved it out." So the file, the catalogue, the
   `text_sync` command that carried it and the `TX_WAS` list that let a reworded default
   reach somebody who already had a file are all gone; the keys and `T()` stay, which is
   what any of it would be rebuilt on. A stale `gardenfell.text.toml` in a store is inert
   now — nothing reads it.
   ------------------------------------------------------------------------------------- */
/* Round 18ay: a tooltip's words live in the element's `title` - except while the pointer
   rests on it, when the scroll-note layer (`Tip`, 30_tip.js) has parked them in
   `data-tip` so the browser's own box stays away. These two read and write wherever the
   words are just now, so a language switch rewords a tooltip that happens to be up. */
const titleOf=el=>(typeof Tip!=='undefined')? Tip.read(el) : (el.getAttribute('title')||'');
const setTitle=(el,text)=>{ if(typeof Tip!=='undefined') Tip.write(el,text); else el.setAttribute('title',text); };

const Text={
  /** key -> the wording in use. Everything the app ships saying, taken once. */
  said:{},

  /** The built-in wording for one key, wherever it lives.
   *
   *  Read from the snapshot rather than from the page, because half the point of a key is
   *  that a script rewrites the element it names: `markInstall` shortens a connect button
   *  to "MO2" the moment an install is live, and answering "MO2" to
   *  `T('top.connect_mo2')` would leave the button stuck that way when the install
   *  changed. The snapshot is the *shipped* wording, which is what a default is. */
  builtin(key){
    if(Text.said[key]!=null) return Text.said[key];
    if(TX_STRINGS[key]) return TX_STRINGS[key][0];
    /* A key whose element was not there when the snapshot was taken — a row the panel
       builds as it draws it. Read the page for it, but do not keep the answer: an element
       that arrives late is as liable to be rewritten as one that was here all along. */
    const el=document.querySelector('[data-tx="'+key+'"]');
    if(el) return el.innerHTML.replace(/\s+/g,' ').trim();
    const t=document.querySelector('[data-txt="'+key+'"]');
    if(t) return titleOf(t);
    const ph=document.querySelector('[data-txp="'+key+'"]');
    if(ph) return ph.getAttribute('placeholder')||'';
    return '';
  },

  /** Whether a key exists at all — in the pack, the snapshot, the catalogue, or the page.
   *  A pack may carry a plural form the English has not got (`.few` for Czech), which is
   *  why the pack is asked first. */
  has(key){
    return Text.pack[key]!=null || Text.said[key]!=null || !!TX_STRINGS[key] ||
           !!document.querySelector('[data-tx="'+key+'"],[data-txt="'+key+'"],[data-txp="'+key+'"]');
  },

  /* ---- language packs (round 18aq, wording-plan.md phase 2) --------------------------
     `pack` is the translation in use, key -> text, and nothing else: English stays in
     `said`, so an untranslated key shows English and switching back to English is
     emptying the pack. `meta` is the pack's `[language]` header for the picker. */
  pack:{},
  meta:null,

  /** The wording in use for one key: the pack's, or the English. No placeholders filled. */
  current(key){
    const p=Text.pack[key];
    return p!=null? p : Text.builtin(key);
  },

  /* ---- engine messages (round 18ar, wording-plan.md phase 3) -------------------------
     The engine words its messages in English from this same catalogue (`lang::word`,
     `msg!` in Rust; the keys are `eng.*`), so what arrives over the wire is the English
     of a key with its placeholders filled. Under a pack the page matches it back: each
     `eng.*` English is turned into a pattern once, the message is tried against every
     pattern whole and then as the tail of a longer message (the engine wraps some — a
     path and a colon in front), and a match answers `T(key, params)`. Under English
     nothing is touched, so the wire and the screen stay byte for byte what they were. */
  _engPatterns:null,
  engPatterns(){
    if(Text._engPatterns) return Text._engPatterns;
    const out=[];
    for(const key in TX_STRINGS){
      if(!key.startsWith('eng.')) continue;
      const en=TX_STRINGS[key][0];
      // A pattern that is only placeholders and punctuation would match anything.
      if(!/[A-Za-z]{3,}/.test(en.replace(/\{\w+\}/g,''))) continue;
      const names=[];
      const re=en.replace(/[.*+?^${}()|[\]\\]/g,m=>m==='{'||m==='}'? m : '\\'+m)
                 .replace(/\{(\w+)\}/g,(m,n)=>{ names.push(n); return '([\\s\\S]*?)'; });
      out.push({key, names, whole:new RegExp('^'+re+'$'), tail:new RegExp('^([\\s\\S]*?)'+re+'$'),
                literal:en.replace(/\{\w+\}/g,'').length});
    }
    // The most literal pattern first, so "{path}: {err}"-shaped ones never win over a
    // message that names its subject.
    out.sort((a,b)=>b.literal-a.literal);
    return Text._engPatterns=out;
  },

  /** An engine message in the language in use: matched back to its key and translated,
   *  or as it came when nothing matches. English is the identity. */
  word(msg){
    if(Text.lang==='en' || msg==null) return msg;
    const s=String(msg);
    for(const p of Text.engPatterns()){
      let m=p.whole.exec(s), prefix='';
      if(!m){ m=p.tail.exec(s); if(m){ prefix=m[1]; m=m.slice(1); } }
      if(!m) continue;
      const vars={};
      p.names.forEach((n,i)=>{ vars[n]=m[i+1]; });
      return prefix+T(p.key,vars);
    }
    return s;
  },

  /** Puts a pack on the page — or takes it off, with `null`.
   *
   *  The markup first: every keyed element whose wording is still what the *outgoing*
   *  language put there takes the incoming one. An element a script has rewritten since
   *  (the connect button shortened to "MO2", a status line filled in) is left alone — it
   *  catches up on its next render, as does everything rendered from script, and the
   *  picker says so. Then `App.reword()` asks the renderers that are cheap to run again. */
  apply(pack){
    const was={pack:Text.pack, lang:Text.lang};
    const outgoing=key=>{ const p=was.pack[key]; return p!=null? p : Text.builtin(key); };
    Text.pack=(pack && pack.text)||{};
    Text.meta=pack? {code:pack.code, name:pack.name, author:pack.author, madeFor:pack.madeFor,
                     translated:pack.translated, total:pack.total, file:pack.file} : null;
    Text.lang=pack? (pack.code||'en') : 'en';
    const norm=s=>String(s||'').replace(/\s+/g,' ').trim();
    document.querySelectorAll('[data-tx]').forEach(el=>{
      const k=el.dataset.tx;
      if(norm(el.innerHTML)===norm(outgoing(k))) el.innerHTML=Text.current(k);
    });
    document.querySelectorAll('[data-txt]').forEach(el=>{
      const k=el.dataset.txt;
      if(titleOf(el)===outgoing(k)) setTitle(el,Text.current(k));
    });
    document.querySelectorAll('[data-txp]').forEach(el=>{
      const k=el.dataset.txp;
      if((el.getAttribute('placeholder')||'')===outgoing(k)) el.setAttribute('placeholder',Text.current(k));
    });
    if(typeof App==='object' && typeof App.reword==='function') App.reword();
    return was.lang;
  },

  /** The CLDR plural category for a count, in the language in use — the pack's code
   *  (`Text.apply`), English until one is applied: `one` for exactly 1, `other` for
   *  everything else, 0 included. */
  lang:'en',
  plural(n){
    const a=Math.abs(n), i=Math.floor(a), f=a!==i;
    switch(Text.lang.split('-')[0]){
      case 'ru': case 'uk': case 'be':
        if(f) return 'other';
        if(i%10===1 && i%100!==11) return 'one';
        if(i%10>=2 && i%10<=4 && (i%100<12 || i%100>14)) return 'few';
        return 'many';
      case 'pl':
        if(f) return 'other';
        if(i===1) return 'one';
        if(i%10>=2 && i%10<=4 && (i%100<12 || i%100>14)) return 'few';
        return 'many';
      case 'cs': case 'sk':
        if(f) return 'other';
        if(i===1) return 'one';
        if(i>=2 && i<=4) return 'few';
        return 'other';
      case 'fr': case 'pt':
        return (i===0 || i===1) && !f? 'one' : 'other';
      case 'ja': case 'zh': case 'ko': case 'tr': case 'hu': case 'id': case 'vi': case 'th':
        return 'other';
      default:                                   // en, de, sv, nl, es, it, fi, da, nb, el
        return i===1 && !f? 'one' : 'other';
    }
  },

  /** Takes the markup's wording once, before anything has had a chance to rewrite it. */
  start(){
    document.querySelectorAll('[data-tx]').forEach(el=>{
      if(Text.said[el.dataset.tx]==null)
        Text.said[el.dataset.tx]=el.innerHTML.replace(/\s+/g,' ').trim();
    });
    document.querySelectorAll('[data-txt]').forEach(el=>{
      if(Text.said[el.dataset.txt]==null) Text.said[el.dataset.txt]=titleOf(el);
    });
    document.querySelectorAll('[data-txp]').forEach(el=>{
      if(Text.said[el.dataset.txp]==null)
        Text.said[el.dataset.txp]=el.getAttribute('placeholder')||'';
    });
    for(const k in TX_STRINGS)
      if(Text.said[k]==null) Text.said[k]=TX_STRINGS[k][0];
  },
};

/* Taken here, at load, rather than from the start-up sequence: the scripts run after the
   markup, so the page is complete, and `syncNav` writes `T('vp.nav_orbit')` into the very
   button that *holds* that wording. Called later, the snapshot would have read back an
   answer the page had already given — which for one round of round 18aa was an empty
   string, and the navigation button lost its name. */
Text.start();

/** One piece of text, by key. `vars` fills in any `{name}` the wording contains.
 *
 *  Plurals (round 18ap, wording-plan.md decision 2): when `vars.n` is a number and the
 *  catalogue holds `key.one` / `key.few` / `key.many` / `key.other`, the form is chosen
 *  by the language's rule — English has `one` for exactly 1 and `other` for the rest —
 *  and a key given without a suffix serves every count. So a call site says
 *  `T('paint.ready',{n})` and never `' mesh'+(n===1?'':'es')`: the two forms are two
 *  whole strings, which is what a translator needs (Russian and Polish have three). */
function T(key,vars){
  if(vars && typeof vars.n==='number'){
    const form=key+'.'+Text.plural(vars.n);
    if(Text.has(form)) key=form;
    else if(Text.has(key+'.other')) key=key+'.other';
  }
  let s=Text.pack[key];
  if(s==null) s=Text.said[key];
  if(s==null) s=Text.builtin(key);
  /* `{err}` is, by convention, an engine message inside a page sentence — "Could not read
     the setups: {err}" — and the engine speaks catalogue English (round 18ar), so it is
     matched back and translated here, once, for every site that says it. */
  if(vars && vars.err!=null && Text.lang!=='en') vars=Object.assign({},vars,{err:Text.word(vars.err)});
  if(vars) s=String(s).replace(/\{(\w+)\}/g,(m,n)=>(vars[n]==null? m : vars[n]));
  return s;
}
