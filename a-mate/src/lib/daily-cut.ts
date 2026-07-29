/** H2 — 매일 컷 옵트인 여부. 기본 off — 매일 반복 과금 + 일기 파생 소재가
 *  이미지 엔진으로 전송되므로 명시 동의가 필요하다 (ADR 0024). */
export function dailyCutEnabled(settings: Record<string, string>): boolean {
  return settings['daily_cut_enabled'] === 'true';
}
