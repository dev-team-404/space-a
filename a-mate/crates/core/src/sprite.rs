//! AI 스프라이트 (2026-07-19) — 절차 생성의 화풍 한계를 넘기 위해, 사용자 레퍼런스 이미지를
//! 스타일 앵커로 첨부해 이미지 생성 모델로 **사용자별 스프라이트를 1회 생성 → 캐시**한다.
//! 시드 스펙(mascot::RobotSpec)이 인물 묘사를 결정하므로 같은 사람은 항상 같은 캐릭터.
//! 실패는 무해(절차 생성 v6 폴백). AGENT_MENTOR_SPRITE=off로 끌 수 있다.

use anyhow::{anyhow, Result};
use base64::Engine as _;

/// 스타일 앵커 — 사용자가 제공한 레퍼런스 픽셀 스프라이트 (같은 화풍, 다른 인물로 생성).
pub const STYLE_REF_JPG: &[u8] = include_bytes!("../assets/sprite-style-ref.jpg");

pub struct SpriteConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub const DEFAULT_IMAGE_MODEL: &str = "google/gemini-2.5-flash-image";

/// 비어있지 않은 첫 값을 고른다: 저장된 설정(설정창) → env 후보들 순.
fn pick(stored: Option<&str>, env_keys: &[&str]) -> Option<String> {
    stored
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            env_keys
                .iter()
                .filter_map(|k| std::env::var(k).ok())
                .map(|v| v.trim().to_string())
                .find(|v| !v.is_empty())
        })
}

impl SpriteConfig {
    /// Engine 설정(OpenRouter 등 OpenAI 호환)을 재사용. 미설정/off면 None.
    pub fn from_env() -> Option<SpriteConfig> {
        Self::resolve(None, None, None)
    }

    /// 설정창 저장값 → env 순으로 해석.
    /// 이미지 전용(`AGENT_MENTOR_IMAGE_*`)이 있으면 그것을, 없으면 텍스트 엔진 설정을 폴백으로 쓴다
    /// (텍스트는 사내 LM Studio, 이미지는 OpenRouter처럼 **분리**할 수 있게 하기 위함).
    /// url이 비면 None → 스프라이트 기능 전체 no-op(절차 생성 폴백).
    pub fn resolve(
        stored_url: Option<&str>,
        stored_key: Option<&str>,
        stored_model: Option<&str>,
    ) -> Option<SpriteConfig> {
        if std::env::var("AGENT_MENTOR_SPRITE").map(|v| v == "off").unwrap_or(false) {
            return None;
        }
        let base_url = pick(stored_url, &["AGENT_MENTOR_IMAGE_URL", "AGENT_MENTOR_ENGINE_URL"])?;
        let api_key = pick(stored_key, &["AGENT_MENTOR_IMAGE_KEY", "AGENT_MENTOR_ENGINE_KEY"])
            .unwrap_or_default();
        let model = pick(stored_model, &["AGENT_MENTOR_IMAGE_MODEL"])
            .unwrap_or_else(|| DEFAULT_IMAGE_MODEL.to_string());
        Some(SpriteConfig {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        })
    }
}

/// MBTI 성향 + 변주 시드 → (체형, 마감, 악세사리, 스타일 무드) 묘사 조각.
/// 각 축의 *부분집합*을 MBTI 성향으로 정의하고 변주 시드 해시가 그 안에서 고른다
/// (같은 타입=일관 무드, 시드마다 다른 조합). MBTI 미설정이면 편향 없이 전체에서 고른다.
fn mbti_traits(mbti: Option<&str>, seed: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(seed.as_bytes());
    let pick = |group: &[&'static str], byte: u8| group[(byte as usize) % group.len()];
    let m = mbti.and_then(crate::mascot::normalize_mbti);
    let mb = m.as_ref().map(|s| s.as_bytes());

    // 체형 (S/N): S=단단·컴팩트 / N=슬렌더·경량
    const BUILD_S: [&str; 2] = ["a compact sturdy body", "a solid grounded build"];
    const BUILD_N: [&str; 2] = ["a slender lightweight body", "a tall willowy build"];
    const BUILD_ANY: [&str; 3] = ["a compact body", "a slender body", "a sturdy build"];
    let build = match mb { Some(b) if b[1] == b'S' => pick(&BUILD_S, d[6]),
                           Some(b) if b[1] == b'N' => pick(&BUILD_N, d[6]),
                           _ => pick(&BUILD_ANY, d[6]) };

    // 마감·색온도 (T/F): T=차가운 금속 / F=따뜻·부드러움
    const FINISH_T: [&str; 2] = ["a cool brushed-steel finish", "a charcoal matte finish"];
    const FINISH_F: [&str; 2] = ["a warm cream plastic finish", "a soft matte-white finish"];
    const FINISH_ANY: [&str; 4] = ["a matte white finish", "a brushed steel finish", "a cream plastic finish", "a charcoal matte finish"];
    let finish = match mb { Some(b) if b[2] == b'T' => pick(&FINISH_T, d[7]),
                            Some(b) if b[2] == b'F' => pick(&FINISH_F, d[7]),
                            _ => pick(&FINISH_ANY, d[7]) };

    // 악세사리: 성향 그룹(N_T 분석가 / N_F 외교관 / S_J 관리자 / S_P 탐험가) 테마.
    // 2026-07-29 다양화 — 로봇 부품류 일변도에서 헤드폰·가방·안경·모자 등 일상 소품 확장.
    const ACC_NT: [&str; 6] = [
        "wearing slim rectangular glasses",
        "with a slim backpack module",
        "with a utility tool belt",
        "holding a small data tablet",
        "with a tiny status light on its chest",
        "with a pen tucked behind its head unit",
    ];
    const ACC_NF: [&str; 6] = [
        "wearing over-ear headphones",
        "with a soft knit scarf",
        "with a small shoulder lamp",
        "with soft glowing trim",
        "holding a tiny flower",
        "with a sticker-covered messenger bag",
    ];
    const ACC_SJ: [&str; 6] = [
        "with a neat bow tie",
        "wearing round glasses",
        "with a utility tool belt",
        "with a wristwatch panel",
        "with a tiny status light on its chest",
        "with a small name badge sticker on its chest",
    ];
    const ACC_SP: [&str; 6] = [
        "wearing a baseball cap",
        "with a crossbody sling bag",
        "holding a small camera",
        "with a light travel pack",
        "with a small shoulder lamp",
        "wearing sporty wristbands",
    ];
    const ACC_ANY: [&str; 10] = [
        "",
        "wearing over-ear headphones",
        "wearing round glasses",
        "with a crossbody sling bag",
        "wearing a baseball cap",
        "with a small shoulder lamp",
        "with a slim backpack module",
        "holding a small camera",
        "with a tiny status light on its chest",
        "with a utility tool belt",
    ];
    let accessory = match mb {
        Some(b) if b[1] == b'N' && b[2] == b'T' => pick(&ACC_NT, d[8]),
        Some(b) if b[1] == b'N' && b[2] == b'F' => pick(&ACC_NF, d[8]),
        Some(b) if b[1] == b'S' && b[3] == b'J' => pick(&ACC_SJ, d[8]),
        Some(b) if b[1] == b'S' && b[3] == b'P' => pick(&ACC_SP, d[8]),
        _ => pick(&ACC_ANY, d[8]),
    };

    // 스타일 무드 (S/N, 재미 요소): S=깔끔·단정 / N=개성·예술·몽환
    let styling = match mb {
        Some(b) if b[1] == b'S' => ", with a clean, tidy, conventional look",
        Some(b) if b[1] == b'N' => ", with a quirky, artistic mix-and-match look and a dreamy vibe",
        _ => "",
    };
    (build, finish, accessory, styling)
}

/// 시드 스펙 + MBTI + 변주 시드 → **로봇** 묘사. 화풍은 레퍼런스(치비 픽셀) 유지.
/// `seed`가 체형/마감/악세/무드 변주를 좌우한다(재생성마다 다른 후보).
pub fn character_description(spec: &crate::mascot::RobotSpec, mbti: Option<&str>, seed: &str) -> String {
    // antenna 슬롯 = 머리 형태
    const HEAD: [&str; 6] = [
        "a rounded helmet-shaped head with a short antenna",
        "a boxy head with rounded corners and two small side vents",
        "a dome head with a wide visor band",
        "a rounded head with small ear-discs on both sides",
        "a tall head unit with a blinking status light on top",
        "a compact head with a flat top panel and no antenna",
    ];
    const EYES: [&str; 6] = [
        "two glowing oval eyes behind a dark visor",
        "big round glowing eyes with bright highlights",
        "calm narrow glowing eye slits",
        "sparkling square eyes with bright highlights",
        "gentle droopy glowing eyes",
        "cheerful curved glowing eyes like a smile",
    ];
    // (본체색, 하체색, 발광 액센트색)
    const COLORS: [(&str, &str, &str); 8] = [
        ("navy", "grey", "cyan"),
        ("blue", "steel blue", "sky blue"),
        ("green", "grey", "lime"),
        ("pink", "mauve", "magenta"),
        ("purple", "dark grey", "violet"),
        ("mustard yellow", "slate", "amber"),
        ("teal", "grey", "mint"),
        ("slate grey", "charcoal", "white"),
    ];
    const POSE: [&str; 6] = [
        "arms relaxed at its sides",
        "hands resting on its hips",
        "one hand raised in a small wave",
        "both hands together in front",
        "holding a small toolbox",
        "hands behind its back",
    ];
    let (cc, lc, ac) = COLORS[(spec.palette as usize) % 8];
    let (build, finish, accessory, styling) = mbti_traits(mbti, seed);
    let chassis = match (spec.body as usize) % 6 {
        0 => format!("a {cc} rounded chest plate with a small lit panel"),
        1 => format!("a {cc} armored torso with shoulder pauldrons"),
        2 => format!("a plain {cc} torso with a single seam line"),
        3 => format!("a {cc} padded torso with soft rounded edges"),
        4 => format!("a white torso with {cc} trim and {cc} buttons"),
        _ => format!("a {cc} torso with an exposed cable harness"),
    };
    let acc_part = if accessory.is_empty() { String::new() } else { format!(", {accessory}") };
    format!(
        "a chibi pixel-art ROBOT (not a human) with {build} and {finish}: {head}, {eyes}, \
         {chassis}, {lc} leg units with flat feet, {ac} glowing accents{acc_part}, {pose}{styling}",
        head = HEAD[(spec.antenna as usize) % 6],
        eyes = EYES[(spec.eyes as usize) % 6],
        pose = POSE[(spec.arms as usize) % 6],
    )
}

/// 시드 → 캐시 파일명(hex 12자). 시드에 `|` 등 파일명 불가 문자가 있어 해시로 안전화.
pub fn seed_cache_name(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(seed.as_bytes());
    format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3], d[4], d[5])
}

/// 임의 시드(방 점유자의 mascot_seed 등)로 스프라이트 PNG를 생성.
/// 내 스프라이트와 **완전히 동일한 로직** — robot_spec_for(seed) → character_description(spec, seed) → generate.
/// 방문 시 다른 사람도 같은 화풍의 AI 캐릭터로 보이게 하는 용도(2026-07-19).
pub fn sprite_for_seed(cfg: &SpriteConfig, seed: &str) -> Result<Vec<u8>> {
    let spec = crate::mascot::robot_spec_for(seed);
    let desc = character_description(&spec, None, seed);
    generate(cfg, &desc)
}

/// 생성 이미지는 "plain white background"로 그려지는데, 마스코트 창은 투명이라 그대로 두면
/// 캐릭터 주위에 **흰 박스**가 보인다. **테두리에서 연결된 배경색만** 알파 0으로 지운다 —
/// 안쪽 흰색(신발·후드 끈)은 둘러싸여 있어 보존된다(플러드 필이 실루엣 아웃라인에서 막힘).
pub fn make_background_transparent(png_bytes: &[u8]) -> Result<Vec<u8>> {
    use std::collections::VecDeque;

    let mut decoder = png::Decoder::new(png_bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 {
        return Err(anyhow!("빈 이미지"));
    }
    let ch = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(anyhow!("지원하지 않는 PNG 색 형식: {other:?}")),
    };

    // RGBA로 정규화
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        rgba[i * 4] = buf[i * ch];
        rgba[i * 4 + 1] = buf[i * ch + 1];
        rgba[i * 4 + 2] = buf[i * ch + 2];
        rgba[i * 4 + 3] = if ch == 4 { buf[i * ch + 3] } else { 255 };
    }

    // 배경 기준색 = 네 모서리 평균 (모델이 순백이 아닌 254 같은 값을 쓰기도 함)
    let at = |px: &[u8], i: usize| [px[i * 4] as i32, px[i * 4 + 1] as i32, px[i * 4 + 2] as i32];
    let corners = [0, w - 1, (h - 1) * w, (h - 1) * w + (w - 1)];
    let mut bg = [0i32; 3];
    for c in corners {
        let p = at(&rgba, c);
        for k in 0..3 {
            bg[k] += p[k];
        }
    }
    for k in 0..3 {
        bg[k] /= 4;
    }
    // 밝고(=배경 후보) 기준색과 가까운 픽셀만 배경으로 본다.
    /// 기준색과의 채널 합 거리 허용치 — JPEG/모델 노이즈로 배경이 순백이 아닐 수 있어 여유를 둔다.
    const BG_COLOR_DISTANCE_THRESHOLD: i32 = 42;
    /// 배경으로 인정할 최소 밝기 — 어두운 실루엣·머리색이 배경으로 오인되지 않게 한다.
    const BG_MIN_BRIGHTNESS: i32 = 150;
    let is_bg = |px: &[u8], i: usize| -> bool {
        let p = at(px, i);
        let dist = (p[0] - bg[0]).abs() + (p[1] - bg[1]).abs() + (p[2] - bg[2]).abs();
        dist <= BG_COLOR_DISTANCE_THRESHOLD && p[0].min(p[1]).min(p[2]) > BG_MIN_BRIGHTNESS
    };

    // 테두리에서 플러드 필
    let mut seen = vec![false; w * h];
    let mut q: VecDeque<usize> = VecDeque::new();
    {
        let mut seed = |i: usize| {
            if !seen[i] && is_bg(&rgba, i) {
                seen[i] = true;
                q.push_back(i);
            }
        };
        for x in 0..w {
            seed(x);
            seed((h - 1) * w + x);
        }
        for y in 0..h {
            seed(y * w);
            seed(y * w + (w - 1));
        }
    }
    while let Some(i) = q.pop_front() {
        let (x, y) = (i % w, i / w);
        let neighbours = [
            if x > 0 { Some(i - 1) } else { None },
            if x + 1 < w { Some(i + 1) } else { None },
            if y > 0 { Some(i - w) } else { None },
            if y + 1 < h { Some(i + w) } else { None },
        ];
        for n in neighbours.into_iter().flatten() {
            if !seen[n] && is_bg(&rgba, n) {
                seen[n] = true;
                q.push_back(n);
            }
        }
    }
    for i in 0..w * h {
        if seen[i] {
            rgba[i * 4 + 3] = 0;
        }
    }

    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&rgba)?;
    }
    Ok(out)
}

/// G6 — 스프라이트에서 얼굴(머리) 영역을 잘라 128×128 아이콘 PNG로 만든다.
/// 크롭 기준은 시각 검증된 CSS(background-size:180%, position 50% 14%)의 환산:
/// 윈도우 한 변 = min(W,H)/1.8, 가로 중앙, 세로 top = 0.14 × (H − 윈도우).
/// 다운스케일은 nearest-neighbor — 치비 픽셀아트의 또렷한 픽셀 경계 보존.
pub fn crop_face(png_bytes: &[u8]) -> Result<Vec<u8>> {
    const FACE_ICON_SIZE: usize = 128;
    let mut decoder = png::Decoder::new(png_bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 {
        return Err(anyhow!("빈 이미지"));
    }
    let ch = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(anyhow!("지원하지 않는 PNG 색 형식: {other:?}")),
    };
    let side = ((w.min(h) as f32 / 1.8).round() as usize).clamp(1, w.min(h));
    let x0 = (w - side) / 2;
    let y0 = (((h - side) as f32) * 0.14).round() as usize;
    let mut out_rgba = vec![0u8; FACE_ICON_SIZE * FACE_ICON_SIZE * 4];
    for oy in 0..FACE_ICON_SIZE {
        let sy = y0 + oy * side / FACE_ICON_SIZE;
        for ox in 0..FACE_ICON_SIZE {
            let sx = x0 + ox * side / FACE_ICON_SIZE;
            let si = sy * w + sx;
            let oi = (oy * FACE_ICON_SIZE + ox) * 4;
            out_rgba[oi] = buf[si * ch];
            out_rgba[oi + 1] = buf[si * ch + 1];
            out_rgba[oi + 2] = buf[si * ch + 2];
            out_rgba[oi + 3] = if ch == 4 { buf[si * ch + 3] } else { 255 };
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, FACE_ICON_SIZE as u32, FACE_ICON_SIZE as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&out_rgba)?;
    }
    Ok(out)
}

/// 이미지 생성 응답에서 PNG 바이트를 추출한다. 게이트웨이마다 이미지를 담는 위치가 달라서
/// (같은 "OpenAI 호환"이라도) 알려진 형태를 순서대로 시도하는 **관용적 파서**다:
/// - OpenRouter: `choices[0].message.images[0].image_url.url` = data URL
/// - LiteLLM(gemini-2.5-flash-image): `choices[0].message.content` 문자열 안에 `data:image…;base64,…`
///
/// 둘 다 없으면 에러. 새 게이트웨이가 또 다른 위치를 쓰면 여기 후보만 추가하면 된다.
fn extract_image_bytes(resp: &serde_json::Value) -> Result<Vec<u8>> {
    let msg = resp.pointer("/choices/0/message");
    // 1) OpenRouter 확장 필드
    let from_images = msg
        .and_then(|m| m.pointer("/images/0/image_url/url"))
        .and_then(|v| v.as_str());
    // 2) LiteLLM: content 문자열에 박힌 data URL
    let from_content = msg
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .and_then(|c| c.find("data:image").map(|i| &c[i..]));
    let data_url = from_images
        .or(from_content)
        .ok_or_else(|| anyhow!("sprite 응답에 이미지 없음"))?;
    decode_data_url(data_url)
}

/// `data:image/png;base64,<payload>` → 디코드된 바이트. 모델이 뒤에 붙인 잡담·개행은 잘라낸다
/// (base64 표준 알파벳에는 공백이 없으므로 첫 공백 전까지가 페이로드).
fn decode_data_url(data_url: &str) -> Result<Vec<u8>> {
    let payload = data_url
        .split_once(',')
        .map(|(_, b)| b)
        .ok_or_else(|| anyhow!("sprite data URL 형식 아님"))?;
    let payload = payload.split_whitespace().next().unwrap_or("");
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| anyhow!("sprite base64 디코드 실패: {e}"))
}

/// 키가 있으면 `Authorization: Bearer <key>`를 붙이고, 비면 헤더를 **생략**한다.
/// 익명 접근을 허용하는 게이트웨이를 지원하며, 프로브와 실제 생성이 **동일하게 인증**하도록 통일한다
/// (빈 키에 프로브는 생략·생성은 빈 Bearer를 보내던 불일치를 제거 — 테스트가 재생성을 충실히 예측).
fn with_bearer(req: ureq::Request, api_key: &str) -> ureq::Request {
    if api_key.is_empty() {
        req
    } else {
        req.set("Authorization", &format!("Bearer {}", api_key))
    }
}

/// 스타일 앵커(레퍼런스 이미지)를 첨부해 이미지 1장을 요청한다 — generate/generate_cut 공용 배관.
fn request_image(cfg: &SpriteConfig, prompt: &str) -> Result<Vec<u8>> {
    let ref_b64 = base64::engine::general_purpose::STANDARD.encode(STYLE_REF_JPG);
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{ref_b64}")}}
            ]
        }],
        "modalities": ["image", "text"],
    });
    let req = ureq::post(&format!("{}/chat/completions", cfg.base_url))
        .timeout(std::time::Duration::from_secs(120))
        .set("Content-Type", "application/json");
    let resp: serde_json::Value = with_bearer(req, &cfg.api_key)
        .send_json(body)
        .map_err(|e| anyhow!("sprite 생성 요청 실패: {e}"))?
        .into_json()?;
    extract_image_bytes(&resp)
}

/// 이미지 생성 — 스타일 앵커 + 인물 묘사. 반환 = PNG 바이트.
pub fn generate(cfg: &SpriteConfig, description: &str) -> Result<Vec<u8>> {
    // "checkerboard 금지"는 실측 대응 — 모델이 가끔 '투명 배경' 흉내로 회색-흰색 체커보드를
    // 실제 픽셀로 그려버리는데, 균일 배경만 지우는 투명화가 이를 못 걷어낸다 (2026-07-29).
    let prompt = format!(
        "Using the EXACT same art style as the attached reference image (16-bit pixel art sprite, \
         chibi proportions with large head, clean dark pixel outline, soft cel shading, \
         front-facing full body, centered, plain white background), draw a DIFFERENT character: \
         {description}. Match the reference's pixel density, outline thickness, shading style and \
         proportions exactly. Single character only, no text, no watermark. The background must be \
         one flat solid white color (#ffffff) — NEVER a gray-and-white checkerboard or any \
         transparency pattern."
    );
    let png = request_image(cfg, &prompt)?;
    // 흰 배경 → 투명. 실패해도 캐릭터는 보여야 하므로 원본으로 폴백(무해).
    match make_background_transparent(&png) {
        Ok(t) => Ok(t),
        Err(e) => {
            eprintln!("[sprite] 배경 투명화 실패(원본 사용): {e}");
            Ok(png)
        }
    }
}

/// 이미지 엔드포인트 프로브 결과 — 무과금 `GET /models` 기반.
/// 실제 이미지 생성 능력까지는 확인하지 못한다(그건 `generate`뿐).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// 200 + 설정 모델이 /models 목록에 있음.
    Ok,
    /// 200 이나 설정 모델이 목록에 없음 (샘플 id 최대 5개).
    ModelMissing(Vec<String>),
    /// 200 이나 `data[]` 배열이 없음 — OpenAI 호환 /models 아님.
    NotOpenAiCompat,
    /// 401 / 403.
    AuthFailed(u16),
    /// 429.
    RateLimited,
    /// 기타 non-2xx.
    HttpError(u16),
    /// 전송 실패(연결 불가·DNS 등).
    Connection(String),
}

/// `GET /models` 200 응답 본문 + 설정 모델명 → 판정 (순수).
/// OpenAI 규격: `{"data":[{"id":"..."}, ...]}`.
pub fn classify_models_body(body: &serde_json::Value, model: &str) -> ProbeVerdict {
    let Some(list) = body.get("data").and_then(|d| d.as_array()) else {
        return ProbeVerdict::NotOpenAiCompat;
    };
    let ids: Vec<String> = list
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();
    let want = model.trim();
    if ids.iter().any(|id| id.trim() == want) {
        ProbeVerdict::Ok
    } else {
        ProbeVerdict::ModelMissing(ids.into_iter().take(5).collect())
    }
}

/// 무과금 프로브 — `GET {base_url}/models`로 엔드포인트·키·모델을 확인한다.
/// (네트워크 — 단위테스트 제외, 로컬 LiteLLM 수동 확인.)
pub fn probe_endpoint(cfg: &SpriteConfig) -> ProbeVerdict {
    let url = format!("{}/models", cfg.base_url);
    let req = ureq::get(&url).timeout(std::time::Duration::from_secs(10));
    match with_bearer(req, &cfg.api_key).call() {
        Ok(resp) => {
            let body: serde_json::Value = resp.into_json().unwrap_or(serde_json::Value::Null);
            classify_models_body(&body, &cfg.model)
        }
        Err(ureq::Error::Status(code, _)) => match code {
            401 | 403 => ProbeVerdict::AuthFailed(code),
            429 => ProbeVerdict::RateLimited,
            _ => ProbeVerdict::HttpError(code),
        },
        Err(ureq::Error::Transport(t)) => ProbeVerdict::Connection(t.to_string()),
    }
}

// ── H2 매일 마스코트 컷 (2026-07-28) ─────────────────────────────────────────
// 스펙: docs/archive/design/a-mate/specs/2026-07-28-sprite-face-daily-cut-design.md

/// H2 — 컷의 샷 축 = 싸이월드 레전드 짤 연출 11종 + 정경 1종 (마스코트 11 : 정경 1).
/// 정면 무난 샷 대신 그 시절 사진 클리셰(얼짱각도·하두리·점프샷·허세…)를 재현한다
/// (2026-07-29 사용자 피드백 — 코믹 레전드 요소 반영).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutShot {
    /// 45도 위에서 내려찍은 얼짱각도 셀카 — 얼굴이 프레임 대부분, 과노출 플래시
    HighAngle,
    /// 하두리캠 흑백 셀카 — 거친 저해상, 괜히 심각한 응시
    Haduri,
    /// 억지 점프샷 — 공중부양, 허우적대는 팔다리
    Jump,
    /// 뒷모습 노을 갬성 — 먼 곳 응시
    BackView,
    /// 화장실 거울 셀카 — 폴더폰으로 얼굴 반 가림
    Mirror,
    /// 소품 허세 — 일상 물건을 기타처럼 들고 아티스트인 척
    PropSwagger,
    /// 중2병 허세샷 — 한쪽 눈 가리기·이마 짚기, 바람에 휘날림
    Drama,
    /// 손가락총 + 윙크 — 오글 포즈
    FingerGun,
    /// 찜질방 양머리 수건 — 간식 소품
    Sauna,
    /// 네컷 스티커사진 — 2×2 분할, 하트·별 낙서만 (글자 금지 유지)
    FourCut,
    /// 음식 인증샷 — 밥상 위에서 내려찍기, 젓가락 든 마스코트
    FoodShot,
    /// 정경 — 마스코트 없이 책상·소품으로 하루 은유
    Scene,
}

impl CutShot {
    pub fn as_str(&self) -> &'static str {
        match self {
            CutShot::HighAngle => "uljjang",
            CutShot::Haduri => "haduri",
            CutShot::Jump => "jump",
            CutShot::BackView => "backview",
            CutShot::Mirror => "mirror",
            CutShot::PropSwagger => "prop",
            CutShot::Drama => "drama",
            CutShot::FingerGun => "fingergun",
            CutShot::Sauna => "sauna",
            CutShot::FourCut => "fourcut",
            CutShot::FoodShot => "food",
            CutShot::Scene => "scene",
        }
    }
    pub fn has_mascot(&self) -> bool {
        !matches!(self, CutShot::Scene)
    }
}

/// 날짜+정체성 시드 → 결정론적 샷 pick (재현·테스트 가능 — character_description 변주 시드 선례).
pub fn pick_cut_shot(date: &str, seed: &str) -> CutShot {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(format!("{date}|{seed}").as_bytes());
    match d[0] % 12 {
        0 => CutShot::HighAngle,
        1 => CutShot::Haduri,
        2 => CutShot::Jump,
        3 => CutShot::BackView,
        4 => CutShot::Mirror,
        5 => CutShot::PropSwagger,
        6 => CutShot::Drama,
        7 => CutShot::FingerGun,
        8 => CutShot::Sauna,
        9 => CutShot::FourCut,
        10 => CutShot::FoodShot,
        _ => CutShot::Scene,
    }
}

/// H2 — 컷 상태(app_data/daily_cut.json). `cut_date`는 화면의 daily_cut.png가 어느 일기의
/// 컷인지(재실행 멱등 키), `attempt_date`/`attempts`는 일일 과금 상한의 원장이다.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CutState {
    pub cut_date: Option<String>,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub shot: String,
    #[serde(default)]
    pub attempt_date: String,
    #[serde(default)]
    pub attempts: u8,
}

/// 하루 최대 생성 시도 — 이미지 건당 과금 가드 (스펙 결정 2).
pub const MAX_CUT_ATTEMPTS_PER_DAY: u8 = 3;

/// 리컨실리에이션 판단 — true면 생성 시도. 최신 일기 컷이 이미 있거나(멱등)
/// 그 날짜의 시도가 소진됐으면 skip. 멱등 판단은 메타만 믿지 않고 **png 실존**도 본다 —
/// 파일만 지워진 경우 그 일기 날짜 내내 sprite 폴백으로 고착되는 것을 막는다 (과금 상한은 유지).
pub fn decide_cut(latest_diary_date: &str, s: &CutState, cut_png_exists: bool) -> bool {
    if s.cut_date.as_deref() == Some(latest_diary_date) && cut_png_exists {
        return false;
    }
    if s.attempt_date == latest_diary_date && s.attempts >= MAX_CUT_ATTEMPTS_PER_DAY {
        return false;
    }
    true
}

/// 시도 기록 — 대상 날짜가 바뀌면 카운터 리셋. 호출자는 네트워크 **전에** persist해
/// 실패·크래시에도 상한을 보장한다.
pub fn register_attempt(s: &mut CutState, date: &str) {
    if s.attempt_date != date {
        s.attempt_date = date.to_string();
        s.attempts = 0;
    }
    s.attempts = s.attempts.saturating_add(1);
}

/// 성공 기록 — 화면 상태(cut_date·caption·shot)를 새 컷으로 갱신.
pub fn register_success(s: &mut CutState, date: &str, caption: &str, shot: CutShot) {
    s.cut_date = Some(date.to_string());
    s.caption = caption.to_string();
    s.shot = shot.as_str().to_string();
}

/// H2 — 일기 → {scene_en, caption_ko} 텍스트 엔진 시스템 프롬프트.
/// 전송 수위(ADR 0024): scene_en은 이미지 엔진으로 넘어가므로 **추상 장면만** —
/// 고유명사·프로젝트/회사명·코드 식별자·수치 금지를 여기서 지시한다.
pub fn build_cut_scene_prompt(diary: &str, shot: CutShot, mbti: Option<&str>) -> String {
    let framing = match shot {
        CutShot::HighAngle => {
            "a 45-degree high-angle selfie — the robot's face fills most of the frame, overexposed flash glow, dreamy haze"
        }
        CutShot::Haduri => {
            "a grainy old-webcam monochrome selfie — the robot stares into the camera looking way too serious"
        }
        CutShot::Jump => {
            "a forced dramatic jump shot — the robot frozen mid-air at a scenic spot, limbs flailing"
        }
        CutShot::BackView => {
            "a sentimental back-view shot — the robot seen from behind, gazing into the distance under an evening sky"
        }
        CutShot::Mirror => {
            "a bathroom-mirror selfie — the robot holds an old flip phone half covering its face"
        }
        CutShot::PropSwagger => {
            "a swagger shot — the robot poses with an everyday object as if it were a rock star's guitar"
        }
        CutShot::Drama => {
            "an overly dramatic pose — one hand covering an eye or pressed to the forehead, windswept, taking itself far too seriously"
        }
        CutShot::FingerGun => "a cheesy finger-gun pose winking at the camera",
        CutShot::Sauna => {
            "a Korean sauna shot — the robot wears a towel folded into lamb ears on its head, snacks beside it"
        }
        CutShot::FourCut => {
            "a four-panel sticker-photo strip — the same robot doing four different silly poses, decorated with heart and star doodles only"
        }
        CutShot::FoodShot => {
            "a top-down food-brag shot over a table of food — the robot reaching in with chopsticks"
        }
        CutShot::Scene => {
            "a cozy scene WITHOUT any character — desk, objects and lighting that hint at today's activity"
        }
    };
    let mood = crate::mascot::mbti_voice_hint_style_only(mbti);
    format!(
        "너는 픽셀아트 일러스트 연출가다. 아래 '오늘의 일기'를 읽고, 오늘 하루를 은유하는 \
         그림 한 컷을 기획하라.\n\
         구도: {framing}.\n{mood}\n\
         규칙:\n\
         - scene_en: 영어 1~2문장 장면 묘사. 반드시 **추상적으로** — 고유명사(사람·회사·프로젝트 \
           이름), 코드 식별자, 파일명, 숫자 수치를 절대 쓰지 마라. 분위기·행동·소품·조명 위주로.\n\
         - caption_ko: 그림 아래 붙일 한국어 감성 한 줄(40자 이내, 싸이월드 미니홈피 갬성, \
           마스코트 1인칭, 따옴표 없이).\n\
         - 그림 속에 글자는 넣을 수 없다 — 텍스트가 필요한 장면을 만들지 마라.\n\
         출력은 JSON 하나만: {{\"scene_en\": \"...\", \"caption_ko\": \"...\"}}\n\n\
         오늘의 일기:\n{diary}"
    )
}

/// 응답 JSON 관용 파싱 — 코드펜스·잡담을 걷어내고 첫 '{'…마지막 '}'만 취한다
/// (extract_image_bytes의 관용 파서와 같은 철학). 빈 필드는 실패로 본다.
pub fn parse_cut_scene(raw: &str) -> Option<(String, String)> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(&raw[start..=end]).ok()?;
    let scene = v.get("scene_en")?.as_str()?.trim().to_string();
    let caption = v.get("caption_ko")?.as_str()?.trim().to_string();
    (!scene.is_empty() && !caption.is_empty()).then_some((scene, caption))
}

/// H2 — 장면+캡션 계산 (store 접근 없음, 네트워크만 — mascot::compute_daily_line 선례).
/// Ok(None) = 응답 파싱 실패 (호출자는 시도 카운트만 남기고 skip).
pub fn compute_cut_scene(
    engine: &dyn crate::diary::engine::Engine,
    diary: &str,
    shot: CutShot,
    mbti: Option<&str>,
) -> Result<Option<(String, String)>> {
    let system = build_cut_scene_prompt(diary, shot, mbti);
    let raw = engine.generate(&system, "")?.text;
    Ok(parse_cut_scene(&raw))
}

/// H2 — 컷 이미지 프롬프트 조립 (로컬, 네트워크 없음). 캐릭터 묘사는 마스코트 샷에만 붙는다
/// (Scene 컷은 캐릭터 없는 정경 — 스펙 데이터 흐름 ⑤).
pub fn build_cut_image_prompt(shot: CutShot, character_desc: Option<&str>, scene_en: &str) -> String {
    let composition = match shot {
        CutShot::HighAngle => {
            "extreme high-angle selfie composition, face filling most of the frame, overexposed flash, hazy glow"
        }
        CutShot::Haduri => {
            "grainy black-and-white webcam selfie composition, low fidelity, soft vignette"
        }
        CutShot::Jump => {
            "wide shot, character frozen mid-jump high above the ground, dynamic silly pose"
        }
        CutShot::BackView => "back-view composition, small character against a wide evening sky",
        CutShot::Mirror => {
            "mirror-reflection selfie composition, flip phone partly covering the face"
        }
        CutShot::PropSwagger => {
            "three-quarter shot, confident swagger pose holding a prop like a guitar"
        }
        CutShot::Drama => "dramatic portrait, one hand over an eye, windswept, moody lighting",
        CutShot::FingerGun => "medium shot, finger-gun pointed at the camera, winking",
        CutShot::Sauna => "cozy indoor medium shot, towel folded like lamb ears on the head",
        CutShot::FourCut => {
            "single image split into a 2x2 sticker-photo grid, four silly poses, heart and star doodles only, absolutely no letters"
        }
        CutShot::FoodShot => {
            "top-down table composition, dishes centered, character reaching in with chopsticks"
        }
        CutShot::Scene => "environment-only composition, NO characters at all",
    };
    let character = character_desc
        .map(|d| format!(" The character is {d}."))
        .unwrap_or_default();
    format!(
        "Using the EXACT same art style as the attached reference image (16-bit pixel art, \
         chibi proportions, clean dark pixel outline, soft cel shading), draw one scene: \
         {scene_en}.{character} {composition}. \
         No text, no letters, no words, no watermark."
    )
}

/// H2 — 장면 컷 생성. sprite와 달리 배경 투명화를 하지 않는다 — 장면 전체가 그림이다.
pub fn generate_cut(cfg: &SpriteConfig, image_prompt: &str) -> Result<Vec<u8>> {
    request_image(cfg, image_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mascot::robot_spec_for;

    #[test]
    fn description_varies_by_seed_and_reflects_mbti() {
        let spec = robot_spec_for("seed-A");
        // 같은 (spec, mbti, seed) → 결정론
        let a1 = character_description(&spec, Some("INTJ"), "v1");
        let a2 = character_description(&spec, Some("INTJ"), "v1");
        assert_eq!(a1, a2);
        // 다른 변주 시드 → 달라질 수 있다(표본에서 최소 2종)
        use std::collections::HashSet;
        let set: HashSet<_> = (0..12)
            .map(|i| character_description(&spec, Some("INTJ"), &format!("v{i}")))
            .collect();
        assert!(set.len() > 1, "변주 시드로 묘사가 달라져야 함 (distinct={})", set.len());
        // 로봇 어휘 유지
        assert!(a1.contains("ROBOT") && a1.contains("leg units"));
    }

    #[test]
    fn description_is_deterministic_and_covers_slots() {
        let id = "DESKTOP-X|user";
        let spec = robot_spec_for(id);
        let d1 = character_description(&spec, None, id);
        let d2 = character_description(&spec, None, id);
        assert_eq!(d1, d2);
        assert!(d1.contains("chibi pixel-art"));
        // 마스코트는 사람이 아니라 로봇 — 화풍은 유지하되 인물 어휘가 섞이면 안 된다
        assert!(d1.contains("ROBOT"), "로봇으로 묘사되어야 함: {d1}");
        assert!(d1.contains("leg units"), "다리 유닛 슬롯 포함: {d1}");
        for human in ["skin", "hair", "pants", "sneakers", "hoodie"] {
            assert!(!d1.contains(human), "인물 어휘 '{human}'가 남아있음: {d1}");
        }
        // MBTI가 있어도 인물 어휘가 섞이면 안 된다.
        let d3 = character_description(&spec, Some("INTJ"), id);
        for human in ["skin", "hair", "pants", "sneakers", "hoodie"] {
            assert!(!d3.contains(human), "MBTI 묘사에 인물 어휘 '{human}'가 남아있음: {d3}");
        }
    }

    #[test]
    fn different_seeds_can_differ() {
        let a = character_description(&robot_spec_for("A|a"), None, "A|a");
        let b = character_description(&robot_spec_for("B|bbbb"), None, "B|bbbb");
        assert_ne!(a, b);
    }

    #[test]
    fn seed_cache_name_is_stable_and_filename_safe() {
        let a = seed_cache_name("JUNNYEONG-PC|junnyeong");
        assert_eq!(a, seed_cache_name("JUNNYEONG-PC|junnyeong")); // 결정론
        assert_ne!(a, seed_cache_name("OTHER|user")); // 시드마다 다름
        assert_eq!(a.len(), 12);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "파일명 안전(hex): {a}");
    }

    #[test]
    fn transparent_bg_removes_only_border_connected_white() {
        let (w, h) = (9usize, 9usize);
        let mut rgb = vec![255u8; w * h * 3]; // 전부 흰색
        // (2,2)-(6,6) 테두리를 어둡게 → 그 안쪽 흰색은 실루엣에 둘러싸인 상태가 된다
        for x in 2..=6usize {
            for y in 2..=6usize {
                if x == 2 || x == 6 || y == 2 || y == 6 {
                    let i = (y * w + x) * 3;
                    rgb[i] = 20;
                    rgb[i + 1] = 20;
                    rgb[i + 2] = 30;
                }
            }
        }
        let mut src = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut src, w as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        }

        let out = make_background_transparent(&src).expect("투명화 성공");

        let mut dec = png::Decoder::new(&out[..]);
        let mut r = dec.read_info().unwrap();
        let mut buf = vec![0u8; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let alpha = |x: usize, y: usize| buf[(y * w + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0, "테두리에 연결된 바깥 흰 배경 → 투명");
        assert_eq!(alpha(4, 4), 255, "링 안쪽 흰색(신발·끈 상당) → 보존");
        assert_eq!(alpha(4, 2), 255, "실루엣(어두운 링) 자체 → 보존");
    }

    #[test]
    fn resolve_prefers_stored_settings_and_trims_url() {
        // 저장된 설정이 있으면 env와 무관하게 그것을 쓴다 (설정창 우선).
        let cfg = SpriteConfig::resolve(
            Some("  https://openrouter.ai/api/v1/  "),
            Some(" sk-test "),
            Some(" some/model "),
        )
        .expect("stored url이 있으면 Some");
        assert_eq!(cfg.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.model, "some/model");
    }

    #[test]
    fn resolve_defaults_model_when_not_given() {
        let cfg = SpriteConfig::resolve(Some("https://x/api/v1"), Some(""), None)
            .expect("stored url이 있으면 Some");
        // 모델 미지정 + env 미설정이면 기본 이미지 모델
        if std::env::var("AGENT_MENTOR_IMAGE_MODEL").is_err() {
            assert_eq!(cfg.model, DEFAULT_IMAGE_MODEL);
        }
        assert_eq!(cfg.base_url, "https://x/api/v1");
    }

    #[test]
    fn style_ref_asset_is_bundled() {
        assert!(STYLE_REF_JPG.len() > 10_000, "스타일 앵커 이미지 번들 확인");
    }

    fn data_url_resp_openrouter(bytes: &[u8]) -> serde_json::Value {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        serde_json::json!({
            "choices": [{"message": {"role": "assistant",
                "images": [{"image_url": {"url": format!("data:image/png;base64,{b64}")}}]}}]
        })
    }

    #[test]
    fn extract_image_reads_openrouter_images_field() {
        let raw = vec![0x89u8, 0x50, 0x4e, 0x47, 1, 2, 3, 42];
        let got = extract_image_bytes(&data_url_resp_openrouter(&raw)).expect("OpenRouter 포맷");
        assert_eq!(got, raw);
    }

    #[test]
    fn extract_image_reads_litellm_content_data_url() {
        // LiteLLM(gemini-2.5-flash-image)은 이미지를 message.content 안에 data URL로 실어 보낸다.
        let raw = vec![0x89u8, 0x50, 0x4e, 0x47, 9, 8, 7, 6];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        let resp = serde_json::json!({
            "choices": [{"message": {"role": "assistant",
                "content": format!("Here you go! data:image/png;base64,{b64}")}}]
        });
        let got = extract_image_bytes(&resp).expect("LiteLLM content 포맷");
        assert_eq!(got, raw);
    }

    #[test]
    fn extract_image_ignores_trailing_prose_after_data_url() {
        // 모델이 이미지 뒤에 잡담을 붙여도 base64만 잘라 디코드해야 한다.
        let raw = vec![1u8, 2, 3, 4, 5];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        let resp = serde_json::json!({
            "choices": [{"message": {"role": "assistant",
                "content": format!("data:image/png;base64,{b64}\n\nHope you like it!")}}]
        });
        let got = extract_image_bytes(&resp).expect("뒤 잡담 무시");
        assert_eq!(got, raw);
    }

    #[test]
    fn extract_image_errors_when_no_image_present() {
        let resp = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "sorry, I can't draw that"}}]
        });
        assert!(extract_image_bytes(&resp).is_err(), "이미지 없으면 에러여야 함");
    }

    #[test]
    fn classify_models_body_ok_when_model_listed() {
        let body = serde_json::json!({"data":[
            {"id":"gpt-4o"},
            {"id":"gemini/gemini-2.5-flash-image"}
        ]});
        assert_eq!(
            classify_models_body(&body, "gemini/gemini-2.5-flash-image"),
            ProbeVerdict::Ok
        );
    }

    #[test]
    fn classify_models_body_missing_when_model_absent() {
        let body = serde_json::json!({"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]});
        match classify_models_body(&body, "gemini/gemini-2.5-flash-image") {
            ProbeVerdict::ModelMissing(ids) => {
                assert!(ids.contains(&"gpt-4o".to_string()), "샘플 id 포함: {ids:?}");
                assert!(ids.len() <= 5, "샘플은 최대 5개");
            }
            v => panic!("ModelMissing 기대, got {v:?}"),
        }
    }

    #[test]
    fn classify_models_body_not_openai_when_no_data_array() {
        // data[] 배열이 없으면(예: 에러 오브젝트/HTML) OpenAI 호환이 아니다.
        let body = serde_json::json!({"error":"not found"});
        assert_eq!(classify_models_body(&body, "x"), ProbeVerdict::NotOpenAiCompat);
    }

    #[test]
    fn classify_models_body_trims_model_before_compare() {
        let body = serde_json::json!({"data":[{"id":"some/model"}]});
        assert_eq!(classify_models_body(&body, "  some/model  "), ProbeVerdict::Ok);
    }

    /// 테스트용 RGBA PNG 인코더 (crop_face 검증 전용).
    fn encode_rgba_png(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&px(x, y));
            }
        }
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        out
    }

    fn decode_rgba_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut decoder = png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        buf.truncate((info.width * info.height * 4) as usize);
        (info.width, info.height, buf)
    }

    #[test]
    fn crop_face_extracts_upper_center_window_at_128() {
        // 360×360: 윈도우 side = round(360/1.8) = 200, x0 = (360-200)/2 = 80,
        // y0 = round((360-200)×0.14) = 22 — CSS(background-size:180%, position 50% 14%) 환산.
        // 윈도우 안 = 초록, 밖 = 빨강. 크롭 결과는 전부 초록이어야 한다.
        let png = encode_rgba_png(360, 360, |x, y| {
            if (80..280).contains(&x) && (22..222).contains(&y) {
                [10, 200, 30, 255]
            } else {
                [200, 10, 30, 255]
            }
        });
        let out = crop_face(&png).unwrap();
        let (w, h, rgba) = decode_rgba_png(&out);
        assert_eq!((w, h), (128, 128));
        for (x, y) in [(0u32, 0u32), (64, 64), (127, 127)] {
            let i = ((y * 128 + x) * 4) as usize;
            assert_eq!(&rgba[i..i + 3], &[10, 200, 30], "픽셀 ({x},{y})는 크롭 윈도우 안이어야 함");
        }
    }

    #[test]
    fn crop_face_handles_tiny_image_by_clamping() {
        // 4×4 초소형: side = round(4/1.8) = 2 → 클램프·업스케일 경로도 에러 없이 128×128.
        let png = encode_rgba_png(4, 4, |_, _| [1, 2, 3, 255]);
        let (w, h, _) = decode_rgba_png(&crop_face(&png).unwrap());
        assert_eq!((w, h), (128, 128));
    }

    #[test]
    fn pick_cut_shot_is_deterministic_and_covers_all_variants() {
        assert_eq!(pick_cut_shot("2026-07-28", "uuid-1"), pick_cut_shot("2026-07-28", "uuid-1"));
        let mut seen = std::collections::HashSet::new();
        for d in 1..=400 {
            seen.insert(pick_cut_shot(&format!("2026-07-{d}"), "uuid-1").as_str());
        }
        // 400개 표본이면 12변형(레전드 짤 11 + 정경 1)이 모두 등장해야 한다
        assert_eq!(seen.len(), 12, "샷 축 12변형이 모두 나와야 함: {seen:?}");
    }

    #[test]
    fn cut_shot_scene_has_no_mascot() {
        for shot in [
            CutShot::HighAngle,
            CutShot::Haduri,
            CutShot::Jump,
            CutShot::BackView,
            CutShot::Mirror,
            CutShot::PropSwagger,
            CutShot::Drama,
            CutShot::FingerGun,
            CutShot::Sauna,
            CutShot::FourCut,
            CutShot::FoodShot,
        ] {
            assert!(shot.has_mascot(), "{}는 마스코트 컷", shot.as_str());
        }
        assert!(!CutShot::Scene.has_mascot());
    }

    #[test]
    fn decide_cut_reconciliation_table() {
        let d = "2026-07-28";
        // 초기 상태(첫 실행) → 생성
        assert!(decide_cut(d, &CutState::default(), false));
        // 최신 일기 컷 완료 + png 존재 → skip (재실행 멱등)
        let mut done = CutState::default();
        register_attempt(&mut done, d);
        register_success(&mut done, d, "캡션", CutShot::Jump);
        assert!(!decide_cut(d, &done, true));
        // 메타는 완료인데 png가 사라짐 → 재생성 (영구 sprite 폴백 방지)
        assert!(decide_cut(d, &done, false));
        // 오늘 3회 실패 소진 → skip (과금 상한 — png 유무와 무관)
        let mut spent = CutState::default();
        for _ in 0..MAX_CUT_ATTEMPTS_PER_DAY {
            register_attempt(&mut spent, d);
        }
        assert!(!decide_cut(d, &spent, false));
        // 어제 소진했어도 새 일기 날짜 → 생성 (카운터는 register_attempt가 리셋)
        assert!(decide_cut("2026-07-29", &spent, false));
        // 어제 성공 컷이 있고 오늘 일기가 새로 생김 → 생성
        assert!(decide_cut("2026-07-29", &done, true));
    }

    #[test]
    fn register_attempt_resets_counter_on_new_date() {
        let mut s = CutState::default();
        register_attempt(&mut s, "2026-07-28");
        register_attempt(&mut s, "2026-07-28");
        assert_eq!((s.attempt_date.as_str(), s.attempts), ("2026-07-28", 2));
        register_attempt(&mut s, "2026-07-29");
        assert_eq!((s.attempt_date.as_str(), s.attempts), ("2026-07-29", 1));
    }

    #[test]
    fn register_success_updates_display_state_and_roundtrips_json() {
        let mut s = CutState::default();
        register_attempt(&mut s, "2026-07-28");
        register_success(&mut s, "2026-07-28", "밤샘 끝, 뿌듯", CutShot::Scene);
        assert_eq!(s.cut_date.as_deref(), Some("2026-07-28"));
        assert_eq!(s.caption, "밤샘 끝, 뿌듯");
        assert_eq!(s.shot, "scene");
        // 손상 대비 직렬화 왕복 (daily_cut.json 포맷)
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<CutState>(&json).unwrap(), s);
    }

    #[test]
    fn cut_scene_prompt_embeds_diary_framing_and_privacy_rules() {
        let p = build_cut_scene_prompt("오늘은 리팩토링을 했다", CutShot::HighAngle, Some("INTJ"));
        assert!(p.contains("오늘은 리팩토링을 했다"), "일기 본문 포함");
        assert!(p.contains("high-angle selfie"), "샷 프레이밍 포함");
        assert!(p.contains("고유명사"), "전송 수위 금지 지시(ADR 0024) 포함");
        assert!(p.contains("scene_en") && p.contains("caption_ko"), "JSON 출력 계약 포함");
        // 레전드 짤 연출이 프레이밍에 실제 반영되는지 표본 확인
        assert!(build_cut_scene_prompt("일기", CutShot::Jump, None).contains("jump shot"));
        assert!(build_cut_scene_prompt("일기", CutShot::Sauna, None).contains("lamb ears"));
        // 정경 샷은 캐릭터 없는 프레이밍
        let scene = build_cut_scene_prompt("일기", CutShot::Scene, None);
        assert!(scene.contains("WITHOUT any character"));
    }

    #[test]
    fn parse_cut_scene_accepts_clean_and_fenced_json_rejects_garbage() {
        let ok = r#"{"scene_en": "a robot at a desk", "caption_ko": "오늘도 무사히"}"#;
        assert_eq!(
            parse_cut_scene(ok),
            Some(("a robot at a desk".into(), "오늘도 무사히".into()))
        );
        let fenced = "```json\n{\"scene_en\": \"night sky\", \"caption_ko\": \"별 헤는 밤\"}\n```";
        assert_eq!(parse_cut_scene(fenced), Some(("night sky".into(), "별 헤는 밤".into())));
        assert_eq!(parse_cut_scene("그림 그려드릴게요!"), None);
        assert_eq!(parse_cut_scene(r#"{"scene_en": "", "caption_ko": "x"}"#), None);
        assert_eq!(parse_cut_scene(r#"{"scene_en": "x"}"#), None);
    }

    #[test]
    fn cut_image_prompt_composes_style_scene_and_notext() {
        let p = build_cut_image_prompt(CutShot::Haduri, Some("a navy robot"), "coding at night");
        assert!(p.contains("coding at night"), "장면 포함");
        assert!(p.contains("a navy robot"), "마스코트 샷은 캐릭터 묘사 포함");
        assert!(p.contains("black-and-white"), "샷별 구도(하두리 흑백) 포함");
        assert!(p.contains("No text"), "그림 안 텍스트 금지");
        assert!(p.contains("same art style"), "스타일 앵커 문구 포함");
        // 네컷은 분할 구도 + 낙서만 (글자 금지 강조)
        let four = build_cut_image_prompt(CutShot::FourCut, Some("a navy robot"), "four moods");
        assert!(four.contains("2x2") && four.contains("no letters"));
        // 정경 샷: 캐릭터 묘사 없음 + 캐릭터 배제 구도
        let s = build_cut_image_prompt(CutShot::Scene, None, "a quiet desk");
        assert!(!s.contains("The character is"));
        assert!(s.contains("NO characters"));
    }
}
