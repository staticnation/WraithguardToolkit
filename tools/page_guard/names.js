// Global names the viewer page uses that nothing in it defines (see README.md here).
// The page's JS parts, in ui/ORDER, as one script through eslint's no-undef.
// Usage: node names.js <viewer-shell/ui>  (eslint 9 and globals 15 installed beside it)
const fs=require('fs'), path=require('path');
const {Linter}=require('eslint'); const globals=require('globals');
const dir=process.argv[2];
const order=fs.readFileSync(path.join(dir,'ORDER'),'utf8').split('\n').map(s=>s.trim()).filter(s=>s&&!s.startsWith('#'));
let src='', map=[];
for(const f of order){ if(!f.endsWith('.js')) continue; const t=fs.readFileSync(path.join(dir,'src',f),'utf8').replace(/\r/g,'').replace(/^\s*<\/?(script|body|html)[^>]*>\s*$/gm,''); const start=src.split('\n').length; src+=t+'\n'; map.push([start,f]); }
const linter=new Linter();
const msgs=linter.verify(src,{languageOptions:{ecmaVersion:2024,sourceType:'script',globals:{...globals.browser,__TAURI__:'readonly'}},rules:{'no-undef':'error'}});
const where=l=>{ let f=map[0]; for(const m of map) if(m[0]<=l) f=m; return f[1]+':'+(l-f[0]+1); };
const seen=new Map();
for(const m of msgs){ if(m.fatal){ console.log('PARSE ERROR', where(m.line), m.message); continue; } const n=(m.message.match(/'([^']+)'/)||[])[1]; if(!seen.has(n)) seen.set(n,[]); seen.get(n).push(where(m.line)); }
for(const [n,ls] of [...seen].sort()) console.log(n+'\t'+ls.length+'\t'+ls.slice(0,3).join(' '));
