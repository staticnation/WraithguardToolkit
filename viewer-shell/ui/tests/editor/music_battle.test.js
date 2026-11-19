/* Battle music (63_wg_sound.js `musicKind`, `musicNext`, the music tick): Music/Battle while an
   actor of the scene's scripts fights the player, Music/Explore otherwise, a track of the
   other kind faded out on the switch, and Explore playing on when the data folders have no
   Battle music. */
'use strict';
const { page, EDITOR, source } = require('./harness');

module.exports = async (t) => {
  const lists = { explore: ['music/explore/a.mp3', 'music/explore/b.mp3'], battle: ['music/battle/x.mp3'] };
  const asked = [];
  const p = page({ modules: EDITOR, engine: (cmd, a) => { if (cmd === 'music_list') { asked.push(a.kind); return lists[a.kind] || []; } return {}; }, answer: () => ({}) });
  const { w } = p;
  w.eval(source('src/63_wg_sound.js'));
  const S = w.WgSound;
  const tick = () => new Promise(r => setTimeout(r, 0));

  // A stand-in Web Audio: sources that remember being stopped, gains that remember a fade.
  const stopped = [], faded = [];
  const param = () => ({ value: 1, cancelScheduledValues() {}, setValueAtTime() {}, linearRampToValueAtTime(v, at) { faded.push(v); } });
  S.ctx = { currentTime: 0, createBufferSource: () => ({ connect() {}, start() {}, stop() { stopped.push(this.buffer); }, buffer: null }),
    createGain: () => ({ gain: param(), connect() {} }) };
  S.audio = () => S.ctx;
  S.buffer = async ({ path }) => path;
  const P = S.place;
  P.musicOn = true; P.mus = {}; P.scriptMusic = false;

  // No fight: Explore.
  const actors = new Map();
  w.WgScripts = { actors };
  t.ok(S.musicKind() === 'explore', 'no fight: Explore');
  await S.musicNext(); await tick();
  t.ok(P.music && P.music.kind === 'explore' && P.music.file.startsWith('music/explore/'), 'an Explore track plays (' + (P.music && P.music.file) + ')');

  // A rat fights the player (StartCombat player): Battle, the Explore track faded out.
  actors.set('rat', { key: 'rat', dead: false, combat: { player: true } });
  t.ok(S.fighting() && S.musicKind() === 'battle', 'the player fought: Battle');
  const was = P.music;
  S.musicFade(1);
  t.ok(P.music === null && faded.includes(0) && stopped.includes(was.file), 'the Explore track fades out and stops');
  await S.musicNext(); await tick();
  t.ok(P.music.kind === 'battle' && P.music.file === 'music/battle/x.mp3', 'then a Battle track plays');

  // A rat fighting another actor is not the player's fight; a dead one fights no one.
  actors.set('rat', { key: 'rat', dead: false, combat: { key: 'guard' } });
  t.ok(!S.fighting(), 'two others fighting: not the player\'s fight');
  actors.set('rat', { key: 'rat', dead: true, combat: { player: true } });
  t.ok(S.musicKind() === 'explore', 'the foe dead: Explore again');

  // No Battle music in the data folders: Explore plays on through a fight.
  S.forget(); lists.battle = []; P.musicOn = true; P.mus = {};
  actors.set('rat', { key: 'rat', dead: false, combat: { player: true } });
  await S.musicNext(); await tick();
  t.ok(S._noBattle && P.music === null && S.musicKind() === 'explore', 'no Battle music: noted, and Explore is chosen');
  await S.musicNext(); await tick();
  t.ok(P.music && P.music.kind === 'explore', 'an Explore track plays through the fight');
};
