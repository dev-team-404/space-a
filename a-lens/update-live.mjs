#!/usr/bin/env node
// update-live.mjs — 실제 GitHub PR 스냅숏 + 사람용 서사 번역을 live-data.js로 생성한다.
// 서사(말풍선 작업명·이슈 제목·요약)는 PR마다 1회만 LLM(claude CLI)으로 번역해 캐시한다
// — 설계의 "서사는 이벤트 시 1회 생성해 캐시" 원칙 그대로. claude CLI가 없으면 규칙 기반 폴백.
import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';

const run = (cmd, input) =>
  execSync(cmd, { input, encoding: 'utf8', maxBuffer: 10 * 1024 * 1024 });

const prs = JSON.parse(run(
  'gh pr list --state all --limit 30 --json number,title,body,author,state,createdAt,updatedAt,mergedAt,headRefName,url'
));

// 기존 서사 캐시 로드 — PR 번호+제목이 그대로면 다시 번역하지 않는다
let narratives = {};
if (existsSync('live-data.js')) {
  const m = readFileSync('live-data.js', 'utf8').match(/const LIVE_NARRATIVES = (\{[\s\S]*?\});\n/);
  if (m) try { narratives = JSON.parse(m[1]); } catch { /* 캐시 포맷이 깨졌으면 전체 재번역 */ }
}
const cacheKey = (p) => `${p.number}:${p.title}`;
// 재번역 대상: 캐시에 없거나, 제목이 바뀌었거나, 지난번에 폴백(규칙 기반)으로 때운 것
const pending = prs.filter(
  (p) => narratives[p.number]?.key !== cacheKey(p) || narratives[p.number]?.fallback
);

const fallback = (p) => {
  const TYPE = { docs: '문서', feat: '기능', fix: '버그 수정', refactor: '리팩터링', chore: '정비', test: '테스트' };
  const m = p.title.match(/^(\w+)(?:\(([^)]*)\))?!?:\s*(.*)$/);
  return {
    key: cacheKey(p),
    label: m ? `${m[2] ? m[2] + ' ' : ''}${TYPE[m[1]] || m[1]} 작업`.slice(0, 14) : p.title.slice(0, 14),
    title: (m ? m[3] : p.title).slice(0, 30),
    summary: '',
    fallback: true, // 다음 실행에서 LLM 번역을 재시도한다
  };
};

if (pending.length) {
  const input = pending.map((p) => ({
    number: p.number, title: p.title, state: p.state,
    body: (p.body || '').replace(/\r/g, '').slice(0, 400),
  }));
  const prompt = `다음 GitHub PR들을, 개발을 모르는 사람도 알아보는 한국어 서사로 번역해라. PR마다:
- label: 말풍선용 작업명. 공백 포함 14자 이내의 명사구로, 뒤에 "중"/"완료"를 붙였을 때 자연스러울 것 (예: "문서 구조 재편")
- title: 사람용 한 줄 제목, 30자 이내 — 커밋 컨벤션 프리픽스 없이 무엇을 하는 작업인지
- summary: 무엇이 왜 바뀌는지 한 문장, 45자 이내
주어진 제목·본문이 재료의 전부다. 추가 정보를 요구하지 말고, 제목뿐인 PR은 제목을 의역해 채워라.
다른 텍스트 없이 JSON 배열만 출력:
[{"number":5,"label":"…","title":"…","summary":"…"}]

${JSON.stringify(input, null, 1)}`;
  try {
    let out;
    try { out = run('claude -p --model haiku', prompt); }
    catch { out = run('claude -p --model haiku', prompt); } // 일시 실패 1회 재시도
    const start = out.indexOf('['), end = out.lastIndexOf(']');
    if (start < 0 || end < start) throw new Error(`LLM 응답에 JSON 배열이 없음: "${out.slice(0, 120)}"`);
    const arr = JSON.parse(out.slice(start, end + 1));
    for (const n of arr) {
      const p = prs.find((x) => x.number === n.number);
      if (p) narratives[p.number] = { key: cacheKey(p), label: n.label, title: n.title, summary: n.summary };
    }
    console.log(`서사 번역 ${arr.length}건 생성 (LLM, 나머지 ${prs.length - pending.length}건은 캐시 재사용)`);
  } catch (e) {
    for (const p of pending) narratives[p.number] = fallback(p);
    console.log(`claude CLI 사용 불가 — 규칙 기반 폴백으로 ${pending.length}건 번역 (${String(e.message).split('\n')[0]})`);
  }
}
// 목록에서 사라진 PR의 캐시는 정리
for (const k of Object.keys(narratives)) if (!prs.some((p) => String(p.number) === k)) delete narratives[k];

const d = new Date();
const p2 = (n) => String(n).padStart(2, '0');
writeFileSync('live-data.js', `// live-data.js — update-live.sh가 생성한 실제 GitHub PR 스냅숏 + 서사 번역 캐시. 직접 수정하지 말 것.
const LIVE_PRS = ${JSON.stringify(prs, null, 1)};
const LIVE_NARRATIVES = ${JSON.stringify(narratives, null, 1)};
const LIVE_FETCHED_AT = '${p2(d.getMonth() + 1)}-${p2(d.getDate())} ${p2(d.getHours())}:${p2(d.getMinutes())}';
`);
console.log(`live-data.js 갱신 완료 — PR ${prs.length}건`);
