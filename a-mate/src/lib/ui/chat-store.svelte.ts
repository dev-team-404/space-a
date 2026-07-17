// 채팅 이력 — 창 수명 동안 프론트 메모리에만 유지(탭 전환에도 보존, 영속화 없음 — 스펙 §5)
import type { ChatMessage } from '../api';

export const chatState = $state({ messages: [] as ChatMessage[] });
