/* The jobs panel (90_wg_jobs.js, Static 2026-11-24): a long command seen as it goes through the
   engine, its progress, Cancel, Open folder; a short one not listed; and the Lua step's round
   trip split for the performance report (74_wg_lua.js, 05_wg_perf.js). */
'use strict';
const { JSDOM } = require('jsdom');
const fs = require('fs');
const path = require('path');

module.exports = async (t) => {
  const src = f => fs.readFileSync(path.join(__dirname, '..', '..', 'src', f), 'utf8');
  const dom = new JSDOM('<!doctype html><body><div id="top"><button id="btnReport">Report</button></div></body>', { runScripts: 'outside-only', pretendToBeVisual: true, url: 'http://tauri.localhost/' });
  const w = dom.window, d = w.document;
  const wait = ms => new Promise(r => setTimeout(r, ms));
  w.eval('var toast=()=>{}; var $=s=>document.querySelector(s); var calls=[]; var gates={};' +
    'window.__TAURI__={core:{invoke:(c,a)=>{ calls.push([c,a]);' +
    '  if(c==="meshopt_progress") return Promise.resolve([3,10]);' +
    '  if(c==="land_merge_progress") return Promise.resolve(JSON.stringify({lines:["[  1.0s] reading the terrain"], next:1}));' +
    '  if(c==="land_merge"||c==="meshopt_write") return new Promise(res=>{ gates[c]=res; });' +
    '  return Promise.resolve(JSON.stringify({ok:true})); }}};');
  const core = src('03_core.js'), at = core.indexOf('const Engine={');
  w.eval('var Engine=' + core.slice(at + 13, core.indexOf('\n};\n', at) + 3));
  w.eval(src('90_wg_jobs.js').replace(/^const WgJobs=/m, 'var WgJobs='));
  const J = w.WgJobs;

  // A short call is not listed; a long one is, with its progress.
  await w.Engine.call('settings_seed', {});
  await w.Engine.call('conflicts_scan', {});
  t.ok(J.list.length === 1 && d.getElementById('btnJobs').hidden, 'a quick scan is a job, but too short to show');
  const pw = w.Engine.call('meshopt_write', { cfg: 'x', outDir: 'D:/out' });
  const pm = w.Engine.call('land_merge', { cfg: 'x', folder: 'D:/lands' });
  await wait(1100);
  const btn = d.getElementById('btnJobs');
  t.ok(!btn.hidden && btn.textContent === 'Jobs (2)', 'two running: ' + btn.textContent);
  btn.click();
  const panel = d.getElementById('wgJobs');
  t.ok(panel && /3 of 10/.test(panel.textContent) && /reading the terrain/.test(panel.textContent), 'the mesh count and Merge Lands\' last line');
  // Cancel: the engine's stop asked; the job says so when it ends.
  const cancels = [...panel.querySelectorAll('button')].filter(b => b.textContent === 'Cancel');
  t.ok(cancels.length === 2, 'both can be stopped');
  cancels[0].click();
  await wait(20);
  t.ok(w.calls.some(c => c[0] === 'land_merge_cancel'), 'Merge Lands asked to stop');
  w.gates.land_merge(JSON.stringify({ error: 'cancelled', cancelled: true }));
  w.gates.meshopt_write(JSON.stringify({ ok: true }));
  await pm; await pw; await wait(20);
  const lm = J.list.find(j => j.cmd === 'land_merge'), mw = J.list.find(j => j.cmd === 'meshopt_write');
  t.ok(lm.status === 'cancelled' && mw.status === 'done', 'cancelled, and done: ' + lm.status + ', ' + mw.status);
  // Open folder, for what wrote somewhere.
  const opens = [...d.querySelectorAll('#wgJobs button')].filter(b => b.textContent === 'Open folder');
  t.ok(opens.length === 2, 'each has its folder');
  opens.find(b => b.title === 'D:/out').click();
  await wait(10);
  t.ok(w.calls.some(c => c[0] === 'open_path' && c[1].path === 'D:/out'), 'the folder opened');
  t.ok(!/Jobs \(/.test(btn.textContent), 'none running now');

  // The Lua step's split, for the report.
  w.eval(src('05_wg_perf.js').replace(/^const WgPerf=/m, 'var WgPerf='));
  const P = w.WgPerf;
  P.tripPart({ page: 1, ipc: 2, collision: 0.5, pipe: 0.3, read: 7, scripts: 1, write: 0.2 });
  P.tripPart({ page: 3, ipc: 4, collision: 0.5, pipe: 0.5, read: 9, scripts: 2, write: 0.4 });
  P.trip(10); P.trip(18); P.frame();
  const s = P.summary();
  t.ok(s.trip && s.trip.parts && s.trip.parts.read === 9 && s.trip.parts.ipc === 4, 'each part\'s median: ' + JSON.stringify(s.trip.parts));
};
