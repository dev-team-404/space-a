import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

// v1 아웃-스코프(방 뷰·차트)는 예외. 이 파일들이 줄면 목록도 축소한다.
const ALLOWLIST = new Set<string>([
  'lib/ui/LifeView.svelte',
  'lib/ui/MiniLife.svelte',
  'lib/ui/home/ModelMix.svelte',
]);

function walk(dir: string, base: string, out: string[]) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, base, out);
    else if (name.endsWith('.svelte')) out.push(full.slice(base.length + 1).replace(/\\/g, '/'));
  }
}

/** <style>…</style> 내부만 추출 */
function styleBlocks(src: string): string {
  const m = src.match(/<style[^>]*>([\s\S]*?)<\/style>/gi) ?? [];
  return m.join('\n');
}

const HEX = /#[0-9a-fA-F]{3,8}\b/;
// --accent는 fill/border 전용. 전경 텍스트에 쓰면 라이트에서 저대비 → --accent-strong를 써야 함.
// (background-color/border-color 등은 앞 글자가 '-'라 lookbehind로 제외)
const ACCENT_TEXT = /(?<![-\w])color:\s*var\(--accent\)/;

describe('컴포넌트 <style>에 하드코딩 hex 색 없음', () => {
  const base = join(process.cwd());
  const files: string[] = [];
  walk(join(base, 'src'), base, files);

  for (const abs of files) {
    const rel = abs.replace(/^src\//, '');
    it(rel, () => {
      if (ALLOWLIST.has(rel)) return; // 아웃-스코프
      const css = styleBlocks(readFileSync(join(base, abs), 'utf8'));
      const hexHit = css.split('\n').find((l) => HEX.test(l));
      expect(hexHit, `하드코딩 hex 발견: ${hexHit?.trim()}`).toBeUndefined();
      const accentText = css.split('\n').find((l) => ACCENT_TEXT.test(l));
      expect(accentText, `전경 color에 --accent (→ --accent-strong 사용): ${accentText?.trim()}`).toBeUndefined();
    });
  }
});
