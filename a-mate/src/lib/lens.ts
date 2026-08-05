// A-Lens(관전 웹) 링크 조립 — 홈 화면 링크와 설정 창이 같은 규칙을 쓰도록 한 곳에 둔다.
//
// 주소는 사람마다 다르다: 각자 기록하는 **팀 공간(space)** 의 방을 봐야 하기 때문이다.
//   <a-lens 주소>/#life/<space_id>
// space_id는 팀 지식 허브 설정(knowledge_hub_space_id)을 그대로 쓴다 — 그 공간에 기록을
// 남기는 사람이 그 공간의 방을 보는 것이 자연스럽고, 값이 이미 개인별로 설정돼 있다.

/** 설정 화면에만 보여줄 로컬 실행 예시. 런타임 기본 연결값은 아니다. */
export const LENS_URL_PLACEHOLDER = 'http://localhost:8600';

/** 허브 기본 공간 — hub.rs·설정 창과 같은 값. */
export const LENS_DEFAULT_SPACE = 'sw-innov';

/** 링크를 숨기고 싶을 때 설정에 넣는 값. */
export const LENS_OFF = 'off';

/**
 * 설정 맵(get_settings) → 열어야 할 A-Lens 주소. 빈 문자열이면 링크를 걸지 않는다.
 * - `a_lens_url` 미설정 → 숨김(빈 문자열 반환)
 * - `a_lens_url` = "off" → 숨김(빈 문자열 반환)
 */
export function lensRoomUrl(settings: Record<string, string | undefined>): string {
  const raw = (settings.a_lens_url ?? '').trim();
  if (!raw || raw.toLowerCase() === LENS_OFF) return '';
  const base = raw.replace(/\/+$/, '');
  if (!/^https?:\/\//i.test(base)) return ''; // 주소 형식이 아니면 죽은 버튼을 만들지 않는다
  const space = (settings.knowledge_hub_space_id ?? '').trim() || LENS_DEFAULT_SPACE;
  return `${base}/#life/${encodeURIComponent(space)}`;
}

/** 링크에 곁들여 보여줄 공간 이름. */
export function lensSpaceLabel(settings: Record<string, string | undefined>): string {
  return (settings.knowledge_hub_space_id ?? '').trim() || LENS_DEFAULT_SPACE;
}
