export type UpdatePhase =
  | { kind: 'idle' }
  | { kind: 'checking'; manual: boolean }
  | { kind: 'available'; version: string; notes: string | null }
  | { kind: 'downloading'; downloaded: number; total: number | null }
  | { kind: 'uptodate'; manual: boolean }
  | { kind: 'error'; manual: boolean; message: string };

export function shouldShow(p: UpdatePhase): boolean {
  switch (p.kind) {
    case 'available':
    case 'downloading':
      return true;
    case 'checking':
    case 'uptodate':
    case 'error':
      return p.manual;
    case 'idle':
    default:
      return false;
  }
}

export function progressPercent(downloaded: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.max(0, Math.floor((downloaded / total) * 100)));
}

export function bannerText(p: UpdatePhase): string {
  switch (p.kind) {
    case 'checking':
      return '업데이트 확인 중…';
    case 'available':
      return `새 버전 ${p.version} 있음`;
    case 'downloading': {
      const pct = progressPercent(p.downloaded, p.total);
      return pct === null ? '업데이트 다운로드 중…' : `업데이트 다운로드 중… ${pct}%`;
    }
    case 'uptodate':
      return '최신 버전입니다';
    case 'error':
      return `업데이트 확인 실패: ${p.message}`;
    case 'idle':
    default:
      return '';
  }
}
