
/* =====================================================================================
   The viewer profile - Wraithguard's.

   The cell viewer has exactly one profile, and Wraithguard owns it. The setup (data
   paths and load order, in the state of Wraithguard's sort lists) arrives as the
   openmw.cfg Wraithguard writes for each launch; this keeps only the viewer's own
   preferences - lighting, sky, effects, camera - in one file Wraithguard names with
   `--prefs` (see `viewer_profile_read`/`viewer_profile_write` in the shell), as the
   JSON `PrevSettings.native()` gives.

   Same surface the preview settings already call (`Profiles.active`, `activeName`,
   `start`), so `PrevSettings` saves into it unchanged.
   Copyright (c) 2026 StaticNation.
   ===================================================================================== */
const Profiles={
  active:'wraithguard',
  known:[],

  activeName(){ return 'Wraithguard'; },
  showName(){},
  async refresh(){},

  /** Reads the viewer profile and applies it to the preview settings. */
  async start(){
    if(!Engine.has()) return;
    let text='';
    try{ text=await Engine.call('viewer_profile_read'); }
    catch(e){ toast(Text.word(String(e.message||e)),'warn',8000); return; }
    // The page's own settings, as it wrote them (JSON). Anything else - empty, or an
    // older build's TOML - starts from the defaults.
    // Engine.call hands back a JSON reply already parsed; a string is parsed here.
    let preview=null;
    if(text && typeof text==='object') preview=text;
    else { try{ preview=JSON.parse(text); }catch(_){ preview=null; } }
    if(preview && typeof preview==='object') PrevSettings.adopt(preview);
  },

  /** Writes the viewer profile: the preview settings as JSON. */
  write(text){ return Engine.call('viewer_profile_write',{text}); },
};
