// Add status-triggered machine-wide animation to original HTML/JS template.
// Same transform used in Machine-Control online and mics-only Windows.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
export function patchStatusAnimations(root,{readOnly=false}={}){
 const edit=(name,fn)=>{
  const file=resolve(root,name),original=readFileSync(file,'utf8'),modified=fn(original);
  if(original===modified)throw Error('No animation changes in '+name);
  writeFileSync(file,modified);
 };
 const once=(s,a,b,name)=>{
  const count=s.split(a).length-1;
  if(count!==1)throw Error('Animation '+name+' anchor count '+count);
  return s.replace(a,b);
 };
 edit('js/common.js',s=>{
  s=once(s,'    drying: Array.isArray(src.drying) ? normalizeRuleList(src.drying) : [],',
    "    drying: Array.isArray(src.drying) ? normalizeRuleList(src.drying) : [],\n    animations: Array.isArray(src.animations) ? src.animations.filter(r=>r && /^\\d{1,4}$/.test(String(r.status??'').trim()) && ['blink','pulse','glow'].includes(r.effect)).slice(0,50).map(r=>({status:String(r.status).trim(),effect:r.effect})) : [],",
    'alias normalization');
  const helper=[
   "// Exact status match. A rule only changes the visual machine card.",
   "export function machineAnimationEffect(statusCode,cfg=null){",
   " const status=String(statusCode??'').trim();",
   " if(!status)return '';",
   " const row=(cfg||getAliasConfig()).animations?.find(r=>r.status===status);",
   " return ['blink','pulse','glow'].includes(row?.effect)?row.effect:'';",
   "}",
   ""
  ].join('\n');
  return once(s,'export function extractMoldAlias(',helper+'export function extractMoldAlias(','effect lookup');
 });
 edit('js/change-aliases-app.js',s=>{
  s=once(s,"      cat: 'color', // 'color' | 'mold' | 'stamp'",
   "      cat: 'color', // color | mold | stamp | drying | animation\n      animationNewStatus: '500',\n      animationNewEffect: 'blink',",
   'editor state');
  s=once(s,
   "      { id: 'drying', label: '♨ 乾燥 (金型別)', count: (state.cfg.drying || []).length },",
   "      { id: 'drying', label: '♨ 乾燥 (金型別)', count: (state.cfg.drying || []).length },\n      { id: 'animation', label: '⚡ Animation (Status別)', count: (state.cfg.animations || []).length },",
   'category tab');
  const methods=[
   '    const animationReadOnly = '+String(Boolean(readOnly))+';',
   '    function addStatusAnimation(){',
   '      if(animationReadOnly)return;',
   "      const status=String(state.animationNewStatus||'').trim(),effect=state.animationNewEffect;",
   "      if(!/^\\d{1,4}$/.test(status)){pushToast('Statusは1〜4桁の数字で入力してください','err');return;}",
   "      if(!['blink','pulse','glow'].includes(effect))return;",
   "      const rules=state.cfg.animations||(state.cfg.animations=[]);",
   "      const current=rules.find(r=>r.status===status);",
   '      if(current)current.effect=effect;',
   '      else if(rules.length<50)rules.push({status,effect});',
   "      else{pushToast('Animationは最大50件です','err');return;}",
   "      void persistConfig('Status '+status+' のAnimationを設定しました');",
   '    }',
   '    function updateStatusAnimation(status,effect){',
   "      if(animationReadOnly||!['blink','pulse','glow'].includes(effect))return;",
   '      const r=(state.cfg.animations||[]).find(r=>r.status===status);',
   "      if(r){r.effect=effect;void persistConfig('Status '+status+' のAnimationを変更しました');}",
   '    }',
   '    function removeStatusAnimation(status){',
   "      if(animationReadOnly||!confirm('Status '+status+' のAnimationを削除しますか？'))return;",
   '      state.cfg.animations=(state.cfg.animations||[]).filter(r=>r.status!==status);',
   "      void persistConfig('Status '+status+' のAnimationを削除しました');",
   '    }',
   ''
  ].join('\n');
  s=once(s,'    function switchCategory(cat) {',methods+'    function switchCategory(cat) {','editor methods');
  s=once(s,'      switchCategory,\n      addRule,',
    '      switchCategory,\n      animationReadOnly,\n      addStatusAnimation,\n      updateStatusAnimation,\n      removeStatusAnimation,\n      addRule,','editor exposure');
  s=once(s,'          <!-- Add rule bar -->',"\n          <!-- Status-triggered animation settings -->\n          <div v-if=\"state.cat === 'animation'\" class=\"rounded-xl border border-slate-300 bg-slate-50 p-3.5 mb-3.5\">\n            <p class=\"text-sm text-slate-700 mb-3\">Statusコードごとに機械ノード全体のアニメーションを設定。例: <b>500 → Blink (点滅)</b>。未設定のStatusは通常表示です。</p>\n            <p v-if=\"animationReadOnly\" class=\"text-xs text-amber-800 mb-3\">読み取り専用: Machine-Control Adminで編集後、D1 → mics-only 同期してください。</p>\n            <form v-if=\"!animationReadOnly\" class=\"flex flex-wrap items-center gap-2 mb-4\" @submit.prevent=\"addStatusAnimation\">\n              <label class=\"text-xs font-semibold\">Status <input v-model=\"state.animationNewStatus\" class=\"border border-slate-300 rounded-lg px-2 py-1.5 w-24 ml-1\" placeholder=\"500\" inputmode=\"numeric\" maxlength=\"4\" /></label>\n              <label class=\"text-xs font-semibold\">Animation\n                <select v-model=\"state.animationNewEffect\" class=\"border border-slate-300 bg-white rounded-lg px-2 py-1.5 ml-1\">\n                  <option value=\"blink\">Blink (点滅)</option>\n                  <option value=\"pulse\">Pulse (脈動)</option>\n                  <option value=\"glow\">Glow (発光)</option>\n                </select>\n              </label>\n              <button type=\"submit\" class=\"bg-blue-600 text-white font-bold text-sm px-3 py-1.5 rounded-lg\">＋ 設定・更新</button>\n            </form>\n            <div class=\"overflow-x-auto bg-white border border-slate-200 rounded-lg\">\n              <table class=\"w-full text-sm border-collapse\">\n                <thead><tr class=\"bg-slate-100 text-left\"><th class=\"p-2\">Status</th><th class=\"p-2\">Animation</th><th class=\"p-2\">プレビュー</th><th class=\"p-2\" v-if=\"!animationReadOnly\">操作</th></tr></thead>\n                <tbody>\n                  <tr v-if=\"!(state.cfg.animations||[]).length\"><td colspan=\"4\" class=\"p-4 text-slate-500 text-center\">ルールは未設定です。例: 500 → Blink を追加してください。</td></tr>\n                  <tr v-for=\"rule in state.cfg.animations || []\" :key=\"rule.status\" class=\"border-t border-slate-200\">\n                    <td class=\"p-2 font-bold\">{{ rule.status }}</td>\n                    <td class=\"p-2\">\n                      <select :value=\"rule.effect\" :disabled=\"animationReadOnly\" @change=\"updateStatusAnimation(rule.status,$event.target.value)\" class=\"border border-slate-300 rounded-lg p-1.5 bg-white disabled:opacity-70\">\n                        <option value=\"blink\">Blink / 点滅</option><option value=\"pulse\">Pulse / 脈動</option><option value=\"glow\">Glow / 発光</option>\n                      </select>\n                    </td>\n                    <td class=\"p-2\"><div class=\"mics-animation-preview px-3 py-2 rounded-lg border-2 border-blue-400 bg-blue-50 text-blue-950 font-bold text-center\" :class=\"'mics-node-anim-'+rule.effect\">Status {{rule.status}}</div></td>\n                    <td v-if=\"!animationReadOnly\" class=\"p-2\"><button type=\"button\" @click=\"removeStatusAnimation(rule.status)\" class=\"text-red-600 border border-red-300 rounded-lg px-2 py-1\">削除</button></td>\n                  </tr>\n                </tbody>\n              </table>\n            </div>\n          </div>\n\n"+'          <!-- Add rule bar -->','editor panel');
  s=once(s,'            id="add-rule-form"','            v-if="state.cat !== \'animation\'"\n            id="add-rule-form"','hide generic form');
  s=once(s,'          <div class="overflow-auto border border-slate-300 rounded-xl bg-slate-50">',
    '          <div v-if="state.cat !== \'animation\'" class="overflow-auto border border-slate-300 rounded-xl bg-slate-50">',
    'hide generic table');
  return s;
 });
 edit('js/dashboard-app.js',s=>{
  s=once(s,'  getStatusConfig, loadStatusConfig, getAliasConfig, loadAliasConfig, statusStyle,',
   '  getStatusConfig, loadStatusConfig, getAliasConfig, loadAliasConfig, statusStyle, machineAnimationEffect,','dashboard import');
  s=once(s,'      statusStyle,\n      productColorStyle,',
   '      statusStyle,\n      machineAnimationEffect,\n      productColorStyle,','dashboard return');
  s=once(s,':class="{ \'opacity-[0.88]\': metricsByMachine[m.machine_no].idle }"',
   ':class="{ \'opacity-[0.88]\': metricsByMachine[m.machine_no].idle,\n                \'mics-node-anim-blink\': machineAnimationEffect(m.status_code,state.aliasCfg)===\'blink\',\n                \'mics-node-anim-pulse\': machineAnimationEffect(m.status_code,state.aliasCfg)===\'pulse\',\n                \'mics-node-anim-glow\': machineAnimationEffect(m.status_code,state.aliasCfg)===\'glow\' }"',
   'entire node class');
  return s;
 });
 edit('css/tailwind-custom.css',s=>{
  if(s.includes('.mics-node-anim-blink'))throw Error('Animation CSS already injected');
  return s+"\n\n/* Status animations on entire machine node; Blink <1Hz, reduced-motion supported. */\n@keyframes mics-node-status-blink { 0%,45%,100% { opacity:1; } 50%,90% { opacity:.48; } }\n@keyframes mics-node-status-pulse { 0%,100% { transform:scale(1); } 50% { transform:scale(1.012); } }\n@keyframes mics-node-status-glow { 0%,100% { box-shadow:0 0 4px var(--st,#3b82f6); } 50% { box-shadow:0 0 24px 6px var(--st,#3b82f6); } }\n.card.mics-node-anim-blink, .mics-animation-preview.mics-node-anim-blink { animation:mics-node-status-blink 2.2s steps(1,end) infinite; }\n.card.mics-node-anim-pulse, .mics-animation-preview.mics-node-anim-pulse { animation:mics-node-status-pulse 2.4s ease-in-out infinite; }\n.card.mics-node-anim-glow, .mics-animation-preview.mics-node-anim-glow { animation:mics-node-status-glow 2.4s ease-in-out infinite; }\n@media (prefers-reduced-motion: reduce) {\n  .card.mics-node-anim-blink, .card.mics-node-anim-pulse, .card.mics-node-anim-glow,\n  .mics-animation-preview.mics-node-anim-blink, .mics-animation-preview.mics-node-anim-pulse,\n  .mics-animation-preview.mics-node-anim-glow {\n    animation:none !important;\n    outline:3px solid var(--st,#f97316);\n  }\n}\n";
 });
}
