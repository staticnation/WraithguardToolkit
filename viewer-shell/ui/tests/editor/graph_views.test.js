/* Graph views (83_wg_graph.js; Static 2026-11-23: "would also be nice to have a graph view in
   dialogue and journal / quests"): a topic's responses in the order they are tried, the choices
   and the responses answering them, and what each does; a quest's stages and what sets each.
   The boxes and their branches drawn, folding, and the clicks that open and follow. */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const dom = new JSDOM('<!doctype html><body></body>', { runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, d = w.document;
  w.eval('var escHtml=s=>String(s==null?"":s).replace(/[&<>"\']/g,c=>"&#"+c.charCodeAt(0)+";"); var toast=()=>{}; var $=s=>document.querySelector(s); var loEsc=escHtml;');
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  w.eval(src('83_wg_graph.js').replace(/^const WgGraph=/m, 'var WgGraph='));
  w.eval(src('52_wg_dialogue_views.js').replace(/^const WgDial=/m, 'var WgDial='));
  const D = w.WgDial;

  // A topic: a greeting with a choice answered by two responses, and a fallback.
  const R = (id, n, more) => Object.assign({ id, n, speaker: 'fargoth', segments: [{ t: 'line ' + id }], conditions: [], choices: [], addtopics: [], journal: [], sets: [], items: [], other: [], plugins: ['Morrowind.esm'], gate: null }, more);
  const rows = [
    R('1', 1, { conditions: ['Disposition >= 50'], choices: [{ n: 1, label: 'Yes' }, { n: 2, label: 'No' }], addtopics: ['latest rumors'], journal: [{ quest: 'MS_Lookout', index: 10 }] }),
    R('2', 2, { gate: 1, sets: [{ name: 'fargothState', target: '' }] }),
    R('3', 3, { gate: 2, goodbye: true }),
    R('4', 4)];
  const tree = D.graphTree('Background', rows);
  t.ok(tree.children.length === 2 && tree.children[0].edge === '1' && tree.children[1].edge === '2', 'the responses not answering a choice, in the order tried');
  const first = tree.children[0];
  const ch = first.children.filter(c => c.kind === 'choice');
  t.ok(ch.length === 2 && ch[0].children[0].info === '2' && ch[1].children[0].info === '3', 'each choice holds the responses answering it');
  t.ok(first.children.some(c => c.kind === 'addtopic' && c.topic === 'latest rumors') && first.children.some(c => c.kind === 'quest' && c.title === 'MS_Lookout 10'), 'what it does: the topic it adds, the journal entry');
  t.ok(/if Disposition >= 50/.test(first.sub) && /Goodbye/.test(ch[1].children[0].badge), 'its conditions, and a goodbye said');

  const host = d.createElement('div'); d.body.append(host);
  let opened = null, drawn = null;
  const g = w.WgGraph.tree(host, tree, { colors: D.GRAPH_COL, onClick: n => { opened = n; }, onDbl: n => { drawn = n; } });
  let count = 0; const walk = n => { count++; (n.children || []).forEach(walk); }; walk(tree);
  const boxes = () => host.querySelectorAll('[data-gid]');
  t.ok(boxes().length === count, 'a box each node (' + boxes().length + '/' + count + ')');
  t.ok([...host.querySelectorAll('text')].some(x => x.textContent === 'choice 1') && [...host.querySelectorAll('text')].some(x => x.textContent === 'adds'), 'the branches are labelled');
  host.querySelector('[data-gid="2"]').onclick();
  t.ok(opened && opened.info === '2', 'a click gives the node (a response: opened)');
  host.querySelector('[data-gid="1:t:latest rumors"]').ondblclick();
  t.ok(drawn && drawn.topic === 'latest rumors', 'a double-click gives it too (a topic: drawn)');
  host.querySelector('[data-fold="1"]').onclick({ stopPropagation() {} });
  let under = 0; const walk2 = n => { (n.children || []).forEach(c => { under++; walk2(c); }); }; walk2(first);
  t.ok(boxes().length === count - under, 'folded: what is under it is left out (' + boxes().length + ')');
  t.ok(!w.document.body.innerHTML.includes('<script'), 'nothing of the mod\'s text runs');

  // The Dialogue window's Graph: drawn from the flow the Editor answers.
  D.E = { ask: async () => ({ id: 'Background', responses: rows }), openRecord: (tag, id) => { opened = { tag, id }; } };
  D.box = d.createElement('div'); d.body.append(D.box);
  await D.graph('Background');
  t.ok(D.box.querySelectorAll('[data-gid]').length === count, 'the Graph view draws the topic');
  D.box.querySelector('[data-gid="4"]').onclick();
  t.ok(opened.tag === 'INFO' && opened.id === '4', 'a response clicked opens in the record dialog');

  // Tools → Journal's Graph.
  w.eval(src('61_wg_tools.js').replace(/^const WgToolsPage=/m, 'var WgToolsPage='));
  const T = w.WgToolsPage;
  const quest = { id: 'MS_Lookout', stages: [
    { infoId: 'a', index: 10, text: 'Fargoth asked me', finished: false, state: '', plugins: ['Morrowind.esm'], conditions: [], effects: [], alsoSets: [],
      setBy: [{ ownerType: 'Response', ownerId: 'Background 1', plugin: 'Morrowind.esm', conditions: ['Disposition >= 50'], speaker: ['fargoth'], effects: [] }] },
    { infoId: 'b', index: 100, text: 'Done', finished: true, state: '', plugins: ['Morrowind.esm'], conditions: [], effects: [], setBy: [],
      alsoSets: [{ quest: 'MS_Other', index: 5, target: { quest: 'MS_Other', infoId: 'x' } }] }] };
  const qt = T.questTree(quest);
  t.ok(qt.children.map(c => c.edge).join() === '10,100' && qt.children[1].kind === 'finished', 'the stages in order, the last one finishing it');
  t.ok(qt.children[0].children[0].kind === 'response' && qt.children[0].children[0].sub === 'Disposition >= 50', 'what sets a stage, with its conditions');
  t.ok(qt.children[1].children.some(c => c.kind === 'none') && qt.children[1].children.some(c => c.jq === 'MS_Other'), 'a stage nothing sets is said so; another quest it sets is a link');
};
