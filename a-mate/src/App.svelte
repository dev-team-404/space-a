<script lang="ts">
  import './lib/theme.css';
  import HomeTab from './lib/ui/HomeTab.svelte';
  import CoachTab from './lib/ui/CoachTab.svelte';
  import DiaryTab from './lib/ui/DiaryTab.svelte';
  import ChatTab from './lib/ui/ChatTab.svelte';
  import GuestbookTab from './lib/ui/GuestbookTab.svelte';
  import SettingsTab from './lib/ui/settings/SettingsTab.svelte';
  import RobotPortrait from './lib/ui/RobotPortrait.svelte';
  import LensLink from './lib/ui/LensLink.svelte';
  import LifeNavigator from './lib/ui/LifeNavigator.svelte';
  import UpdateBanner from './lib/ui/UpdateBanner.svelte';
  import { runCheck } from './lib/ui/update-store.svelte';
  import {
    getSummary, getDailyLine, getDailyCut, listFindings, onScanDone, onGotoTab, onChatShown, onDailyCutReady,
    onNewFindings, onDiaryReady, onOccasionToday, onDailyLine, onUpdateCheckRequested,
    onLifeVisit, onGuestbookNew, onReuseCelebrated, noticesReady, lifeContentAccess, lifeGoto, lifeGuestbook,
    lifeSetDailyLine, lifeSyncDailyCut, lifeView,
    type Summary,
  } from './lib/api';
  import {
    diaryNotice, findingNotice, guestbookNotice, loadNotices, occasionNotice, pushNotice, saveNotices,
    visitNotice, reuseNotice, type Notice, type NoticeDest,
  } from './lib/notices';
  import {
    addDiaryDate, clearDiaryDates, clearGuestbookSeen, loadUnseen, maxCreatedAt, newGuestbookIds,
    saveUnseen, seedGuestbookSeen,
  } from './lib/unseen';
  import { isTab, resolveTabAfterLifeChange, type Tab } from './lib/ui/tab-routing';
  import { normalizeGroup, type SettingsGroup } from './lib/ui/settings/groups';
  import { diaryVisibility, syncAllSharedDiaries, syncSharedDiary } from './lib/diary-sharing';
  import { resolveHomeLine } from './lib/home-line';
  import { isPermanentLifeError } from './lib/life-sync';

  const TABS: { id: Tab; label: string }[] = [
    { id: 'home', label: '홈' },
    { id: 'diary', label: '다이어리' },
    { id: 'coach', label: '코칭' },
    { id: 'chat', label: '채팅' },
    { id: 'guestbook', label: '방명록' },
    { id: 'settings', label: '설정' },
  ];

  let tab = $state<Tab>('home');
  // 방문 중에 설정 딥링크를 받으면 내 방으로 돌아간 뒤 착지해야 한다.
  // 방 변경 시 홈으로 리셋하는 폴링 로직이 그 의도를 덮어쓰지 않도록 예약해 둔다.
  let pendingTab = $state<Tab | null>(null);
  // 그룹은 지연하지 않는다 — 설정 탭이 안 보이는 동안 바뀌어도 관측되는 효과가 없다.
  let settingsGroup = $state<SettingsGroup>('conn');

  // 방문 컨텍스트 (docs/design/life-visit.md §3) — 남의 방을 보는 동안에는
  // 사적 탭(일기·코칭·채팅)을 숨긴다. 데이터는 원래 로컬 전용이라 유출은 없지만,
  // 남의 방 화면에 내 사적 탭이 보이면 "남의 것"으로 오독된다.
  let visiting = $state(false);
  let lifeOwner = $state('');
  let ownerSeed = $state(''); // 방문 중인 방 주인의 마스코트 시드 (없으면 이름 폴백)
  let ownerAgentId = $state(''), ownerImageVersion = $state('');
  // O1 — 방문 중인 방 주인의 대문(문장·사진 버전). 기존 2초 폴링이 채우므로 추가 조회가 없다.
  let ownerDailyLine = $state(''), ownerCutVersion = $state('');
  let currentLifeId=$state(''),myLifeId=$state(''),meId=$state('');
  let canViewDiary=$state(true);
  let diaryCatchUpStarted = false;
  let gbBootstrapped = false;
  // O1 대문사진 게시 상태 — 폴링(2초)에 얹어 올리므로 재시도 조건을 좁게 잡아야 한다.
  let cutSyncPending = true;    // 올릴 컷이 남았나 (연결 확보·새 컷·서버 전환에 다시 선다)
  let cutSyncBlocked = false;   // 서버가 대문사진을 받지 않는다 → 이 앱 실행 동안 포기
  let cutSyncedLifeId = '';     // 마지막으로 게시한 내 방 (서버·계정 전환 감지)
  const currentDiaryVisibility = () => diaryVisibility(localStorage.getItem('life-diary-visibility'));
  // 창(App) 레벨에서 직접 폴링 — 어느 탭에 있든 방 이동을 감지해 방문 모드로 전환
  // 창 표시 시 강제 폴링용 — $effect 안의 tick 을 밖으로 노출한다(아래 onChatShown).
  let pollNow: () => void = () => {};
  $effect(() => {
    let ticking = false;
    const tick = async () => {
      if (ticking) return;
      ticking = true;
      try {
        const v = await lifeView();
        const lifeChanged = currentLifeId !== '' && currentLifeId !== v.life.life_id;
        visiting = v.me.life_id !== v.me.my_life_id;
        lifeOwner = v.life.owner_name;
        ownerSeed = v.life.owner_mascot_seed || v.life.owner_name;
        ownerAgentId = v.life.owner_agent_id;
        ownerImageVersion = v.life.owner_mascot_image_sha256 || '';
        ownerDailyLine = v.life.owner_daily_line ?? '';
        ownerCutVersion = v.life.owner_daily_cut_sha256 || '';
        currentLifeId=v.life.life_id;myLifeId=v.me.my_life_id;meId=v.me.agent_id;
        if (!diaryCatchUpStarted) {
          diaryCatchUpStarted = true;
          syncAllSharedDiaries(currentDiaryVisibility()).catch(() => { diaryCatchUpStarted = false; });
        }
        // 방명록 뱃지 부트스트랩 — 앱 꺼진 동안 온 글 보완. $effect가 아니라 이 tick에 얹는 이유:
        // 가드가 비반응 값이고 폴링은 같은 id를 재대입하므로 $effect는 실패 후 재시도가 걸리지
        // 않는다(diaryCatchUpStarted와 같은 관용구 — 실패 시 다음 tick 재시도).
        if (!gbBootstrapped && myLifeId && meId) {
          gbBootstrapped = true;
          lifeGuestbook(myLifeId)
            .then(({ entries }) => { observeGuestbook(entries); seedGuestbookIfNeeded(entries); })
            .catch(() => { gbBootstrapped = false; });
        }
        // O1 — 대문사진 게시. 앱이 꺼진 동안의 미게시분을 메우고, 새 컷·서버 전환에도 다시 선다.
        // refresh()에 얹으면 scan:done마다 PNG를 올리므로 여기(폴링)에 둔다.
        // 일시 실패는 다음 tick 재시도, **4xx는 포기** — 구서버·저장소 없는 서버에 붙어 있으면
        // 거부된 PNG를 2초마다 영구 재전송하게 된다(maybe_reply_guestbook의 비활성 래치 선례).
        if (myLifeId && cutSyncedLifeId && cutSyncedLifeId !== myLifeId) {
          cutSyncPending = true;
          cutSyncBlocked = false; // 다른 서버는 지원할 수 있다
        }
        if (cutSyncPending && !cutSyncBlocked && myLifeId) {
          cutSyncPending = false;
          const target = myLifeId;
          lifeSyncDailyCut()
            .then(() => { cutSyncedLifeId = target; })
            .catch((e) => {
              if (isPermanentLifeError(e)) cutSyncBlocked = true;
              else cutSyncPending = true;
            });
        }
        const next = resolveTabAfterLifeChange(lifeChanged, pendingTab, tab);
        tab = next.tab;
        pendingTab = next.pendingTab;
        canViewDiary = !visiting || (await lifeContentAccess(v.life.life_id)).features.diary.can_view;
        // 설정 착지를 예약한 상태(내 방으로 돌아오는 중)면 홈으로 밀지 않는다 — 다음 tick이 착지시킨다.
        if (!pendingTab && visiting && (!['home','diary','guestbook'].includes(tab) || (tab === 'diary' && !canViewDiary))) tab = 'home';
      } catch {
        diaryCatchUpStarted = false;
        visiting = false;
        lifeOwner = '';
        ownerSeed = '';
        ownerDailyLine = '';
        ownerCutVersion = '';
      } finally {
        ticking = false;
      }
    };
    tick();
    // 정지됐다 깨어난 tick 이 아직 안 끝났으면 가드에 막히므로, 강제 폴링은 가드를 먼저 푼다.
    // 두 tick 이 겹쳐도 같은 값을 재대입할 뿐이라 무해하다.
    pollNow = () => { ticking = false; tick(); };
    const t = setInterval(tick, 2000);
    // 창 show/hide 는 여기 안 걸린다 — 숨겨져도 visibilityState 가 'visible' 로 남는 것을 실측했다.
    // 그 경로는 onChatShown(네이티브 emit)이 담당한다. 이건 그 외 상황용으로만 남긴다.
    const onVis = () => { if (!document.hidden) tick(); };
    document.addEventListener('visibilitychange', onVis);
    return () => { clearInterval(t); document.removeEventListener('visibilitychange', onVis); };
  });
  const visibleTabs = $derived(visiting ? TABS.filter((t) => ['home','guestbook'].includes(t.id) || (t.id === 'diary' && canViewDiary)) : TABS);
  let summary = $state<Summary | null>(null);
  let dailyLine = $state<string | null>(null);
  // H2 — 컷 캡션. 있으면 한마디 카드에 캡션을 우선 표시 (그림을 아는 텍스트가 이김, 컷 밑 별도 텍스트 없음)
  let cutCaption = $state<string | null>(null);
  // O1 — 카드에 그릴 문장. 방문 중이면 주인 게시분, 내 방이면 캡션 우선.
  const homeLine = $derived(resolveHomeLine({ visiting, ownerLine: ownerDailyLine, cutCaption, dailyLine }));
  let activeCount = $state(0);
  let coachFocus = $state<string | null>(null);
  let diaryFocus = $state<string | null>(null);
  let notices = $state<Notice[]>(loadNotices());
  // N1 탭 뱃지 — 다이어리는 날짜 set(영속), 방명록은 entry_id set(세션) + lastSeen(영속)
  let unseen = $state(loadUnseen());
  let gbUnseenIds = $state(new Set<string>());
  // 관측한 타인 글의 최신 서버 시각 — 클리어 시 워터마크로 쓴다(클라이언트 시계 배제, 단조 증가)
  let gbMaxSeenAt = $state<string | null>(null);
  const unseenDiary = $derived(unseen.diaryDates.length);
  const unseenGuestbook = $derived(gbUnseenIds.size);

  // 부트스트랩(서버 조회)과 guestbook:new 이벤트의 공통 반영 — entry_id dedup이라 중복 카운트 없음
  function observeGuestbook(rows: { entry_id: string; author_agent_id: string; created_at: string }[]) {
    const fresh = newGuestbookIds(rows, meId, unseen.guestbookLastSeen);
    if (fresh.length) gbUnseenIds = new Set([...gbUnseenIds, ...fresh]);
    const at = maxCreatedAt(rows, meId);
    if (at && (!gbMaxSeenAt || at > gbMaxSeenAt)) gbMaxSeenAt = at;
  }

  // 최초 시드는 **부트스트랩 경로 전용**이다 — 이벤트에서 시드하면 그 글 자신의 시각이 워터마크가 돼
  // 방금 온 글을 읽음 처리한다(빈 부트스트랩 뒤 첫 글이 사라지던 경로).
  function seedGuestbookIfNeeded(rows: { author_agent_id: string; created_at: string }[]) {
    const next = seedGuestbookSeen(unseen, rows, meId);
    if (next === unseen) return;
    unseen = next;
    saveUnseen(unseen);
  }

  // O1 — 대문 게시 게이트: 로컬 조회가 **성공한** 사이클에만 열린다.
  // getDailyLine은 실패를 던지고 호출부가 null로 뭉개므로, 성공 여부를 따로 들고 있지 않으면
  // 일시 장애가 "빈 문자열 게시" = 서버의 정상 문장 삭제로 이어진다.
  let homeReadOk = $state(false);

  async function refresh() {
    summary = await getSummary().catch(() => null);
    activeCount = (await listFindings(false).catch(() => [])).length;
    // 조회 **전에** 게이트를 닫는다 — 두 조회 사이의 await에서 effect가 flush되면 아직 갱신되지
    // 않은 중간값(캡션만 비운 빈 문자열 등)이 게시돼 서버의 정상 문장을 지운다.
    homeReadOk = false;
    let readOk = true;
    dailyLine = await getDailyLine().catch(() => { readOk = false; return null; });
    cutCaption = (await getDailyCut())?.caption || null;
    homeReadOk = readOk;
  }
  refresh();
  onScanDone(() => refresh());
  // 창이 숨겨진 동안 WebView2가 웹뷰를 정지시켜 JS가 20~30초씩 멈춘다(실측). 그동안 폴링도
  // refresh 도 서지 않아 창을 열면 낡은 화면이 남는다. document.visibilityState 는 숨겨져도
  // 'visible' 이라 visibilitychange 로는 이 순간을 잡을 수 없어, 네이티브가 보내는 신호를 쓴다.
  onChatShown(() => { refresh(); pollNow(); });

  // O1 대문 게시 — 내 화면에 걸린 문장을 그대로 올린다(빈 문자열 = 지움).
  // visiting을 절대 참조하지 않는다 — 참조하면 남의 방에 들어간 순간 내 대문이 지워진다.
  // myLifeId를 의존에 넣는 이유: life 연결 확보(미연결 부팅 뒤 나중에 연결)와 서버·계정 전환을
  // 재게시 신호로 쓴다. 같은 값 재대입은 무효화되지 않으므로 2초 폴링이 게시를 반복하지 않는다.
  const publishLine = $derived(cutCaption || dailyLine || '');
  $effect(() => {
    if (!homeReadOk || !myLifeId) return;
    lifeSetDailyLine(publishLine).catch(() => {});
  });

  // 자동 업데이트: 시작 시 조용히 1회 체크 + 트레이 "업데이트 확인" 수동 트리거
  runCheck(false);
  $effect(() => {
    const un = onUpdateCheckRequested(() => runCheck(true));
    return () => { un.then((u) => u()); };
  });
  onGotoTab(({ tab: t, target }) => {
    if (!isTab(t) || t === 'guestbook') return; // 방명록은 방 문맥이 필요해 딥링크 대상이 아니다
    if (t === 'settings') {
      settingsGroup = normalizeGroup(target);
      // 남의 방 방문 중이면 내 방으로 돌아간 뒤 착지한다 (내 설정이 방 주인 것으로 오독되지 않게)
      if (visiting) { pendingTab = 'settings'; lifeGoto(myLifeId).catch(() => { pendingTab = null; }); }
      else tab = 'settings';
      return;
    }
    if (target && t === 'coach') gotoCoach(target);
    else if (target && t === 'diary') gotoDiary(target);
    else tab = t;
  });

  // 알림 히스토리 기록 (스펙 §2 — 창이 숨김이어도 수신됨). 문구·target 조합은 notices.ts 헬퍼.
  function record(n: Notice) {
    notices = pushNotice(notices, n);
    saveNotices(notices);
  }
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && record(findingNotice(rows, new Date().toISOString()))),
      onDiaryReady((date) => {
        record(diaryNotice(date, new Date().toISOString()));
        unseen = addDiaryDate(unseen, date);
        saveUnseen(unseen);
        syncSharedDiary(date, currentDiaryVisibility()).catch(() => { diaryCatchUpStarted = false; });
      }),
      onOccasionToday((labels) => labels.length && record(occasionNotice(labels, new Date().toISOString()))),
      onLifeVisit((rows) => rows.length && record(visitNotice(rows, new Date().toISOString()))),
      onGuestbookNew((rows) => {
        if (!rows.length) return;
        record(guestbookNotice(rows, new Date().toISOString()));
        observeGuestbook(rows);
      }),
      // 인정 루프 — 내 지식이 재사용된 순간을 개인 칭찬으로 (스펙 PR #81)
      onReuseCelebrated((rows) => rows.length && record(reuseNotice(rows, new Date().toISOString()))),
      onDailyLine((text) => { dailyLine = text; }),
      onDailyCutReady(() => {
        getDailyCut().then((c) => (cutCaption = c?.caption || null));
        // O1 — 게시는 폴링에 맡긴다(직접 호출하면 실패해도 재시도 경로가 없다)
        cutSyncPending = true;
      }),
    ];
    // 등록이 전부 끝난 뒤에 신고 — listen()은 비동기라 배열 생성만으로는 아직 수신 준비가 아니다.
    // 이 신고 전까지 백엔드는 인바운드 폴링을 보류한다(수신자 없는 emit = 소식 영구 유실).
    Promise.all(subs).then(() => noticesReady()).catch(() => {});
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  // 탭 확인 시 뱃지 클리어 — 방명록은 내 방 문맥일 때만 (남의 방 방명록을 봐도 내 소식은 그대로)
  $effect(() => {
    if (tab === 'diary' && !visiting && unseen.diaryDates.length) {
      unseen = clearDiaryDates(unseen);
      saveUnseen(unseen);
    }
    if (tab === 'guestbook' && !visiting && currentLifeId === myLifeId && gbUnseenIds.size) {
      // 워터마크는 관측한 서버 시각으로 — 클라이언트 시계를 쓰면 오차만큼 새 글이 숨거나
      // 본 글이 되살아난다 (Codex 리뷰 P2). 관측값이 없으면 워터마크는 그대로 두고 뱃지만 정리.
      if (gbMaxSeenAt) {
        unseen = clearGuestbookSeen(unseen, gbMaxSeenAt);
        saveUnseen(unseen);
      }
      gbUnseenIds = new Set();
    }
  });

  function gotoCoach(dedupKey: string) {
    coachFocus = dedupKey;
    tab = 'coach';
  }

  function gotoDiary(date: string) {
    diaryFocus = date;
    tab = 'diary';
  }

  // NoticeLog 클릭 착지 — dest.tab에 따라 코칭 카드/다이어리 날짜/방명록 탭으로
  function gotoDest(dest: NoticeDest) {
    if (dest.tab === 'coach') gotoCoach(dest.target);
    else if (dest.tab === 'guestbook') gotoGuestbook();
    else gotoDiary(dest.target);
  }

  // 방명록 알림은 내 방 문맥으로 착지 — 방문 중이면 내 방으로 돌아간 뒤 (설정 딥링크 선례)
  function gotoGuestbook() {
    if (visiting) { pendingTab = 'guestbook'; lifeGoto(myLifeId).catch(() => { pendingTab = null; }); }
    else tab = 'guestbook';
  }
</script>

<div class="wall">
  <div class="homepy">
    <UpdateBanner />
    <header class="titlebar">
      <!-- 헤더 = 지금 보는 방의 주인. 자기 방이면 hub 등록 이름, hub 미연결이면 로컬 계정명 -->
      <h1>
        <span>{lifeOwner || (summary?.user_name ?? '주인')}님의 <span class="mh">미니홈피</span></span>
        {#if visiting}<span class="visit-chip">방문 중</span>{/if}
      </h1>
      <!-- 카운터도 내 로컬 세션 수 — 방문 중엔 숨김 (주인 수치로 오독 방지) -->
      {#if !visiting}
        <div class="counter">
          TODAY <b>{summary?.session_count ?? '–'}</b> · TOTAL <b>{summary?.total_sessions ?? '–'}</b>
        </div>
      {/if}
    </header>
    <div class="body">
      <aside class="profile">
        <!-- 프로필 = 지금 보는 미니홈피의 주인. 방문 중이면 그 방 주인의 대문사진·로봇 -->
        <RobotPortrait seed={visiting ? ownerSeed : null} agentId={visiting ? ownerAgentId : null}
          imageVersion={visiting ? ownerImageVersion : null}
          cutAgentId={visiting ? ownerAgentId : null} cutVersion={visiting ? ownerCutVersion : null} />
        {#if homeLine}
          <div class="daily">
            <span class="cap">💬 오늘의 한마디</span>
            <span class="daily-line">{homeLine}</span>
          </div>
        {/if}
        <!-- 프로필 하단 도크: a-lens 링크 → 미니홈피 이동 순서로 붙인다.
             두 컴포넌트가 각자 margin-top:auto를 쓰면 서로 밀어내므로 여기서 한 번만 띄운다. -->
        <div class="bottom-dock">
          <LensLink />
          <LifeNavigator {myLifeId} currentLifeId={currentLifeId} />
        </div>
      </aside>
      <!-- 다이어리는 좌우(달력·활동 / 일기)가 각자 스크롤하므로 바깥 스크롤을 끈다 —
           켜두면 스크롤 컨테이너가 둘로 겹쳐 어느 쪽이 움직이는지 알 수 없고,
           좌우 높이가 서로 무관하게 늘어나 비대칭이 된다. -->
      <main class="content" class:fixed={tab === 'diary'}>
        {#if tab === 'home'}
          <HomeTab {summary} {visiting} onGotoCoach={gotoCoach} onGotoNotice={gotoDest} />
        {:else if tab === 'coach'}
          <CoachTab focusKey={coachFocus} onChanged={refresh} />
        {:else if tab === 'diary'}
          <DiaryTab focusDate={diaryFocus} {visiting} lifeId={currentLifeId} />
        {:else if tab === 'chat'}
          <ChatTab />
        {:else if tab === 'guestbook'}
          <GuestbookTab lifeId={currentLifeId} {meId} {myLifeId} isOwner={currentLifeId===myLifeId}/>
        {:else}
          <SettingsTab group={settingsGroup} />
        {/if}
      </main>
      <nav class="tabs">
        {#each visibleTabs as t (t.id)}
          <button class:active={tab === t.id} onclick={() => (tab = t.id)}>
            <span class="label">{t.label}</span>
            {#if t.id === 'coach' && activeCount > 0}<span class="badge">{activeCount}</span>{/if}
            {#if t.id === 'diary' && unseenDiary > 0}<span class="badge">{unseenDiary}</span>{/if}
            {#if t.id === 'guestbook' && unseenGuestbook > 0}<span class="badge">{unseenGuestbook}</span>{/if}
          </button>
        {/each}
      </nav>
    </div>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; height: 100%; }
  :global(body) {
    background: var(--bg-grad);
    color: var(--ink);
    font-family: "Pretendard", "Apple SD Gothic Neo", "Malgun Gothic", system-ui, sans-serif;
    font-size: 14px;
  }
  /* 우측 패딩 = 좌측과 같은 여백(18) + 프레임 밖으로 걸치는 세로 탭(right:-30px) */
  .wall { height: 100vh; padding: 18px 48px 18px 18px; box-sizing: border-box; }
  .homepy {
    height: 100%; display: flex; flex-direction: column;
    background: var(--frame-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-soft);
  }
  .titlebar {
    display: flex; justify-content: space-between; align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid var(--pastel-lav);
  }
  .titlebar h1 { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 16px; font-weight: 700; }
  .titlebar h1 .mh { color: var(--accent-strong); }
  .visit-chip {
    flex: none;
    border-radius: 999px;
    padding: 2px 7px;
    background: var(--coral);
    color: var(--coral-ink);
    font-size: 10px;
    font-weight: 700;
    line-height: 1.4;
  }
  .counter { font-size: 12px; color: var(--ink-soft); }
  .counter b { color: var(--accent-strong); }
  .body { flex: 1; display: flex; min-height: 0; position: relative; }
  .profile {
    width: 168px; padding: 16px 14px;
    border-right: 1px solid var(--pastel-lav);
    display: flex; flex-direction: column; gap: 12px;
  }
  /* 프로필 하단 도크 — a-lens 링크 + 미니홈피 이동을 아래에 붙여 한 묶음으로 둔다 */
  .bottom-dock {
    margin-top: auto;
    display: flex; flex-direction: column; gap: 8px;
  }
  .bottom-dock :global(.life-navigator) { margin-top: 0; }
  /* 한마디 카드 — LLM 오늘의 한마디를 4줄로 접는(…) 표시 전용. 좁은 프로필 칸 가독성 */
  .daily {
    display: flex; flex-direction: column; gap: 5px;
    background: var(--panel2); border: 1px solid var(--line); border-left: 2px solid var(--accent);
    border-radius: var(--radius-s); padding: 8px 10px;
  }
  .daily .cap { font-size: 10px; color: var(--ink-soft); letter-spacing: 0.3px; }
  .daily-line {
    margin: 0; font-size: 12px; color: var(--ink); line-height: 1.55;
    overflow-wrap: break-word; word-break: keep-all;
    display: -webkit-box; -webkit-line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden;
  }
  /* margin-right: 스크롤바를 프레임 가장자리(우측 세로 탭이 걸치는 곳)에서 안쪽으로 밀어냄 */
  .content { flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column; margin-right: 10px; }
  .content.fixed { overflow: hidden; }
  .tabs {
    position: absolute; right: -30px; top: 24px;
    display: flex; flex-direction: column; gap: 6px;
  }
  .tabs button {
    writing-mode: vertical-rl;
    border: 1px solid var(--line); border-left: none; cursor: pointer; font: inherit; font-size: 12px;
    padding: 13px 7px;
    background: var(--panel2); color: var(--ink-soft);
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
    box-shadow: var(--shadow-soft);
    display: flex; align-items: center; gap: 5px;
  }
  .tabs button.active { background: var(--accent); font-weight: 700; color: var(--accent-ink); border-color: transparent; }
  .badge {
    writing-mode: horizontal-tb;
    background: var(--coral); color: var(--coral-ink); font-weight: 700;
    border-radius: 999px; font-size: 10px; padding: 1px 5px;
  }
</style>
