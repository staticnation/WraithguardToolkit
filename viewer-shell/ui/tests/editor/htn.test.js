/* HTN AI (82_wg_htn.js, Static 2026-11-23): the example domain checks clean and exports for both
   engines; the checks catch what cannot run; what a person types reaches the Lua only as data;
   the Preview run plans, runs, notices the world change and takes an earlier branch; the page
   draws the tree, selects, adds, and renames a value everywhere it is used. */
'use strict';
const { page, EDITOR, source } = require('./harness');

module.exports = async (t) => {
  const sent = [];
  const p = page({ modules: EDITOR, engine: (c, a) => { sent.push([c, a]); if (c === 'htn_deps') return { omw: true, mwse: false, luajit: 'C:/OpenMW/lua51.dll', rubic0n: false }; if (c === 'htn_export') return a.files.map(f => 'X/' + f[0]); return {}; }, answer: () => ({}) });
  const { w } = p;
  const d = w.document;
  w.toast = () => {};
  w.eval(source('src/82_wg_htn.js').replace(/^const WgHtn=/m, 'var WgHtn='));
  const H = w.WgHtn;

  // The example: clean, and both engines' files.
  H.doc = H.example();
  const probs = H.validate();
  t.ok(probs.filter(x => x.level === 'error').length === 0, 'the example has no errors ' + JSON.stringify(probs));
  const files = H.files();
  const names = files.map(f => f[0]);
  t.ok(['wg_htn_guard.omwscripts', 'scripts/wg_htn_guard/actor.lua', 'scripts/wg_htn_guard/player.lua', 'MWSE/mods/wg_htn_guard/main.lua', 'wg_htn_guard.wghtn.json'].every(n => names.includes(n)), 'both engines\' files, and the domain ' + names.join(', '));
  const actor = files.find(f => f[0] === 'scripts/wg_htn_guard/actor.lua')[1], main = files.find(f => f[0] === 'MWSE/mods/wg_htn_guard/main.lua')[1];
  t.ok(/pcall\(require, "scripts\.sb_htn\.interface"\)/.test(actor) && /type\(table\.size\) ~= "function"/.test(actor), 'OpenMW: FluadHTN required, not carried; Rubic0n asked for when missing');
  t.ok(/pcall\(require, "sb_htn\.interop"\)/.test(main) && /tes3\.event\.simulate/.test(main), 'MWSE: FluadHTN required; run each simulate');
  t.ok(/\["imperial guard"\] = true/.test(actor) && /:Sequence\("Back off when hurt"\)/.test(actor) && /:Effect\("Set Greeted", E\.PlanAndExecute/.test(actor), 'the actors, the domain and its effects are in the Lua');
  t.ok(/NPC, CREATURE: scripts\/wg_htn_guard\/actor\.lua/.test(files[0][1]), 'the .omwscripts registers the actor script');
  t.ok(!/sb_htn\/|Safebox36/.test(names.join(' ')), 'no FluadHTN file is in the export');

  // What a person types is data in the Lua.
  const evil = H.example();
  evil.name = 'Bad\nos.exit()';
  evil.root.children[1].do.args.text = 'Hi" .. os.exit() .. "\n]]';
  H.doc = evil;
  const ea = H.files().find(f => f[0] === 'scripts/wg_htn_guard/actor.lua')[1];
  t.ok(ea.includes('G.say(a, "Hi\\" .. os.exit() .. \\"\\n]]")') && !/^os\.exit\(\)/m.test(ea), 'a message and a name stay inside their strings and comments');

  // The checks.
  const bad = H.empty();
  bad.modId = 'Bad Id';
  bad.root.children.push(H.node('select', 'Nothing'), H.node('pause'), H.node('action', 'Uses a missing value', { conds: [H.check('state', { var: 'Gone', op: '==', value: 1 })] }));
  H.doc = bad;
  const bp = H.validate().map(x => x.text).join('\n');
  t.ok(/mod id/.test(bp) && /with nothing in it/.test(bp) && /Pause plan only works inside a Sequence/.test(bp) && /Gone is not there/.test(bp) && /which actors/.test(bp), 'the checks catch what cannot run:\n' + bp);
  const shadow = H.example();
  shadow.root.children.unshift(H.node('action', 'Always'));
  H.doc = shadow;
  t.ok(H.validate().some(x => x.level === 'warn' && /can always be done/.test(x.text)), 'a branch that hides the ones after it is warned of');

  // The Preview run, on a pretend scene: the Scripts' layer as far as the run uses it.
  const A = { key: 'k2', id: 'imperial guard', hp: 100, max: [100, 100, 100], items: new Map(), ai: null, combat: null, dead: false };
  const said = [], ais = [];
  let playerDist = 300;
  w.App._scene = { cells: [{ refs: [{ key: 'k2', id: 'Imperial Guard', pos: [0, 0, 0], rot: [0, 0, 0] }] }] };
  w.WgScripts = { on: true, inst: new Map(), actorDefs: new Map([['imperial guard', { tag: 'NPC_' }]]),
    actorOf: () => A, dist: () => playerDist, posOf: () => [0, 0, 0], refById: () => null,
    ai: (tt, kind, args) => { ais.push(kind); A.ai = { kind, args }; }, say: (who, k, text) => said.push(text), playAnim: () => {} };
  H.doc = H.example();
  H.startRun('k2');
  t.ok(H.run && H.run.A === A, 'the run starts on the actor');
  const step = n => { for (let i = 0; i < n; i++) H.frame(0.25); };
  step(2);
  t.ok(said.includes('Halt! Keep the peace.'), 'the player near: it greets ' + JSON.stringify(H.run.log));
  const gi = H.run.doc.state.findIndex(v => v.name === 'Greeted');
  t.ok(H.run.ws[gi] === 1, 'Greeted is set when the greeting is done');
  step(3);
  t.ok(ais.includes('wander') && H.run.cur && H.run.cur.name === 'Wander', 'then it patrols ' + (H.run.cur && H.run.cur.name));
  A.hp = 10;
  step(2);
  t.ok(said.includes('Enough! I yield.'), 'hurt: an earlier branch replaces the patrol ' + JSON.stringify(H.run.log.slice(-6)));
  t.ok(H.run.log.some(l => /Stopped Wander for a better plan/.test(l[1])), 'the patrol was stopped for it');
  H.stopRun();
  t.ok(!H.run, 'the run stops');

  // Partial plans: the steps after a Pause are planned once the ones before it are done.
  const part = H.empty();
  part.modId = 'wg_part'; part.attach.ids = ['imperial guard'];
  part.state = [{ name: 'N', kind: 'number', init: 0, sensor: null }];
  part.root.children.push(H.node('sequence', 'Go then decide', { children: [
    H.node('action', 'First', { do: H.act('say', { text: 'one' }), effects: [{ var: 'N', op: 'add', value: 1, kind: 'run' }] }),
    H.node('pause'),
    H.node('action', 'After', { conds: [H.check('state', { var: 'N', op: '>=', value: 1 })], do: H.act('say', { text: 'two' }) })] }));
  H.doc = part; said.length = 0;
  t.ok(H.validate().filter(x => x.level === 'error').length === 0, 'the partial domain is clean');
  H.startRun('k2'); step(4); H.stopRun();
  t.ok(said[0] === 'one' && said[1] === 'two', 'the step after the Pause runs once the one before it is done ' + JSON.stringify(said));

  // The page.
  H.doc = H.example(); H.path = ''; H.sel = '';
  d.body.appendChild(H.pane()); H.wire();
  H.show();
  await new Promise(r => setTimeout(r, 0));
  const rows = d.querySelectorAll('#tlHtnTree .wgHtnRow');
  let count = 0; H.walk(() => count++);
  t.ok(rows.length === count, 'a row each node (' + rows.length + '/' + count + ')');
  t.ok(/FluadHTN \(OpenMW\)/.test(d.getElementById('tlHtnDeps').textContent) && /✗ Rubic0n/.test(d.getElementById('tlHtnDeps').textContent), 'what the export needs is shown: ' + d.getElementById('tlHtnDeps').textContent);
  const greet = [...rows].find(r => /Greet the player/.test(r.textContent));
  greet.onclick();
  t.ok(/Only when/.test(d.getElementById('tlHtnSide').textContent) && /Show a message/.test(d.getElementById('tlHtnSide').textContent), 'a node selected shows its inspector');
  const before = H.doc.root.children.length;
  H.sel = H.doc.root.id; H.drawTree();
  const addAct = [...d.querySelectorAll('#tlHtnTree button')].find(b => b.textContent === '+ Action');
  addAct.onclick();
  t.ok(H.doc.root.children.length === before + 1 && H.sel === H.doc.root.children[before].id, 'an Action is added and selected');
  H.doc.state.find(v => v.name === 'Greeted').name = 'Welcomed'; H.rename('Greeted', 'Welcomed');
  const g2 = H.doc.root.children.find(n => n.name === 'Greet the player');
  t.ok(g2.conds[1].args.var === 'Welcomed' && g2.effects[0].var === 'Welcomed', 'a value renamed is renamed where it is used');
  // Export: the files sent to the folder picked.
  w.Engine.pick = async () => 'D:\\Mods\\Guard';
  H.doc.root.children.pop();
  await H.exportMod();
  const ex = sent.find(s => s[0] === 'htn_export');
  t.ok(ex && ex[1].folder === 'D:\\Mods\\Guard' && ex[1].files.some(f => f[0] === 'scripts/wg_htn_guard/actor.lua'), 'Export sends the files to the folder picked');
};
