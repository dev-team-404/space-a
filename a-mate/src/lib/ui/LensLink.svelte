<script lang="ts">
  // A-Lens 링크 — 우리 팀 방을 관전 웹에서 연다. 주소 조립 규칙은 lib/lens.ts 한 곳에 둔다
  // (설정 창도 같은 모듈을 쓴다). 설정이 없어도 팀 기본 주소로 동작하고, 'off'면 숨는다.
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { getSettings } from '../api';
  import { lensRoomUrl, lensSpaceLabel } from '../lens';

  let url = $state('');
  let space = $state('');

  async function load() {
    try {
      const settings = await getSettings();
      url = lensRoomUrl(settings);
      space = lensSpaceLabel(settings);
    } catch (e) {
      console.error('A-Lens 링크 설정을 읽지 못했습니다:', e);
    }
  }
  load();

  async function open() {
    if (!url) return;
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }
</script>

{#if url}
  <button class="lens-link" onclick={open} title={url}>
    <span class="ico">🔭</span>
    <span class="txt">
      <b>A-Lens에서 보기</b>
      <small>{space} 팀 방</small>
    </span>
    <span class="go">↗</span>
  </button>
{/if}

<style>
  /* 미니홈피 이동(LifeNavigator) 바로 위 한 줄 — 같은 폭·같은 리듬으로 붙는다 */
  .lens-link {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    background: var(--panel2);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .lens-link:hover { border-color: var(--accent); }
  .ico { font-size: 15px; flex: 0 0 auto; }
  .txt { display: flex; flex-direction: column; line-height: 1.25; min-width: 0; }
  .txt b { font-size: 12px; }
  .txt small { font-size: 10px; color: var(--ink-soft); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .go { margin-left: auto; color: var(--ink-soft); font-size: 12px; }
</style>
