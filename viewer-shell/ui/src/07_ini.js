
/* =====================================================================================
   The config model — the thing the editor edits.

   This is input: a person types into it, and input belongs on the side with the
   keyboard. What is *not* here any more is a second definition of the file format.
   Reading a `.ini`, reading and writing `.toml`, deciding what a config means and
   whether it is valid: all of that is the engine's, because it is the same question
   the engine answers everywhere else.

   The shapes below are the editor's own, in the editor's own names — `iGap` because
   that is what twenty years of community configs call it. `configToJson` is the one
   place those names are mapped to the engine's, which is the whole point of having
   only one.
   ===================================================================================== */

const MW_REGIONS=[
  "Ascadian Isles Region","Ashlands Region","Azura's Coast Region","Bitter Coast Region",
  "Grazelands Region","Molag Mar Region","Red Mountain Region","Sheogorad","West Gash Region",
  "Felsaad Coast Region","Hirstaang Forest Region","Isinder Mountains Region",
  "Moesring Mountains Region","Brodir Grove Region","Thirsk Region",
  // Tamriel Rebuilt / Project Tamriel regions seen in existing configs
  "Telvanni Isles Region","Shipal-Shin Region","Midkarth Region","Othreleth Woods Region",
  "Aanthirin Region","Alt Orethan Region","Boethiah's Spine Region","Grasslands Region",
  "Lan Orethan Region","Mephala's Nest Region","Molagreahd Region","Nedothril Region",
  "Roth Roryn Region","Sacred Lands Region","Velothi Mountains Region"
];



/* Round 10: what the ground is doing where a blade would stand. `null` for a gate means
   it is off — the shape the engine sends and expects — so "off" and "set to the defaults"
   are different things, as they must be for a control with a switch. Every default is
   inert: an imported .ini regenerates byte-identically until a control is touched. */
/* Round 18k adds `shoreAlign`/`shoreTurn` (blades lie along the shore, with a turn in
   degrees) and `obstacle` (a band on the distance to the nearest avoided object). */
const TERRAIN_DEF={slopeMin:0, slopeMinFade:0, facing:null, curve:null, curveRadius:512, shore:null,
                   shoreAlign:false, shoreTurn:0, obstacle:null};



const DEFAULT_FILLER='Grass\\filler.nif';







function newConfig(){
  return { name:'untitled.toml', displayName:'',
           /* `iZPositionModifier` is still carried here so an imported .ini round-trips
              with the value it arrived with, but nothing reads it: the offset is a
              per-rule field now, and the import writes the file's global into every rule
              it brings in. */
           global:{iZPositionModifier:0, sObjectPrefix:'GRS_'},
           // No starter section: adding one from the cell's own texture list is a
           // click, and a placeholder only ever gets deleted.
           globalExtra:{}, selectors:[], header:'',
           // Round 18k: the named canopies (`{id, name, match:[patterns]}`), in the rules file.
           canopies:[],
           filler:DEFAULT_FILLER };
}








function selKey(s){
  return s.qualKind==='none' ? s.texture
       : s.qualKind==='named'? s.texture+':ANY_NAMED_CELL'
       : s.texture+':'+(s.qualValue||'');
}

/** A name for one rule that survives its neighbours being added and deleted.
 *
 *  `selKey` is what a rule *is* — its ground and its scope — and that is what the
 *  interface shows. It is not quite an identity: two rules can share a texture and a
 *  scope, which is unusual but legal and is exactly the case the per-rule switches exist
 *  for, so a bare `selKey` would have one switch standing for two rules. An ordinal is
 *  appended only when there is a clash, so the common case stays readable.
 *
 *  Deliberately not the index. Indices move when a rule above is added or deleted, and a
 *  viewport switch that silently jumps to a different rule is worse than no switch. */
function ruleTag(cfg,i){
  const list=(cfg&&cfg.selectors)||[];
  const s=list[i]; if(!s) return '';
  const key=selKey(s);
  let n=0, dupes=0;
  for(let k=0;k<list.length;k++){
    if(selKey(list[k])!==key) continue;
    dupes++;
    if(k<i) n++;
  }
  return dupes>1 ? key+'#'+n : key;
}




















