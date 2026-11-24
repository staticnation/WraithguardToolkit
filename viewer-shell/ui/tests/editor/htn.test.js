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
  t.ok(/pcall\(function\(\) return require\("scripts\.sb_htn\.interface"\) end\)/.test(actor) && /type\(table\.size\) ~= "function"/.test(actor), 'OpenMW: FluadHTN required, not carried; Rubic0n asked for when missing');
  t.ok(/pcall\(function\(\) return require\("sb_htn\.interop"\) end\)/.test(main) && /tes3\.event\.simulate/.test(main), 'MWSE: FluadHTN required; run each simulate');
  t.ok(/\["imperial guard"\] = true/.test(actor) && /:Select\("Back off when hurt"\)/.test(actor) && /:Effect\("Set Greeted", E\.PlanAndExecute/.test(actor), 'the actors, the domain and its effects are in the Lua');
  t.ok(/NPC, CREATURE: scripts\/wg_htn_guard\/actor\.lua/.test(files[0][1]), 'the .omwscripts registers the actor script');
  t.ok(!/sb_htn\/|Safebox36/.test(names.join(' ')), 'no FluadHTN file is in the export');

  // Every block is written for both engines (none falls to the default).
  for (const k of Object.keys(H.CHECKS)) for (const eng of ['omw', 'mwse']) t.ok(H.luaCheck(H.check(k), eng) !== '(false)', 'check ' + k + ' for ' + eng);
  for (const k of Object.keys(H.ACTIONS)) t.ok(H.luaAct(H.act(k), 'omw')[0] !== 'return T.Failure', 'action ' + k);
  for (const k of Object.keys(H.CHECKS)) t.ok(!/^(not: )?undefined/.test(H.checkText(H.check(k))), 'check ' + k + ' reads as words');

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
  // Hurt and the player far: the new blocks in the Preview run - it runs from the player.
  H.stopRun();
  const flee = H.empty(); flee.modId = 'wg_flee'; flee.attach.ids = ['imperial guard'];
  flee.root.children.push(H.node('action', 'Run', { conds: [H.check('player_health_below', { percent: 101 })], do: H.act('flee_player', { distance: 1000 }) }));
  w.WgScripts.actorOf = (tt) => tt && tt.player ? { hp: 100, max: [100, 100, 100], items: new Map() } : A;
  w.WgScripts.posOf = (tt) => tt && tt.player ? [100, 0, 0] : [0, 0, 0];
  H.doc = flee; ais.length = 0; playerDist = 300;
  H.startRun('k2'); step(1);
  t.ok(ais[0] === 'travel' && A.ai.args[0] < 0, 'it runs away from the player ' + JSON.stringify(A.ai));
  playerDist = 1200; step(1);
  t.ok(H.run.log.some(l => /Done: Run/.test(l[1])), 'and is done once far enough');
  w.WgScripts.actorOf = () => A; w.WgScripts.posOf = () => [0, 0, 0];
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

  // The run on FluadHTN: the exported Lua added to the Preview's Lua for this actor, its planner's
  // events read back from what it prints.
  let starts = 0;
  w.WgLua = { extra: null, start: async () => { starts++; } };
  H.deps = { omw: true, rubic0n: false };
  H.doc = H.example();
  await H.startFluad('k2');
  const X = w.WgLua.extra;
  t.ok(starts === 1 && X && X.tableExt === true && X.scripts.some(s => s[0] === 'scripts/wg_htn_guard/actor.lua' && s[1].includes('NPC')), 'the Lua starts again with the mod added, Rubic0n\'s table functions asked for');
  const ta = X.files.find(f => f[0] === 'scripts/wg_htn_guard/actor.lua')[1];
  t.ok(/\["imperial guard"\] = true/.test(ta) && /wg_htn_trace/.test(ta) && !X.files.some(f => /omwscripts|wghtn/.test(f[0])), 'for this actor alone, traced, Lua files only');
  t.ok(!/wg_htn_trace/.test(H.files().find(f => f[0] === 'scripts/wg_htn_guard/actor.lua')[1]), 'an export is never traced');
  const P = 'scripts/wg_htn_guard/actor.lua';
  t.ok(H.fromLua(P, 'print', '[wg_htn_trace] plan|Greet the player') && H.fromLua(P, 'print', '[wg_htn_trace] task|Greet the player'), 'its events are taken');
  t.ok(H.run.curName === 'Greet the player' && H.run.plan.length === 0, 'the step it runs is known');
  H.fromLua(P, 'print', '[wg_htn_trace] ws|1,0,1');
  t.ok(H.run.ws.join() === '1,0,1', 'and the world state');
  t.ok(H.fromLua(P, 'print', 'plain line') === false, 'other prints pass through');
  H.stopRun();
  t.ok(!H.run && w.WgLua.extra === null && starts === 2, 'stopped: the Lua starts again without it');
  H.deps = { omw: false };
  await H.startFluad('k2');
  t.ok(H.run && H.run.mode !== 'fluad', 'without FluadHTN installed it runs on the page\'s planner');
  H.stopRun();

  // Templates: each checks clean once it has its actors.
  for (const [k] of H.TEMPLATES) {
    if (k === 'empty') continue;
    const d = H.template(k);
    if (!d.attach.ids.length) d.attach.ids = ['someone'];
    const errs = H.validate(d).filter(x => x.level === 'error');
    t.ok(errs.length === 0, 'the ' + k + ' template is clean ' + JSON.stringify(errs));
  }

  // Reusable parts: built first, spliced in; planned through by the page's planner; a circle refused.
  const pd = H.template('guard');
  const pt = H.node('select', 'Idle about', { children: [H.node('action', 'Look around', { do: H.act('say', { text: 'part ran' }) })] });
  pd.subs = [pt];
  pd.root.children = [H.node('splice', 'Idle about', { sub: pt.id })];
  H.doc = pd;
  t.ok(H.validate().filter(x => x.level === 'error').length === 0, 'a domain using a part is clean');
  const pl = H.files().find(f => f[0] === 'scripts/wg_htn_guard/actor.lua')[1];
  t.ok(pl.indexOf('PART["' + pt.id + '"] = htn.DomainBuilder:new(Ctx, "Idle about")') >= 0 && pl.indexOf(':Splice(PART["' + pt.id + '"])') > pl.indexOf('PART["' + pt.id + '"] ='), 'the part is built, then spliced in');
  said.length = 0; H.startRun('k2'); step(2); H.stopRun();
  t.ok(said.includes('part ran'), 'the page\'s planner plans through a part ' + JSON.stringify(said));
  const loop = H.node('select', 'Loop', { children: [] });
  loop.children.push(H.node('splice', 'Loop', { sub: pt.id }));
  pt.children.push(H.node('splice', 'Back', { sub: loop.id }));
  pd.subs.push(loop);
  t.ok(H.validate().some(x => /in a circle/.test(x.text)), 'parts that use each other in a circle are refused');

  // Moving by dragging (`moveTo`), and undo / redo.
  H.doc = H.template('guard'); H.fresh();
  const [hu, gr, pa] = H.doc.root.children;
  t.ok(H.moveTo(gr.id, { to: pa.id, inside: true }) && pa.children[pa.children.length - 1] === gr, 'a row dropped on a Sequence goes in, last');
  t.ok(!H.moveTo(pa.id, { to: gr.id, inside: false, after: false }), 'not into itself');
  t.ok(H.moveTo(gr.id, { to: hu.id, inside: false, after: false }) && H.doc.root.children[0].id === gr.id, 'above a row');
  const pz = H.node('pause'); pa.children.push(pz); H.changed();
  t.ok(!H.moveTo(pz.id, { to: hu.id, inside: false, after: true }), 'a Pause stays in a Sequence');
  H.undo(); H.undo();
  t.ok(H.doc.root.children[0].name === 'Back off when hurt' && H.doc.root.children[1].children.some(c => c.name === 'Greet the player'), 'undone twice: back where the first move left it');
  H.redo();
  t.ok(H.doc.root.children[0].name === 'Greet the player', 'redone');

  // The page.
  H.doc = H.example(); H.path = ''; H.sel = ''; H.deps = null;
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
