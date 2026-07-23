import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import type { UpdatePhase } from './update-banner-helpers';

// 모듈 레벨 룬 스토어 (chat-store.svelte.ts 컨벤션)
export const updateStore = $state<{ phase: UpdatePhase }>({ phase: { kind: 'idle' } });

let pending: Update | null = null;

/** 업데이트 체크. manual=false는 시작 시 자동(조용히), true는 트레이 수동. */
export async function runCheck(manual: boolean): Promise<void> {
  updateStore.phase = { kind: 'checking', manual };
  try {
    const update = await check();
    pending = update;
    if (update) {
      updateStore.phase = { kind: 'available', version: update.version, notes: update.body ?? null };
    } else {
      updateStore.phase = { kind: 'uptodate', manual };
    }
  } catch (e) {
    updateStore.phase = { kind: 'error', manual, message: String(e) };
  }
}

/** 다운로드+설치 후 재시작. runCheck로 확보한 pending 핸들 사용. */
export async function installUpdate(): Promise<void> {
  if (!pending) return;
  let downloaded = 0;
  let total: number | null = null;
  updateStore.phase = { kind: 'downloading', downloaded: 0, total: null };
  await pending.downloadAndInstall((ev) => {
    switch (ev.event) {
      case 'Started':
        total = ev.data.contentLength ?? null;
        updateStore.phase = { kind: 'downloading', downloaded: 0, total };
        break;
      case 'Progress':
        downloaded += ev.data.chunkLength;
        updateStore.phase = { kind: 'downloading', downloaded, total };
        break;
      case 'Finished':
        break;
    }
  });
  await relaunch();
}

export function dismiss(): void {
  updateStore.phase = { kind: 'idle' };
}
