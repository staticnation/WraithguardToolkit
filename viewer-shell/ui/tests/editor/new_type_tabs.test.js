/* A record type the load order has no record of: its tab exists once the patch makes the
   first record, and "+ Type" starts one without asking the engine for rows it has not got. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let pending = [];
  const asked = [];
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editRevision: 'rev' },
    engine: (c, args) => {
      if (c === 'editor_tags') return [['NPC_', 1]];
      if (c === 'editor_records') { asked.push(args.tag); return { plugins: ['M.esm'], rows: [['fargoth', 'Fargoth', '', [0], 0]] }; }
      return {};
    },
    answer: (url) => url === 'pend' ? pending : url === 'rev' ? { rev: 1, undo: 0, redo: 0 } : {},
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  await p.wait(30);
  const tabs = () => [...d.querySelectorAll('#edTabs [data-tag]')].map(b => b.dataset.tag);
  t.ok(tabs().join() === 'NPC_', 'only the type the load order has: ' + tabs());
  t.ok(d.getElementById('edTabsNew') && !d.getElementById('edTabsNew').hidden, 'the + Type button is there');

  // The menu offers the types without a tab (and not the ones the editor cannot start).
  E.newTypeMenu(d.getElementById('edTabsNew'));
  const labels = [...d.querySelectorAll('.edFlyRoot .it')].map(x => x.textContent);
  t.ok(labels.some(l => l.includes('(STAT)')), 'Static is offered');
  t.ok(!labels.some(l => l.includes('(NPC_)')), 'a type with a tab is not');
  t.ok(!labels.some(l => l.includes('(GMST)') || l.includes('(DIAL)') || l.includes('(SKIL)')), 'game settings, dialogue and skills are not');
  w.WgUI.closeMenus();

  // Choosing one: its tab, empty, the engine not asked for its rows, the id box focused.
  await E.startType('WEAP');
  t.ok(E.tag === 'WEAP' && tabs().includes('WEAP'), 'the new type has a tab on screen: ' + tabs());
  t.ok(!asked.includes('WEAP'), 'the engine was not asked for rows of a type it has none of');
  t.ok(d.activeElement && d.activeElement.id === 'edNewId', 'the id box has the focus');

  // Leaving it empty drops the tab again.
  E.tag = 'NPC_'; E._newType = null; E.fillTabs();
  t.ok(!tabs().includes('WEAP'), 'an empty chosen type does not stay');

  // The patch has made the first record of a type: its tab stays, and shows the record.
  pending = [{ tag: 'STAT', id: 'rock_new', made: { name: '', mesh: '' } }];
  await E.refreshPending();
  await p.wait(20);
  t.ok(tabs().includes('STAT'), 'a type with only patch-made records has a tab: ' + tabs());
};
