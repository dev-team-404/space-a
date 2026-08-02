import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

// 등록된 룰(R6·R7·R8)이 전부 est_tokens_saved=0이라 "~N tok"은 언제나 "~0 tok"으로 렌더됐다.
// 절약 수치를 화면에 보여주는 코드가 다시 들어오는 것을 막는다 (스펙 §1.2 D1·D2).
// 정렬·타입 정의용 사용은 허용 — 사용자에게 **보여주는** 것만 금지한다.

function walk(dir: string, base: string, out: string[]) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, base, out);
    else if (/\.(svelte|ts)$/.test(name) && !name.endsWith('.test.ts')) {
      out.push(full.slice(base.length + 1).replace(/\\/g, '/'));
    }
  }
}

/** .svelte에서 <script>·<style>를 뺀 마크업만 — 여기 남은 것은 곧 화면에 나간다. */
function markup(src: string): string {
  return src
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<style[\s\S]*?<\/style>/gi, '');
}

/** .ts에서 템플릿 리터럴만 — 사용자 문구를 조립하는 자리. */
function templateLiterals(src: string): string {
  return (src.match(/`[^`]*`/g) ?? []).join('\n');
}

const SAVINGS = /est_tokens_saved/;

describe('절약 수치는 화면에 표시하지 않는다', () => {
  const base = process.cwd();
  const files: string[] = [];
  walk(join(base, 'src'), base, files);

  it('스캔 대상 파일이 존재한다', () => {
    expect(files.length).toBeGreaterThan(10);
  });

  it('est_tokens_saved가 화면 문자열에 삽입되지 않는다', () => {
    const offenders: string[] = [];
    for (const rel of files) {
      const src = readFileSync(join(base, rel), 'utf8');
      const rendered = rel.endsWith('.svelte') ? markup(src) : templateLiterals(src);
      rendered.split(/\r?\n/).forEach((line) => {
        if (SAVINGS.test(line)) offenders.push(`${rel}  ${line.trim()}`);
      });
    }
    expect(offenders, `절약 수치 표시 발견:\n${offenders.join('\n')}`).toEqual([]);
  });
});
