#!/usr/bin/env node
/* The innerHTML lint (security audit, 2026-11-23).

   Every place the page turns a string into markup (`innerHTML =`, `outerHTML =`,
   `insertAdjacentHTML`, `document.write`) is a place a mod's text could become script if it
   is not escaped on the way in. eslint-plugin-no-unsanitized finds them; there are a few
   hundred, each built from escaped text or from the page's own strings, and going through
   all of them again is not what this check is for. It is for the *next* one: the places
   found today are kept in html_sinks.baseline.json, and a place that is not in it fails
   the check, so a new sink is looked at by a person before it lands.

   A place is known by its file and its own source text (spaces squeezed), not by its line,
   so moving code about does not fail the check; changing what a sink is built from does,
   which is the point.

     node html_sinks.js           check (exit 1 on a new sink)
     node html_sinks.js --update  write the baseline from what is there now (after review) */
'use strict';
const fs=require('fs'), path=require('path');
const {Linter}=require('eslint');
const unsanitized=require('eslint-plugin-no-unsanitized');

const SRC=path.join(__dirname,'..','src');
const BASELINE=path.join(__dirname,'html_sinks.baseline.json');

function sinks(){
  const linter=new Linter({configType:'flat'});
  const config=[{
    files:['**/*.js'],
    languageOptions:{ecmaVersion:'latest', sourceType:'script'},
    plugins:{'no-unsanitized':unsanitized},
    rules:{'no-unsanitized/property':'error', 'no-unsanitized/method':'error'},
  }];
  const found={};
  for(const name of fs.readdirSync(SRC).filter(n=>n.endsWith('.js')).sort()){
    // The files are one <script> of the page (build.rs joins them): its tag lines are
    // blanked, so the lines still count as the file's.
    const text=fs.readFileSync(path.join(SRC,name),'utf8').replace(/^[ \t]*<\/?(script|body|html)\b[^>]*>[ \t]*$/gm,'');
    const lines=text.split('\n');
    for(const m of linter.verify(text, config, {filename:name})){
      if(m.fatal) throw new Error(name+':'+m.line+': '+m.message);
      // The statement the sink is in, from its start: enough to tell one from another.
      const src=lines.slice(m.line-1, (m.endLine||m.line)).join(' ');
      const key=name+' | '+src.slice(Math.max(0,m.column-1)).replace(/\s+/g,' ').trim().slice(0,240);
      (found[key]||(found[key]={count:0, at:[]})).count++;
      found[key].at.push(name+':'+m.line);
    }
  }
  return found;
}

const found=sinks();
if(process.argv.includes('--update')){
  const out={};
  for(const k of Object.keys(found).sort()) out[k]=found[k].count;
  fs.writeFileSync(BASELINE, JSON.stringify(out,null,1)+'\n');
  console.log('html sinks: baseline written, '+Object.values(out).reduce((a,b)=>a+b,0)+' places');
  process.exit(0);
}
const known=JSON.parse(fs.readFileSync(BASELINE,'utf8'));
const fresh=[];
for(const [k,v] of Object.entries(found)){
  if(v.count>(known[k]||0)) fresh.push(v.at.join(', ')+'\n    '+k.split(' | ').slice(1).join(' | '));
}
const total=Object.values(found).reduce((a,v)=>a+v.count,0);
if(fresh.length){
  console.error('html sinks: '+fresh.length+' new place(s) the page writes markup from a string:\n');
  for(const f of fresh) console.error('  '+f+'\n');
  console.error('Build it with escHtml() on every piece that is not the page\'s own text, or with\n'+
                'textContent / createElement. When a person has checked it is safe:\n'+
                '  node html_sinks.js --update   (and say why in the commit)');
  process.exit(1);
}
const gone=Object.keys(known).filter(k=>!found[k]).length;
console.log('html sinks: '+total+' places, none new'+(gone? ' ('+gone+' gone from the baseline: --update to drop them)' : ''));
