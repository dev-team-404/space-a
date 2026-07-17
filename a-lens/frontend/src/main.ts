// A-Lens 프론트 진입점 — PixiJS 씬 + DOM 패널의 최소 검증.
// 씬: 로비의 층(스페이스)들을 클릭 가능한 오브젝트로 그린다.
// 데이터: 백엔드 뷰모델(/api/lobby)만 사용한다 — 번역은 서버 몫 (ADR 0003).

import { Application, Container, Graphics, Text, TextStyle } from 'pixi.js'

type LobbyFloor = {
  space_id: string
  name: string
  floor: number | null
  activity: number
  stats: { knowledge?: number; reuse?: number; resolved?: number }
  highlight: string | null
}

type LobbyView = {
  floors: LobbyFloor[]
  totals: Record<string, number>
  tokens_saved_est: number | null
  highlight: { summary?: string } | null
}

const panel = document.getElementById('panel') as HTMLElement
const panelTitle = document.getElementById('panel-title') as HTMLElement
const panelBody = document.getElementById('panel-body') as HTMLElement

function showPanel(title: string, html: string) {
  panelTitle.textContent = title
  panelBody.innerHTML = html
  panel.hidden = false
}

async function fetchLobby(): Promise<LobbyView> {
  const res = await fetch('/api/lobby')
  if (!res.ok) throw new Error(`GET /api/lobby → ${res.status}`)
  const lobby: LobbyView = await res.json()
  if (!Array.isArray(lobby.floors)) throw new Error('로비 응답에 floors가 없음')
  return lobby
}

async function main() {
  const app = new Application()
  await app.init({ resizeTo: window, background: '#14171c', antialias: true })
  const scene = document.getElementById('scene')
  if (!scene) throw new Error('#scene 엘리먼트 없음')
  scene.appendChild(app.canvas)

  let lobby: LobbyView
  try {
    lobby = await fetchLobby()
  } catch (e) {
    showPanel('연결 오류', `백엔드(/api/lobby)에 연결하지 못했습니다.<br/>${String(e)}`)
    return
  }

  const building = new Container()
  app.stage.addChild(building)

  const floorW = 420
  const floorH = 80
  const labelStyle = new TextStyle({ fill: '#e8eaed', fontSize: 16 })

  // 층 = 스페이스. activity(0~3)가 창문 불빛 밝기 — 04-data-mapping의 공간 번역.
  const glow = ['#2a2f38', '#3d4652', '#556070', '#7a8aa0']

  lobby.floors.forEach((f, i) => {
    const y = i * (floorH + 12)
    const g = new Graphics()
      .roundRect(0, y, floorW, floorH, 8)
      .fill(glow[Math.min(f.activity, 3)] ?? glow[0])
    g.eventMode = 'static'
    g.cursor = 'pointer'
    g.on('pointertap', () => {
      showPanel(
        f.name,
        `지식 ${f.stats.knowledge ?? 0} · 재사용 ${f.stats.reuse ?? 0} · 해결 ${f.stats.resolved ?? 0}` +
          (f.highlight ? `<br/><br/>★ ${f.highlight}` : ''),
      )
    })
    building.addChild(g)

    const label = new Text({ text: `${f.floor ?? '?'}F  ${f.name}`, style: labelStyle })
    label.x = 16
    label.y = y + floorH / 2 - 10
    building.addChild(label)
  })

  building.x = (app.screen.width - floorW) / 2
  building.y = 60
}

main()
