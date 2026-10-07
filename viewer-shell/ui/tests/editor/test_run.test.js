/* Test in OpenMW (editTestRun): OpenMW not found asks where it is and tries again with
   it; the cell on screen is sent; the job is followed until the game closes. */
'use strict';
const { page, EDITOR } = require('./harness');

module.exports = async (t) => {
  let polls = 0;
  const p = page({
    modules: EDITOR,
    links: { editRecord: 'r', editPending: 'pend', editTestRun: 'test', editBuildStatus: 'st' },
    engine: (c) => c === 'editor_tags' ? [['WEAP', 1]] : c === 'editor_records' ? { plugins: [], rows: [] } : {},
    answer: (url, b) => {
      if (url === 'pend') return [];
      if (url === 'test') {
        if (!b.exe) throw new Error('Wraithguard answered 400: needExe: Where is OpenMW? Choose openmw.exe (or the folder it is in)');
        return { job: 3 };
      }
      if (url === 'st') {
        polls++;
        const result = { output: 'x.esp', records: 2, pid: 42, note: '' };
        return polls === 1 ? { job: 3, state: 'running', lines: ['writing'], total: 1, result: null, error: null }
          : polls === 2 ? { job: 3, state: 'running', lines: ['Loading cell Vault'], total: 2, result, error: null }
            : { job: 3, state: 'done', lines: [], total: 2, result: { ...result, exit: 0 }, error: null };
      }
      return {};
    },
  });
  const { w, d } = p;
  const E = w.WgEditor;
  await E.enter();
  t.ok(d.getElementById('edTestBtn'), 'the toolbar has Test in OpenMW');
  w.App._scene = { cells: [{ kind: 'int', name: 'Vault' }], origin: [0, 0] };

  const run = E.testRun();
  await p.wait(5);
  const inp = d.querySelector('.edAskBg input');
  t.ok(inp && /Where is OpenMW/.test(d.querySelector('.edAskBg').textContent), 'OpenMW not found: it asks');
  inp.value = 'C:\\Games\\OpenMW'; d.querySelector('.edAskBg [data-a="ok"]').click();
  const st = await run;
  const sent = p.posts.filter(x => x[0] === 'test');
  t.ok(sent.length === 2 && sent[1][1].exe === 'C:\\Games\\OpenMW' && sent[1][1].cell === 'Vault', 'asked again with it, in the cell on screen: ' + JSON.stringify(sent));
  t.ok(st && st.state === 'done' && polls === 3, 'followed to the end');
  t.ok(p.toasts.some(m => /^OpenMW started in Vault/.test(m)) && p.toasts.some(m => /^OpenMW closed/.test(m)), 'said: ' + JSON.stringify(p.toasts));
  E.leave();
};
