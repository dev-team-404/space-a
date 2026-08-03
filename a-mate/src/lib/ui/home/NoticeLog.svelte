<script lang="ts">
  import { noticeDest, noticeStamp, type Notice, type NoticeDest } from '../../notices';
  let { notices, onGoto }: { notices: Notice[]; onGoto: (dest: NoticeDest) => void } = $props();
  const ICON: Record<Notice['kind'], string> = { finding: '💡', diary: '📓', occasion: '🎉', visit: '👋', guestbook: '✍️', reuse: '🌱', announcement: '📣' };
  // 포맷 로직은 notices.ts의 순수 함수에 있다 — 이 저장소엔 컴포넌트 테스트 라이브러리가 없다.
  // 상주 앱이라 홈 탭이 자정을 넘겨 떠 있을 수 있다. `now`가 어제로 굳으면 판정이 뒤집힌다 —
  // 어제 알림이 계속 HH:MM으로 보이고, 자정 직후 새 알림은 MM-DD가 된다. 날이 바뀔 때만 갱신한다.
  let now = $state(new Date());
  $effect(() => {
    const id = setInterval(() => {
      const d = new Date();
      if (d.toDateString() !== now.toDateString()) now = d;
    }, 60_000);
    return () => clearInterval(id);
  });
</script>

<div class="widget">
  <h3>최근 알림</h3>
  {#if notices.length === 0}
    <p class="empty">아직 알림이 없어요</p>
  {:else}
    <ul>
      {#each notices.slice(0, 6) as n (n.ts + n.text)}
        {@const dest = noticeDest(n)}
        {@const stamp = noticeStamp(n.ts, now)}
        <li>
          <span>{ICON[n.kind]}</span>
          {#if dest}
            <button class="text" onclick={() => onGoto(dest)}>{n.text}</button>
          {:else}
            <span class="text">{n.text}</span>
          {/if}
          <time datetime={n.ts} title={stamp.full}>{stamp.label}</time>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; font-size: 12px; }
  li { display: flex; gap: 6px; align-items: baseline; }
  .text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button.text {
    border: none; background: none; font: inherit; color: inherit;
    padding: 0; cursor: pointer; text-align: left;
  }
  button.text:hover { color: var(--accent-strong); text-decoration: underline; }
  time { color: var(--ink-soft); font-size: 10px; }
</style>
