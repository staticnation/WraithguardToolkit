/* The Editor's links are the viewer's own (editcore::links, port step 4.1, slice 7): asked of
   the engine (`editor_link`) once `editor_open` named them; no host hands any over since the
   Python app's loopback links went (2026-11-10). With no `extra.editor`, the install the
   viewer has open (`editor_setup`); an engine that cannot open one: editing off, said. */
'use strict';
const { page, EDITOR } = require('./harness');

const ED = { plugins: [['Morrowind.esm', '/data/Morrowind.esm']], journal: '/wg/wraithguard_patch_journal.json' };

module.exports = async (t) => {
  // WG_TAINT marks up every string the engine answers, link names among them: what this
  // checks is which side is asked, not what reaches the page.
  if (process.env.WG_TAINT) return;
  const opened = [];
  const p = page({
    modules: EDITOR, editor: Object.assign({ folders: ['/data'], last: '/data/Old.esp', testSettings: '/wg/openmw_test.json' }, ED),
    engine: (c, a) => {
      if (c === 'editor_open') { opened.push(a); return JSON.stringify(['editRecord', 'editPending']); }
      if (c === 'editor_link') return a.name === 'editPending' ? '[]' : { tag: 'NPC_', id: 'fargoth', fields: [] };
      return {};
    },
  });
  const E = p.w.WgEditor;
  t.ok(Array.isArray(await E.ask('editPending', {})), 'a link the Editor answers: its answer');
  await E.ask('editRecord', { tag: 'NPC_', id: 'fargoth' });
  t.ok(opened.length === 1, 'the Editor opened once: ' + opened.length);
  t.ok(opened[0] && opened[0].journal === ED.journal && JSON.stringify(opened[0].plugins) === JSON.stringify(ED.plugins), 'on the launch\'s load order and journal');
  t.ok(JSON.stringify(opened[0].folders) === '["/data"]' && opened[0].last === '/data/Old.esp' && opened[0].testSettings === '/wg/openmw_test.json',
    'opened with the data folders, the last patch and the test settings: ' + JSON.stringify(opened[0]));
  t.ok(JSON.stringify(p.native.map(n => n[0])) === '["editPending","editRecord"]' && p.posts.length === 0, 'asked of the engine, nothing posted: ' + JSON.stringify(p.native));
  t.ok(p.w.WgEditor.links().editRecord === 'native:editRecord' && p.w.WgEditor.canEdit(), 'the links are the Editor\'s own');
  // A link the Editor does not name is refused, not sent anywhere.
  let why = '';
  try { await E.ask('editUses', { tag: 'NPC_', id: 'fargoth' }); } catch (e) { why = String(e.message || e); }
  t.ok(/does not answer editUses/.test(why) && p.native.length === 2, 'a link it does not name: ' + why);

  // No `extra.editor`: the install the viewer has open.
  const asked = [];
  const r = page({ modules: EDITOR, engine: (c, a) => { asked.push(c); if (c === 'editor_setup') return ED; if (c === 'editor_open') return ['editPending']; return c === 'editor_link' ? '[]' : {}; } });
  await r.w.WgEditor.ask('editPending', {});
  t.ok(asked.includes('editor_setup') && asked.includes('editor_open') && r.native.length === 1, 'from editor_setup: ' + JSON.stringify(asked));

  // An engine with no Editor (an older build): editing off, said once, nothing posted.
  const q = page({ modules: EDITOR, editor: ED, engine: (c) => { if (c === 'editor_open') throw new Error('command editor_open not found'); return {}; } });
  let off = '';
  try { await q.w.WgEditor.ask('editPending', {}); } catch (e) { off = String(e.message || e); }
  t.ok(/Editing needs the Editor open/.test(off) && q.native.length === 0 && q.posts.length === 0 && !q.w.WgEditor.canEdit(), 'no Editor: editing off: ' + off);

  // A refusal from the Editor is the link's refusal.
  const s = page({
    modules: EDITOR, editor: ED,
    engine: (c) => { if (c === 'editor_open') return '["editRecord"]'; if (c === 'editor_link') throw new Error('bad id'); return {}; },
  });
  let said = '';
  try { await s.w.WgEditor.ask('editRecord', { tag: 'NPC_', id: 'x' }); } catch (e) { said = String(e.message || e); }
  t.ok(said === 'bad id' && s.posts.length === 0, 'refused: ' + said);
};
