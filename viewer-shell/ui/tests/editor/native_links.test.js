/* The links the viewer's own Editor answers (editcore::links, port step 4.1, slice 7):
   asked of the engine (`editor_link`) once `editor_open` named them, the rest of Wraithguard;
   with no `extra.editor`, or an engine that cannot open one, every link of Wraithguard. */
'use strict';
const { page, EDITOR } = require('./harness');

const LINKS = { editRecord: 'r', editPending: 'pend', editUses: 'u' };
const ED = { plugins: [['Morrowind.esm', '/data/Morrowind.esm']], journal: '/wg/wraithguard_patch_journal.json' };

module.exports = async (t) => {
  // WG_TAINT marks up every string the engine answers, link names among them: what this
  // checks is which side is asked, not what reaches the page.
  if (process.env.WG_TAINT) return;
  const opened = [];
  const p = page({
    modules: EDITOR, links: LINKS, editor: ED,
    engine: (c, a) => {
      if (c === 'editor_open') { opened.push(a); return JSON.stringify(['editRecord', 'editPending']); }
      if (c === 'editor_link') return a.name === 'editPending' ? '[]' : { tag: 'NPC_', id: 'fargoth', fields: [] };
      return {};
    },
    answer: (url) => url === 'u' ? { tag: 'NPC_', id: 'fargoth', uses: [], live: [], cells: [] } : {},
  });
  const E = p.w.WgEditor;
  t.ok(Array.isArray(await E.ask('editPending', {})), 'a link the viewer answers: its answer');
  await E.ask('editRecord', { tag: 'NPC_', id: 'fargoth' });
  await E.ask('editUses', { tag: 'NPC_', id: 'fargoth' });
  t.ok(opened.length === 1, 'the Editor opened once: ' + opened.length);
  t.ok(opened[0] && opened[0].journal === ED.journal && JSON.stringify(opened[0].plugins) === JSON.stringify(ED.plugins), 'on the launch\'s load order and journal');
  t.ok(JSON.stringify(p.native.map(n => n[0])) === '["editPending","editRecord"]', 'asked of the engine: ' + JSON.stringify(p.native));
  t.ok(p.posts.length === 1 && p.posts[0][0] === 'u', 'the rest of Wraithguard: ' + JSON.stringify(p.posts));

  // An engine with no Editor (an older build): everything to Wraithguard, said once.
  const q = page({
    modules: EDITOR, links: LINKS, editor: ED,
    engine: (c) => { if (c === 'editor_open') throw new Error('command editor_open not found'); return {}; },
    answer: (url) => url === 'pend' ? [] : {},
  });
  await q.w.WgEditor.ask('editPending', {});
  await q.w.WgEditor.ask('editPending', {});
  t.ok(q.native.length === 0 && q.posts.length === 2, 'no Editor: Wraithguard answers: ' + JSON.stringify(q.posts));

  // No `extra.editor` (a viewer launched by an older Wraithguard): the engine is not asked.
  let asked = 0;
  const r = page({ modules: EDITOR, links: LINKS, engine: (c) => { if (c === 'editor_open') asked++; return {}; }, answer: () => [] });
  await r.w.WgEditor.ask('editPending', {});
  t.ok(asked === 0 && r.posts.length === 1, 'nothing to open: Wraithguard answers');

  // Wraithguard hands over the links the viewer answers as `native:` (it does not serve
  // them): asked of the engine, with what the launch named besides the load order.
  const opened2 = [];
  const n = page({
    modules: EDITOR, links: { editRecord: 'native:wg_edit_record', editUses: 'u' },
    editor: Object.assign({ folders: ['/data'], last: '/data/Old.esp', testSettings: '/wg/openmw_test.json' }, ED),
    engine: (c, a) => { if (c === 'editor_open') { opened2.push(a); return ['editRecord']; } return c === 'editor_link' ? '{"fields": []}' : {}; },
  });
  await n.w.WgEditor.ask('editRecord', { tag: 'NPC_', id: 'fargoth' });
  t.ok(n.native.length === 1 && n.posts.length === 0, 'a native: link to the engine: ' + JSON.stringify(n.posts));
  t.ok(opened2[0] && JSON.stringify(opened2[0].folders) === '["/data"]' && opened2[0].last === '/data/Old.esp' && opened2[0].testSettings === '/wg/openmw_test.json',
    'opened with the data folders, the last patch and the test settings: ' + JSON.stringify(opened2[0]));
  // A native: link a viewer cannot answer (one older than Wraithguard) is said, not posted.
  const o = page({ modules: EDITOR, links: { editRecord: 'native:wg_edit_record' }, editor: ED,
    engine: (c) => { if (c === 'editor_open') throw new Error('command editor_open not found'); return {}; } });
  let why = '';
  try { await o.w.WgEditor.ask('editRecord', { tag: 'NPC_', id: 'x' }); } catch (e) { why = String(e.message || e); }
  t.ok(/older than Wraithguard/.test(why) && o.posts.length === 0, 'an older viewer says so: ' + why);

  // A refusal from the viewer's Editor is the link's refusal, as Wraithguard's would be.
  const s = page({
    modules: EDITOR, links: LINKS, editor: ED,
    engine: (c) => { if (c === 'editor_open') return '["editRecord"]'; if (c === 'editor_link') throw new Error('bad id'); return {}; },
  });
  let said = '';
  try { await s.w.WgEditor.ask('editRecord', { tag: 'NPC_', id: 'x' }); } catch (e) { said = String(e.message || e); }
  t.ok(said === 'bad id' && s.posts.length === 0, 'refused, and not asked again of Wraithguard: ' + said);
};
