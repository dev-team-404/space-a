// c2-live.js — 백엔드 브리지. 프로토타입이 a-lens 서버(/prototype)로 서빙될 때만,
// 가짜 C2 데이터를 실서버 스냅숏(/api/c2/*)으로 덮어쓴다.
// file:// 로 직접 열면 아무것도 하지 않는다 — 기존 목업 데모 그대로.
// 동기 XHR인 이유: c2-adapter.js가 로드 즉시 DB를 만들기 때문에 그 전에 끝나야 한다.
// (브리지는 임시 장치 — frontend/ 이관이 끝나면 프로토타입과 함께 제거된다)

(function () {
  if (!location.protocol.startsWith('http')) return;

  function get(path) {
    const x = new XMLHttpRequest();
    x.open('GET', path, false);
    x.send();
    if (x.status !== 200) throw new Error(path + ' → ' + x.status);
    return JSON.parse(x.responseText);
  }

  try {
    const spaces = get('/api/c2/spaces'); // 픽스처 모드면 404 → catch로 빠져 가짜 데이터 유지
    C2.spaces = spaces;
    C2.spaceDetail = get('/api/c2/space-details');
    C2.activity = get('/api/c2/activity');
    C2.reuseEvents = get('/api/c2/reuse-events');
    C2.stats = get('/api/c2/stats');
    console.info('[c2-live] 허브 실데이터로 렌더:', spaces.spaces.map((s) => s.space_id).join(', '));
  } catch (e) {
    console.warn('[c2-live] 브리지 미동작 — 내장 가짜 데이터로 렌더:', e.message);
  }
})();
