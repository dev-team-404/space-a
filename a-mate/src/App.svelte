<script lang="ts">
  import './lib/theme.css';
  import HomeTab from './lib/ui/HomeTab.svelte';
  import CoachTab from './lib/ui/CoachTab.svelte';
  import DiaryTab from './lib/ui/DiaryTab.svelte';
  import ChatTab from './lib/ui/ChatTab.svelte';
  import GuestbookTab from './lib/ui/GuestbookTab.svelte';
  import SettingsTab from './lib/ui/settings/SettingsTab.svelte';
  import RobotPortrait from './lib/ui/RobotPortrait.svelte';
  import LifeNavigator from './lib/ui/LifeNavigator.svelte';
  import UpdateBanner from './lib/ui/UpdateBanner.svelte';
  import { runCheck } from './lib/ui/update-store.svelte';
  import {
    getSummary, getDailyLine, listFindings, onScanDone, onGotoTab,
    onNewFindings, onDiaryReady, onOccasionToday, onDailyLine, onUpdateCheckRequested,
    lifeContentAccess, lifeGoto, lifeView, type Summary,
  } from './lib/api';
  import {
    diaryNotice, findingNotice, loadNotices, occasionNotice, pushNotice, saveNotices,
    type Notice, type NoticeDest,
  } from './lib/notices';
  import { isTab, resolveTabAfterLifeChange, type Tab } from './lib/ui/tab-routing';
  import { normalizeGroup, type SettingsGroup } from './lib/ui/settings/groups';

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
  let currentLifeId=$state(''),myLifeId=$state(''),meId=$state('');
  let canViewDiary=$state(true);
  // 창(App) 레벨에서 직접 폴링 — 어느 탭에 있든 방 이동을 감지해 방문 모드로 전환
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
        currentLifeId=v.life.life_id;myLifeId=v.me.my_life_id;meId=v.me.agent_id;
        const next = resolveTabAfterLifeChange(lifeChanged, pendingTab, tab);
        tab = next.tab;
        pendingTab = next.pendingTab;
        canViewDiary = !visiting || (await lifeContentAccess(v.life.life_id)).features.diary.can_view;
        // 설정 착지를 예약한 상태(내 방으로 돌아오는 중)면 홈으로 밀지 않는다 — 다음 tick이 착지시킨다.
        if (!pendingTab && visiting && (!['home','diary','guestbook'].includes(tab) || (tab === 'diary' && !canViewDiary))) tab = 'home';
      } catch {
        visiting = false;
        lifeOwner = '';
        ownerSeed = '';
      } finally {
        ticking = false;
      }
    };
    tick();
    const t = setInterval(tick, 2000);
    const onVis = () => { if (!document.hidden) tick(); };
    document.addEventListener('visibilitychange', onVis);
    return () => { clearInterval(t); document.removeEventListener('visibilitychange', onVis); };
  });
  const visibleTabs = $derived(visiting ? TABS.filter((t) => ['home','guestbook'].includes(t.id) || (t.id === 'diary' && canViewDiary)) : TABS);
  let summary = $state<Summary | null>(null);
  let dailyLine = $state<string | null>(null);
  let activeCount = $state(0);
  let coachFocus = $state<string | null>(null);
  let diaryFocus = $state<string | null>(null);
  let notices = $state<Notice[]>(loadNotices());

  async function refresh() {
    summary = await getSummary().catch(() => null);
    activeCount = (await listFindings(false).catch(() => [])).length;
    dailyLine = await getDailyLine().catch(() => null);
  }
  refresh();
  onScanDone(() => refresh());

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
      onDiaryReady((date) => record(diaryNotice(date, new Date().toISOString()))),
      onOccasionToday((labels) => labels.length && record(occasionNotice(labels, new Date().toISOString()))),
      onDailyLine((text) => { dailyLine = text; }),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  function gotoCoach(dedupKey: string) {
    coachFocus = dedupKey;
    tab = 'coach';
  }

  function gotoDiary(date: string) {
    diaryFocus = date;
    tab = 'diary';
  }

  // NoticeLog 클릭 착지 — dest.tab에 따라 코칭 카드/다이어리 날짜로
  function gotoDest(dest: NoticeDest) {
    if (dest.tab === 'coach') gotoCoach(dest.target);
    else gotoDiary(dest.target);
  }
</script>

<div class="wall">
  <div class="homepy">
    <UpdateBanner />
    <header class="titlebar">
      <!-- 헤더 = 지금 보는 방의 주인. 자기 방이면 hub 등록 이름, hub 미연결이면 로컬 계정명 -->
      <h1>{lifeOwner || (summary?.user_name ?? '주인')}님의 <span class="mh">미니홈피</span></h1>
      <!-- 카운터도 내 로컬 세션 수 — 방문 중엔 숨김 (주인 수치로 오독 방지) -->
      {#if !visiting}
        <div class="counter">
          TODAY <b>{summary?.session_count ?? '–'}</b> · TOTAL <b>{summary?.total_sessions ?? '–'}</b>
        </div>
      {/if}
    </header>
    <div class="body">
      <aside class="profile">
        <!-- 프로필 = 지금 보는 미니홈피의 주인. 방문 중이면 그 방 주인의 로봇 -->
        <RobotPortrait seed={visiting ? ownerSeed : null} agentId={visiting ? ownerAgentId : null} imageVersion={visiting ? ownerImageVersion : null} />
        {#if dailyLine && !visiting}
          <div class="daily">
            <span class="cap">💬 오늘의 한마디</span>
            <span class="daily-line">{dailyLine}</span>
          </div>
        {/if}
        <LifeNavigator {myLifeId} currentLifeId={currentLifeId} />
      </aside>
      <main class="content">
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
  .wall { height: 100vh; padding: 18px 34px 18px 18px; box-sizing: border-box; }
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
  .titlebar h1 { margin: 0; font-size: 16px; font-weight: 700; }
  .titlebar h1 .mh { color: var(--accent-strong); }
  .counter { font-size: 12px; color: var(--ink-soft); }
  .counter b { color: var(--accent-strong); }
  .body { flex: 1; display: flex; min-height: 0; position: relative; }
  .profile {
    width: 168px; padding: 16px 14px;
    border-right: 1px solid var(--pastel-lav);
    display: flex; flex-direction: column; gap: 12px;
  }
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
