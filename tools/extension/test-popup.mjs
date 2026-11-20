// Browser-level popup regression checks. Install Playwright and Chromium, then run:
//   node tools/extension/test-popup.mjs
// PLAYWRIGHT_MODULE and CHROME_PATH can select an existing local runtime.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || "playwright");
import fs from 'node:fs';
import assert from 'node:assert/strict';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
process.chdir(fileURLToPath(new URL('../../', import.meta.url)));
fs.mkdirSync('target/extension-previews', {recursive:true});
const server=http.createServer((req,res)=>{
 const file=path.resolve('.'+decodeURIComponent(new URL(req.url,'http://localhost').pathname));
 if(!file.startsWith(process.cwd()+path.sep)){res.writeHead(403);res.end();return;}
 const types={'.html':'text/html','.css':'text/css','.js':'text/javascript','.png':'image/png','.woff2':'font/woff2'};
 try{res.setHeader('Content-Type',types[path.extname(file)]||'application/octet-stream');res.end(fs.readFileSync(file));}catch{res.writeHead(404);res.end();}
});
const messages=JSON.parse(fs.readFileSync('extension/_locales/en/messages.json','utf8'));
const version=/^version\s*=\s*"([^"]+)"/m.exec(fs.readFileSync('Cargo.toml','utf8'))[1];
const media=[
 {url:'https://media.test/walkthrough.mp4',name:'Orbit product walkthrough',ext:'mp4',mime:'video/mp4',kind:'video',heights:[1080],size:248*2**20,duration:768},
 {url:'https://media.test/night.mp4',name:'Night Shift — Episode 3',ext:'mp4',kind:'video',drm:true,duration:3120},
 {url:'https://media.test/session.mp3',name:'Acoustic session — live',ext:'mp3',kind:'audio',mime:'audio/mpeg',duration:192,size:38*2**20},
 {url:'https://media.test/keynote.mp4',name:'Keynote recording — 2026',ext:'mp4',mime:'video/mp4',kind:'video',heights:[2160],size:1.4*2**30,duration:2885},
 {url:'https://media.test/review.mp4',name:'Design review — highlights',ext:'mp4',kind:'video',heights:[1080],size:65*2**20,duration:498},
];
const browser=await chromium.launch({headless:true,...(process.env.CHROME_PATH?{executablePath:process.env.CHROME_PATH}:{})});
try {
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const ctx=await browser.newContext({viewport:{width:360,height:800},locale:'sk-SK',colorScheme:'dark'});
const page=await ctx.newPage();
await page.addInitScript(({messages,media,version})=>{
 const data={remote:{url:'https://old.test',token:'old'}};
 const state={hidden:false,excluded:false,fail:false,copyFail:false,connected:true,paused:false,host:'https://github.com/watch',managed:false,calls:[],data,media,copied:[],why:null,declined:null,firefox:new URL(location.href).searchParams.has('firefox'),proxyGranted:false,proxyAnswer:false};
 window.__popup=state;
 Object.defineProperty(navigator,'clipboard',{value:{writeText:async text=>{if(state.copyFail)throw Error('denied');state.copied.push(text);}}});
 const api={
 i18n:{getMessage:(key,subs=[])=>{const m=messages[key];if(!m)return '';let text=m.message;for(const [name,p] of Object.entries(m.placeholders||{}))text=text.replaceAll('$'+name+'$',subs[Number(p.content.slice(1))-1]||'');return text;}},
 tabs:{query:async()=>[{id:1,url:state.host}],create:async()=>{}},
 storage:{local:{get:async d=>({...d,...data}),set:async v=>{if(state.fail)throw Error('storage failed');Object.assign(data,v);},remove:async k=>delete data[k]}},
 permissions:{contains:async()=>state.proxyGranted,request:async()=>{state.proxyGranted=state.proxyAnswer;return state.proxyAnswer;},remove:async()=>{state.proxyGranted=false;return true;}},
 runtime:{getManifest:()=>({permissions:state.firefox?[]:['proxy'],optional_permissions:state.firefox?['proxy']:[],version}),sendMessage:async m=>{state.calls.push(m);
 if(m.type==='state')return {connected:state.connected,paused:state.paused,why:state.why,declined:state.declined,app:version,version};
 if(m.type==='site-settings')return {ok:true,hidden:state.hidden || !((data.downloadButtonSites||{})[m.host] ?? data.showDownloadButton ?? true),hideUndo:!state.managed,excluded:state.excluded,captureUndo:!state.managed};
 if(m.type==='set-site-panels'){if(state.fail)return {ok:false};state.hidden=m.hidden;data.downloadButtonSites={...data.downloadButtonSites,[m.host]:!m.hidden};return {ok:true};}
 if(m.type==='set-site-capture'){if(state.fail)return {ok:false};state.excluded=m.excluded;return {ok:true};}
 if(m.type==='pause'){state.paused=m.paused;return {ok:true};}
 if(m.type==='media')return {media:state.media};return {ok:!state.fail};}}};
 Object.assign(window.chrome,api);
},{messages,media,version});
const errors=[];page.on('pageerror',e=>errors.push(e.message));
await page.goto('http://127.0.0.1:'+server.address().port+'/extension/popup.html');
await page.waitForFunction(()=>!document.getElementById('site-show-buttons').disabled);
await page.evaluate(()=>document.fonts.ready);
assert.equal(await page.locator('html').getAttribute('lang'),'en');
assert.equal(await page.locator('#send, #to-remote').count(),0);
assert.equal(await page.evaluate(()=>window.__popup.data.remote),undefined);
assert.equal(await page.locator('#site-host').textContent(),'github.com');
assert.equal(await page.locator('#download-all-label').textContent(),'Download all (4)');
assert.equal(await page.locator('#media-count').textContent(),'5');
assert.equal(await page.locator('.protected .icon-button').first().isEnabled(),true);
assert.equal(await page.locator('.protected .icon-button').last().isDisabled(),true);
await page.locator('#popup').screenshot({path:'target/extension-previews/main-dark.png'});
assert.equal(await page.locator('#popup').evaluate(x=>x.getBoundingClientRect().height),526);
assert.equal(await page.evaluate(()=>[...document.fonts].every(f=>f.status==='loaded')),true);
await page.emulateMedia({colorScheme:'light'});
await page.locator('#popup').screenshot({path:'target/extension-previews/main-light.png'});
await page.emulateMedia({colorScheme:'dark'});
await page.locator('#media-list').evaluate(x=>x.scrollTop=100);
await page.locator('#settings').click();
assert.equal(await page.locator('#main-view').isHidden(),true);
await page.locator('#popup').screenshot({path:'target/extension-previews/settings-dark.png'});
assert.equal(await page.locator('#popup').evaluate(x=>x.getBoundingClientRect().height),600);
await page.emulateMedia({colorScheme:'light'});
await page.locator('#popup').screenshot({path:'target/extension-previews/settings-light.png'});
assert.equal(await page.locator('#show-download-button').isChecked(),true);
assert.equal(await page.locator('#site-cookies').isChecked(),true);
await page.locator('#site-cookies').uncheck();await page.waitForFunction(()=>window.__popup.data.useSiteCookies===false);
await page.locator('#site-cookies').check();await page.waitForFunction(()=>window.__popup.data.useSiteCookies===true);
await page.locator('#show-download-button').uncheck();await page.waitForFunction(()=>window.__popup.data.showDownloadButton===false);
await page.locator('#back').click();
assert.equal(await page.locator('#site-show-buttons').isChecked(),false,'unconfigured site follows global off');
await page.locator('#site-show-buttons').check();
await page.waitForFunction(()=>window.__popup.data.downloadButtonSites?.['github.com']===true);
await page.locator('#settings').click();
assert.equal(await page.locator('#show-download-button').isChecked(),false);
await page.locator('#show-download-button').check();await page.waitForFunction(()=>!document.getElementById('show-download-button').disabled);
await page.locator('#show-download-button').uncheck();await page.waitForFunction(()=>!document.getElementById('show-download-button').disabled);
await page.locator('#back').click();
assert.equal(await page.locator('#site-show-buttons').isChecked(),true,'site allow survives global on/off');
await page.locator('#site-show-buttons').uncheck();
await page.waitForFunction(()=>window.__popup.data.downloadButtonSites?.['github.com']===false);
await page.locator('#settings').click();await page.locator('#show-download-button').check();
await page.waitForFunction(()=>!document.getElementById('show-download-button').disabled);
await page.locator('#back').click();
assert.equal(await page.locator('#site-show-buttons').isChecked(),false,'site deny survives global on');
await page.locator('#site-show-buttons').check();
await page.waitForFunction(()=>window.__popup.data.downloadButtonSites?.['github.com']===true);
await page.locator('#settings').click();
await page.locator('#proxy').focus();await page.keyboard.press('Space');await page.waitForFunction(()=>window.__popup.data.proxyOn===true);
await page.locator('#video-hide-fullscreen').check();await page.waitForFunction(()=>window.__popup.data.hideVideoFullscreen===true);
await page.locator('#media-title-name').check();await page.waitForFunction(()=>window.__popup.data.mediaTitleName===true);
await page.evaluate(()=>window.__popup.fail=true);await page.locator('#video-hide-fullscreen').click();
await page.waitForFunction(()=>!document.getElementById('video-hide-fullscreen').disabled);
assert.equal(await page.locator('#video-hide-fullscreen').isChecked(),true);await page.evaluate(()=>window.__popup.fail=false);
await page.locator('#skip-key').selectOption('Ctrl');await page.waitForFunction(()=>window.__popup.data.skipKey==='Ctrl');
await page.keyboard.press('Escape');assert.equal(await page.locator('#settings-view').isHidden(),true);
assert.equal(await page.locator('#skip-hint').textContent(),'Ctrl');
assert.equal(await page.locator('#media-list').evaluate(x=>x.scrollTop),60);
assert.equal(await page.evaluate(()=>document.activeElement.id),'settings');
await page.locator('.protected .copy-link').click();await page.waitForFunction(()=>window.__popup.copied.length===1);
assert.equal(await page.locator('#toast-text').textContent(),'Link copied');
await page.evaluate(()=>window.__popup.copyFail=true);await page.locator('.protected .copy-link').click();
await page.waitForFunction(()=>document.getElementById('toast').classList.contains('error'));
await page.evaluate(()=>window.__popup.copyFail=false);
await page.locator('#download-all').click();
await page.waitForFunction(()=>window.__popup.calls.filter(m=>m.type==='media-download').length===4);
const sent=await page.evaluate(()=>window.__popup.calls.filter(m=>m.type==='media-download').map(m=>m.url));
assert.ok(!sent.includes(media[1].url));
await page.evaluate(()=>window.__popup.fail=true);await page.locator('.download').first().click();
await page.waitForFunction(()=>document.getElementById('toast-text').textContent.includes('Could not add'));
assert.equal(await page.locator('.download').first().isEnabled(),true);await page.evaluate(()=>window.__popup.fail=false);
await page.locator('#site-show-buttons').uncheck();await page.waitForFunction(()=>window.__popup.hidden);
assert.equal(await page.locator('#site-capture').isChecked(),true);
await page.locator('#site-capture').uncheck();await page.waitForFunction(()=>window.__popup.excluded);
await page.locator('#site-show-buttons').check();await page.waitForFunction(()=>!window.__popup.hidden);
assert.equal(await page.locator('#site-capture').isChecked(),false);
await page.evaluate(()=>window.__popup.fail=true);await page.locator('#site-capture').click();
await page.waitForFunction(()=>!document.getElementById('site-capture').disabled);
assert.equal(await page.locator('#site-capture').isChecked(),false);
assert.equal(await page.locator('#toast').isVisible(),true);
await page.evaluate(async()=>{Object.assign(window.__popup,{fail:false,hidden:true,managed:true});await show();});
assert.equal(await page.locator('#site-show-buttons').isDisabled(),true);
await page.evaluate(async()=>{Object.assign(window.__popup,{why:'update-app',declined:{why:'file-type',name:'long-file-name.zip'}});await show();});
assert.ok(await page.locator('#popup').evaluate(x=>x.scrollHeight===x.clientHeight && x.clientHeight<=600));
await page.evaluate(()=>Object.assign(window.__popup,{why:null,declined:null}));
await page.evaluate(async()=>{Object.assign(window.__popup,{hidden:false,excluded:false,managed:false,connected:false});await show();});
assert.equal(await page.locator('#take').isDisabled(),true);
assert.equal(await page.locator('#site-capture').isDisabled(),true);
assert.equal(await page.locator('#download-all').isDisabled(),true);
assert.equal(await page.locator('#fix-label').textContent(),'Set up Orbit');
await page.evaluate(()=>document.getElementById('toast').hidden=true);
await page.emulateMedia({colorScheme:'dark'});
await page.locator('#popup').screenshot({path:'target/extension-previews/disconnected.png'});
assert.ok(await page.locator('#popup').evaluate(x=>x.scrollHeight===x.clientHeight && x.clientHeight<=600));
await page.evaluate(async()=>{Object.assign(window.__popup,{connected:true,media:[]});await show();});
await page.locator('#popup').screenshot({path:'target/extension-previews/empty.png'});
assert.equal(await page.locator('#media-none').isVisible(),true);
await page.evaluate(async()=>{window.__popup.host='chrome://newtab/';await show();});
assert.equal(await page.locator('#site').isHidden(),true);
// Firefox's optional proxy permission stays off after denial, then can be granted and revoked.
await page.goto('http://127.0.0.1:'+server.address().port+'/extension/popup.html?firefox=1');
await page.waitForFunction(()=>!document.getElementById('take').disabled);
await page.locator('#settings').click();await page.locator('#proxy').click();
await page.waitForFunction(()=>!document.getElementById('proxy').checked);
assert.equal(await page.evaluate(()=>window.__popup.proxyGranted),false);
await page.evaluate(()=>window.__popup.proxyAnswer=true);await page.locator('#proxy').check();
await page.waitForFunction(()=>window.__popup.proxyGranted);
await page.locator('#proxy').uncheck();await page.waitForFunction(()=>!window.__popup.proxyGranted);
assert.deepEqual(errors,[]);
console.log('Popup browser checks passed: layouts, Settings/back/focus/scroll, stored preferences, site rules and rollback, copy DRM links, batch excludes DRM, disconnected and empty states.');
} finally {await browser.close();server.close();}
