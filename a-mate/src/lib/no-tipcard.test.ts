import { describe, it, expect } from 'vitest';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

// 「오늘의 배움」 컨테이너는 항목마다 독립 카드로 해체됐다 (스펙 §4.1.1).
// 컨테이너가 되살아나면 두 섹션의 카드 문법이 다시 어긋난다.

// 이 파일 자체가 'TipCard'를 문자열로 담고 있어 스캔 대상에서 뺀다.
const SELF = 'src/lib/no-tipcard.test.ts';

function walk(dir: string, base: string, out: string[]) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, base, out);
    else if (/\.(svelte|ts)$/.test(name)) out.push(full.slice(base.length + 1).replace(/\\/g, '/'));
  }
}

describe('TipCard 컨테이너 해체', () => {
  const base = process.cwd();

  it('TipCard.svelte가 존재하지 않는다', () => {
    expect(existsSync(join(base, 'src/lib/ui/home/TipCard.svelte'))).toBe(false);
  });

  it('TipCard를 import하는 곳이 없다', () => {
    const files: string[] = [];
    walk(join(base, 'src'), base, files);
    const offenders = files
      .filter((rel) => rel !== SELF)
      .filter((rel) => /TipCard/.test(readFileSync(join(base, rel), 'utf8')));
    expect(offenders, `TipCard 참조 발견: ${offenders.join(', ')}`).toEqual([]);
  });
});
