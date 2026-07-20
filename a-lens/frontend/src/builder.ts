// 방 만들기 마법사 — DOM 오버레이(선택 UI) + PixiJS 라이브 프리뷰.
// 선택 결과는 LifeConfig 데이터일 뿐이며, 렌더링은 전적으로 renderer.ts가 담당한다.

import { Application } from 'pixi.js'
import type { LobbyFloor } from './api'
import { DESK_OPTIONS, LIFE_OPTIONS, resolveLifeId } from './life/catalog'
import { buildLifeScene, PREVIEW_DATA } from './life/renderer'
import type { DeskChoice, LifeConfig, LifePresetId, VariantOption } from './life/types'
import { getLife } from './store'

export type BuilderOptions = {
  floors: LobbyFloor[]
  /** 수정 모드 — 기존 방 설정에서 시작 */
  initial?: LifeConfig
  onSaved: (config: LifeConfig) => void
}

// 프리뷰 Pixi 앱은 싱글턴 — 열 때마다 destroy/재생성하면 두 번째부터 공유 텍스처
// 상태가 깨져 본 씬 렌더링이 실패한다 (실측). canvas만 모달에 다시 붙인다.
let previewApp: Application | null = null
async function ensurePreviewApp(): Promise<Application> {
  if (!previewApp) {
    previewApp = new Application()
    await previewApp.init({ width: 460, height: 330, background: '#0d1220', antialias: true })
  }
  return previewApp
}

const DEFAULTS: Omit<LifeConfig, 'space_id' | 'space_name' | 'created_at'> = {
  life: 'r1', // 방 배경 프리셋
  desk: 'd1',
  desks: 'auto',
}

export async function openBuilder(opts: BuilderOptions): Promise<void> {
  const host = document.getElementById('builder')
  if (!host) throw new Error('#builder 엘리먼트 없음')

  const state: LifeConfig = opts.initial
    ? {
        ...opts.initial,
        // 구버전 저장분 → 방 배경 프리셋으로 정규화
        life: resolveLifeId(opts.initial.life),
      }
    : {
        space_id: opts.floors[0]?.space_id ?? '',
        space_name: opts.floors[0]?.name ?? '',
        ...DEFAULTS,
        created_at: new Date().toISOString(),
      }

  host.innerHTML = `
    <div class="builder-card">
      <header class="builder-head">
        <h2>${opts.initial ? '방 꾸미기 수정' : '방 만들기'}</h2>
        <button class="icon-btn" data-act="close" title="닫기">✕</button>
      </header>
      <div class="builder-body">
        <div class="builder-form">
          <label class="field-label">스페이스 (a-hub 방 이름)</label>
          <select id="b-space" ${opts.initial ? 'disabled' : ''}></select>
          <div id="b-space-note" class="field-note"></div>
          <div id="b-groups"></div>
          <label class="field-label">책상 수</label>
          <div id="b-desks" class="chip-row"></div>
        </div>
        <div class="builder-preview">
          <div class="field-label">미리보기</div>
          <div id="b-preview-canvas"></div>
          <button class="primary-btn" data-act="save">${opts.initial ? '저장' : '방 만들기 완료'}</button>
        </div>
      </div>
    </div>`
  host.hidden = false

  // ── 스페이스 선택 ──
  const select = host.querySelector<HTMLSelectElement>('#b-space')!
  const note = host.querySelector<HTMLElement>('#b-space-note')!
  for (const f of opts.floors) {
    const o = document.createElement('option')
    o.value = f.space_id
    o.textContent = f.name + (getLife(f.space_id) ? ' (이미 방 있음 — 덮어씀)' : '')
    if (f.space_id === state.space_id) o.selected = true
    select.appendChild(o)
  }
  const syncNote = () => {
    note.textContent = getLife(state.space_id) && !opts.initial ? '⚠️ 이 스페이스의 기존 방을 덮어씁니다.' : ''
  }
  select.addEventListener('change', () => {
    state.space_id = select.value
    state.space_name = opts.floors.find((f) => f.space_id === select.value)?.name ?? select.value
    syncNote()
  })
  syncNote()

  // ── variant 선택 그룹 ──
  const groups = host.querySelector<HTMLElement>('#b-groups')!
  function addGroup<Id extends string>(
    title: string,
    options: VariantOption<Id>[],
    get: () => Id,
    set: (id: Id) => void,
  ) {
    const label = document.createElement('label')
    label.className = 'field-label'
    label.textContent = title
    const row = document.createElement('div')
    row.className = 'swatch-row'
    for (const opt of options) {
      const btn = document.createElement('button')
      btn.className = 'swatch'
      btn.innerHTML = opt.thumb
        ? `<img class="swatch-thumb" src="${opt.thumb}" alt="" />${opt.label}`
        : `<span class="swatch-dot" style="background:${opt.swatch}"></span>${opt.label}`
      btn.dataset.id = opt.id
      btn.addEventListener('click', () => {
        set(opt.id)
        row.querySelectorAll('.swatch').forEach((b) => b.classList.toggle('on', (b as HTMLElement).dataset.id === opt.id))
        renderPreview()
      })
      row.appendChild(btn)
    }
    groups.append(label, row)
    row.querySelectorAll('.swatch').forEach((b) => b.classList.toggle('on', (b as HTMLElement).dataset.id === get()))
  }
  // 방 배경 프리셋 5종 중 하나를 고른다 (벽·바닥·책장·칠판이 다 그려진 통짜 이미지).
  addGroup('방 배경', LIFE_OPTIONS, () => resolveLifeId(state.life), (id: LifePresetId) => (state.life = id))
  // 그 위에 얹을 책상 종류만 선택
  addGroup('책상', DESK_OPTIONS, () => state.desk, (id: DeskChoice) => (state.desk = id))

  // ── 책상 수 (자동 = 에이전트 수 따라감) ──
  const desksRow = host.querySelector<HTMLElement>('#b-desks')!
  const deskChoices: ('auto' | number)[] = ['auto', 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
  for (const choice of deskChoices) {
    const chip = document.createElement('button')
    chip.className = 'chip'
    chip.textContent = choice === 'auto' ? '자동 (에이전트 수)' : String(choice)
    chip.dataset.desks = String(choice)
    chip.addEventListener('click', () => {
      state.desks = choice
      desksRow.querySelectorAll('.chip').forEach((b) => b.classList.toggle('on', (b as HTMLElement).dataset.desks === String(choice)))
      renderPreview()
    })
    desksRow.appendChild(chip)
  }
  const currentDesks = state.desks ?? 'auto'
  desksRow.querySelectorAll('.chip').forEach((b) => b.classList.toggle('on', (b as HTMLElement).dataset.desks === String(currentDesks)))

  // 장식은 선택 없음 — 방을 데이터(에이전트·이슈·지식)에 집중시킨다

  // ── 라이브 프리뷰 (싱글턴 Pixi Application 재사용) ──
  const previewHost = host.querySelector<HTMLElement>('#b-preview-canvas')!
  const preview = await ensurePreviewApp()
  previewHost.appendChild(preview.canvas)

  function renderPreview() {
    preview.stage.removeChildren().forEach((c) => c.destroy({ children: true }))
    const scene = buildLifeScene(state, PREVIEW_DATA)
    const b = scene.getLocalBounds()
    const s = Math.min(preview.screen.width / (b.width + 40), preview.screen.height / (b.height + 40))
    scene.scale.set(s)
    scene.position.set(
      (preview.screen.width - b.width * s) / 2 - b.x * s,
      (preview.screen.height - b.height * s) / 2 - b.y * s,
    )
    preview.stage.addChild(scene)
  }
  renderPreview()

  // ── 닫기 / 저장 ──
  const close = () => {
    preview.stage.removeChildren().forEach((c) => c.destroy({ children: true }))
    host.hidden = true
    host.innerHTML = '' // canvas는 DOM에서만 떨어지고 앱은 재사용
  }
  host.querySelector('[data-act="close"]')!.addEventListener('click', close)
  host.querySelector('[data-act="save"]')!.addEventListener('click', () => {
    if (!state.space_id) return
    const saved = { ...state }
    close()
    opts.onSaved(saved)
  })
}
