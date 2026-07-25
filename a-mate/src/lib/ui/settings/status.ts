/** 저장·연결 등의 진행/결과 상태. Settings/LifeSettings에 세 번 중복됐던 패턴을 한 곳으로. */
export type StatusKind = 'idle' | 'ok' | 'err' | 'busy';
export interface Status {
  kind: StatusKind;
  text: string;
}

/** text가 빈 문자열이면 StatusLine이 아무것도 렌더하지 않는다. */
export const IDLE: Status = { kind: 'idle', text: '' };

export const busy = (text: string): Status => ({ kind: 'busy', text });
export const ok = (text: string): Status => ({ kind: 'ok', text });
/** catch(e)의 e는 unknown — 무엇이 와도 문자열로 만든다. */
export const err = (e: unknown): Status => ({ kind: 'err', text: `${e}` });
