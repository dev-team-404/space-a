/** O1 — 대문 한마디 카드에 그릴 문장. 없으면 null(카드를 그리지 않는다).
 *
 *  방문 중이면 그 방 주인이 게시한 문장, 내 방이면 컷 캡션 우선(그림을 아는 텍스트가 이긴다).
 *  방문 중에 내 캡션·한마디를 참조하면 남의 방에 내 문장이 남으므로 분기가 배타적이다.
 *  게시(publish) 경로는 이 함수를 쓰지 않는다 — visiting을 참조하면 남의 방에 들어간 순간
 *  내 대문이 지워지기 때문이다 (스펙 §2.4 함정 1). */
export function resolveHomeLine(input: {
  visiting: boolean;
  ownerLine: string;
  cutCaption: string | null;
  dailyLine: string | null;
}): string | null {
  const pick = input.visiting ? input.ownerLine : input.cutCaption || input.dailyLine || '';
  return pick.trim() ? pick : null;
}
