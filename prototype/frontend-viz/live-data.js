// live-data.js — update-live.sh가 생성한 실제 GitHub PR 스냅숏 + 서사 번역 캐시. 직접 수정하지 말 것.
const LIVE_PRS = [
 {
  "author": {
   "id": "MDQ6VXNlcjg3ODk5MTQz",
   "is_bot": false,
   "login": "JuyoungKimmy-Kim",
   "name": "Kimmy Kim"
  },
  "createdAt": "2026-07-13T14:54:07Z",
  "headRefName": "docs/space-view-restructure",
  "mergedAt": null,
  "number": 5,
  "state": "OPEN",
  "title": "docs(space-view): restructure frontend-viz docs into leveled space-view docs",
  "updatedAt": "2026-07-13T14:56:38Z",
  "url": "https://github.com/dev-team-404/space-a/pull/5"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjkzMTE5OTA=",
   "is_bot": false,
   "login": "msaltnet",
   "name": "Jeong Seongmoon"
  },
  "createdAt": "2026-07-12T13:51:57Z",
  "headRefName": "docs/adr-repo-structure",
  "mergedAt": "2026-07-13T08:53:40Z",
  "number": 4,
  "state": "MERGED",
  "title": "docs(adr): record repo structure and shared LLM cluster decisions",
  "updatedAt": "2026-07-13T08:53:40Z",
  "url": "https://github.com/dev-team-404/space-a/pull/4"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjkzMTE5OTA=",
   "is_bot": false,
   "login": "msaltnet",
   "name": "Jeong Seongmoon"
  },
  "createdAt": "2026-07-12T13:19:42Z",
  "headRefName": "docs/contracts-v2-align",
  "mergedAt": "2026-07-13T08:52:55Z",
  "number": 3,
  "state": "MERGED",
  "title": "docs: self-evolving space + align C1/C2 with Pillar 3 (v2)",
  "updatedAt": "2026-07-13T08:52:55Z",
  "url": "https://github.com/dev-team-404/space-a/pull/3"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjkzMTE5OTA=",
   "is_bot": false,
   "login": "msaltnet",
   "name": "Jeong Seongmoon"
  },
  "createdAt": "2026-07-12T11:35:14Z",
  "headRefName": "docs/collab-space-design",
  "mergedAt": "2026-07-12T13:06:44Z",
  "number": 2,
  "state": "MERGED",
  "title": "docs(design): add collab-space design docs for agent collaboration hub",
  "updatedAt": "2026-07-12T13:06:44Z",
  "url": "https://github.com/dev-team-404/space-a/pull/2"
 }
];
const LIVE_NARRATIVES = {
 "2": {
  "key": "2:docs(design): add collab-space design docs for agent collaboration hub",
  "label": "협업 공간 설계",
  "title": "에이전트 협업 허브를 위한 협업 공간 설계 추가",
  "summary": "에이전트들이 협업하는 공간의 구조와 기능을 설계 문서로 정의합니다."
 },
 "3": {
  "key": "3:docs: self-evolving space + align C1/C2 with Pillar 3 (v2)",
  "label": "자체 진화 공간 정렬",
  "title": "자체 진화 공간과 Pillar 3 계약 정렬 (v2)",
  "summary": "자체 진화하는 공간의 개념과 C1/C2를 Pillar 3과 맞춥니다."
 },
 "4": {
  "key": "4:docs(adr): record repo structure and shared LLM cluster decisions",
  "label": "ADR 기록",
  "title": "저장소 구조와 LLM 클러스터 결정 사항 기록",
  "summary": "주요 아키텍처 결정인 저장소 구조와 공유 LLM 클러스터를 ADR로 남깁니다."
 },
 "5": {
  "key": "5:docs(space-view): restructure frontend-viz docs into leveled space-view docs",
  "label": "공간 뷰 문서 재편",
  "title": "프론트엔드 시각화 문서를 단계별 공간 뷰로 재구성",
  "summary": "프론트엔드 시각화 문서를 더 체계적인 공간 뷰 구조로 정리합니다."
 }
};
const LIVE_FETCHED_AT = '07-14 00:12';
