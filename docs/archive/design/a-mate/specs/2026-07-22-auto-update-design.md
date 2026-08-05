---
status: done
archived: 2026-08-03
---

# a-mate 앱 내 자동 업데이트 설계

- **날짜**: 2026-07-22
- **컴포넌트**: a-mate (Pillar 1)
- **범위**: a-mate 앱(Tauri v2)에 `tauri-plugin-updater` 기반 인앱 자동 업데이트 도입. 릴리스 채널은 GitHub Releases(공개 릴리스 전용 저장소). 코드 서명 인증서·CI 자동화는 범위 밖.

## 배경 / 문제

현재 a-mate는 자동 업데이트가 없다. 배포는 로컬 Windows에서 수동 빌드해 NSIS 설치 파일(`*-setup.exe`)을 만들어 전달하고, 새 버전마다 사용자가 다시 받아 재설치해야 한다. 로드맵에는 "3단계에서 updater 활성화(서명 인프라 미확정으로 보류)"로만 남아 있었다.

목표는 **일반 사용자가 앱 안에서 쉽게 업데이트**하는 것이다: 앱이 새 버전을 감지 → 알림 → 클릭 한 번으로 다운로드·설치·재시작.

## 결정 요약

| 항목 | 결정 | 비고 |
|------|------|------|
| UX | 앱 내 자동 업데이트 | 감지→다운로드→설치→재시작 |
| 메커니즘 | `tauri-plugin-updater` (v2 공식) | + `tauri-plugin-process`(재시작) |
| 피드 호스팅 | GitHub Releases | 정적 `latest.json` 엔드포인트 |
| 저장소 | **공개 릴리스 전용 저장소** `dev-team-404/a-mate-releases` | 소스(`space-a`)는 private 유지 |
| 서명 | **updater 서명키(무료)만** | Windows 코드 서명 인증서 미포함 → SmartScreen은 최초 설치 1회만 |
| 릴리스 발행 | 수동 릴리스 스크립트 | 개발자 로컬 Windows 실행. CI 미도입 |

### 서명 두 종류 구분 (중요)

| 서명 | 자동 업데이트 필수 | 비용 | 이번 범위 |
|------|--------------------|------|-----------|
| updater 서명키 (minisign) | ✅ 필수 | 무료 | 포함 |
| Windows 코드 서명 인증서 (Authenticode) | ❌ 선택 (SmartScreen 제거용) | 유료 | 제외 |

updater는 자체 서명키만으로 동작한다. 코드 서명 인증서 부재는 **최초 수동 설치 1회**의 SmartScreen 경고에만 영향을 주며, 이후 인앱 자동 업데이트에는 지장이 없다.

## 아키텍처 / 컴포넌트

| # | 컴포넌트 | 변경/추가 | 위치 |
|---|----------|-----------|------|
| 1 | updater 플러그인 (Rust) | `tauri-plugin-updater` 의존성 + `lib.rs` 플러그인 등록 | `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs` |
| 2 | updater 플러그인 (JS) | `@tauri-apps/plugin-updater` | `package.json` |
| 3 | process 플러그인 | `tauri-plugin-process` + `@tauri-apps/plugin-process` (재시작용) | `src-tauri/Cargo.toml`, `package.json`, `lib.rs` |
| 4 | 권한(capabilities) | `updater:default`, `process:allow-restart` 추가 | `src-tauri/capabilities/default.json` |
| 5 | updater 설정 | `plugins.updater`(`endpoints`, `pubkey`) + `bundle.createUpdaterArtifacts: true` | `src-tauri/tauri.conf.json` |
| 6 | 서명 키페어 | `tauri signer generate`로 생성. **public key만 config에 커밋**, private key+암호는 비공개 보관 | config(pubkey) / 릴리스 담당자(privkey) |
| 7 | 업데이트 UI | 시작 시 `check()` → 배너 표시 → `downloadAndInstall()` + 진행률 → `relaunch()` | `src/` (프론트) + 트레이 메뉴 |
| 8 | 릴리스 스크립트 | `release-amate.sh` (빌드·서명·`latest.json` 생성·업로드) | 저장소 스크립트(또는 개발자 로컬) |

### 릴리스 채널

- 새 공개 저장소 `dev-team-404/a-mate-releases`.
- 릴리스마다 자산: `Agent Mentor_<v>_x64-setup.exe`, 해당 `.sig`, `latest.json`.
- 소스 코드는 `space-a`(private)에 그대로. 릴리스 저장소에는 **빌드 산출물만** 올린다.

## 데이터 흐름 (엔드투엔드)

```
[개발자]
  버전 올림 → release-amate.sh 실행
    → (TAURI_SIGNING_PRIVATE_KEY로) 서명 빌드: setup.exe + .sig 생성
    → latest.json 생성 (version/url/signature)
    → gh release create v<버전> --repo dev-team-404/a-mate-releases  setup.exe  latest.json

[사용자 앱]
  시작 시 check()
    → GET <release-base-url>/latest/download/latest.json
    → 현재 버전 < 최신 이면 배너 "새 버전 있음" 표시
    → [지금 업데이트] → 다운로드 → 임베드된 pubkey로 .sig 검증
    → NSIS 설치 실행(덮어쓰기) → relaunch()
```

### `latest.json` 형식 (Tauri v2)

```json
{
  "version": "0.2.0",
  "notes": "변경 내용 요약",
  "pub_date": "2026-07-22T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<.sig 파일 내용>",
      "url": "<release-base-url>/download/v0.2.0/Agent.Mentor_0.2.0_x64-setup.exe"
    }
  }
}
```

`releases/latest/download/latest.json` 별칭이 최신 릴리스의 `latest.json` 자산을 가리키므로, 엔드포인트 URL은 버전이 올라도 고정이다.

### `tauri.conf.json` (해당 부분)

```json
"bundle": {
  "active": true,
  "targets": ["nsis"],
  "createUpdaterArtifacts": true,
  "icon": ["icons/icon.ico", "icons/128x128.png"]
},
"plugins": {
  "updater": {
    "endpoints": ["<release-base-url>/latest/download/latest.json"],
    "pubkey": "<updater public key>"
  }
}
```

## 릴리스 스크립트 (수동 발행)

기존 `update-amate.sh`(사용자 빌드용)를 개발자용 `release-amate.sh`로 발전. bash가 `powershell.exe`를 오케스트레이션한다.

1. **버전 인자** 받기 → `src-tauri/tauri.conf.json` + `src-tauri/Cargo.toml` 버전 갱신 → 커밋·태그(`v<버전>`)
2. `TAURI_SIGNING_PRIVATE_KEY`(+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) env 세팅 후 `npm run tauri build`
   → `Agent Mentor_<v>_x64-setup.exe` + `.sig` 생성 (`CARGO_HTTP_CHECK_REVOKE=false` 유지 — 사내망 TLS 우회)
3. `.sig` 내용 읽어 `latest.json` 생성
4. `gh release create v<버전> --repo dev-team-404/a-mate-releases <setup.exe> <latest.json> --notes ...`

private key는 **커밋하지 않는다**. 릴리스 담당자 로컬 파일 또는 비밀번호 관리자에 보관하고 env로만 주입한다.

## 업데이트 확인 시점 & UI

- **시점**: 앱 시작 시 1회 자동 체크 + 트레이 메뉴 "업데이트 확인"(수동). 주기적 폴링은 미포함(YAGNI).
- **위치**: 메인(chat) 창 상단 배너. "새 버전 X 있음 · [지금 업데이트] [나중]". 진행률은 배너 내 표시.
- **조용한 실패**: 자동 체크 실패(네트워크/피드 불가)는 사용자에게 표시하지 않고 로그만 남기며 다음 시작 시 재시도.
- **수동 체크**: 최신이면 "최신 버전입니다" 안내, 실패면 에러 안내.

## 에러 처리

| 상황 | 동작 |
|------|------|
| 네트워크/피드 접근 실패 (자동) | 조용히 실패, 로그만, 다음 시작 시 재시도 |
| 서명 검증 실패 | 플러그인이 설치 거부(내장) → 에러 배너 |
| 업데이트 없음 (자동) | 무표시 |
| 업데이트 없음 (수동) | "최신 버전입니다" |
| 설치 실행 실패 | 에러 안내 + 로그 |

## 테스트

- **프론트(Vitest)**: 업데이트 배너 컴포넌트 — updater API(`check` / `downloadAndInstall`)를 모킹해 (a) 업데이트 있음, (b) 없음, (c) 실패 3분기의 렌더/상태를 검증.
- **수동 E2E (필수)**: 테스트용 릴리스 저장소에 v0.0.2를 발행하고, v0.0.1 설치본에서 앱이 감지 → 다운로드 → 재시작하는지 실제 확인. 업데이터는 실환경 검증이 핵심이라 자동화만으로 대체 불가.

## 범위 밖 (이번 반복 제외)

- Windows 코드 서명 인증서 / SmartScreen 경고 제거
- CI 기반 자동 릴리스 (GitHub Actions) — 추후 업그레이드 여지
- 주기적 자동 체크(interval polling)
- delta 업데이트, 단계적 롤아웃, 강제 업데이트
- macOS / Linux (a-mate는 Windows 전용)

## 후속 문서화 (프로젝트 규칙)

- **ADR 작성**: "공개 릴리스 전용 저장소 분리 + 자동 업데이트 채널 채택"은 레포 구성이 걸린 되돌리기 어려운 결정이므로 `docs/adr/NNNN-*.md`로 남긴다. (구현 계획에 포함)
- 로드맵 문서(`04-history-and-roadmap.md`, `02-features.md`)의 "updater 뼈대만/보류" 항목을 이 설계 확정에 맞춰 갱신.
