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

/// 확장 특성 — 시드 해시의 미사용 바이트(6·7·8)로 성별 표현·피부톤·액세서리를 추가.
/// (mascot::RobotSpec 계약은 d[0..5]만 사용 — 여기서 더 다양해진다)
pub fn extended_traits(identity: &str) -> (&'static str, &'static str, &'static str) {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(identity.as_bytes());
    const GENDER: [&str; 3] = ["boy", "girl", "person"];
    const SKIN: [&str; 4] = [
        "fair skin",
        "light tan skin",
        "warm tan skin",
        "deep brown skin",
    ];
    const ACC: [&str; 6] = [
        "",
        "wearing small round glasses",
        "wearing a baseball cap",
        "with headphones around the neck",
        "with a tiny hairpin",
        "with light freckles",
    ];
    (
        GENDER[(d[6] as usize) % 3],
        SKIN[(d[7] as usize) % 4],
        ACC[(d[8] as usize) % 6],
    )
}

/// 시드 스펙 + 정체성 → 인물 묘사 (v6 슬롯 의미와 동일한 매핑 — 결정론).
pub fn character_description(spec: &crate::mascot::RobotSpec, identity: &str) -> String {
    const HAIR: [&str; 6] = [
        "neat bowl-cut hair with straight bangs",
        "messy tousled hair with a few spiky strands",
        "side-parted hair with a small tuft",
        "curly poofy hair",
        "longer hair reaching the shoulders",
        "short tidy hair with visible forehead",
    ];
    const EYES: [&str; 6] = [
        "dark oval eyes",
        "big round eyes",
        "calm dark oval eyes",
        "sparkling eyes with bright highlights",
        "gentle droopy eyes",
        "cheerful closed smiling eyes",
    ];
    // (의상색, 바지색, 머리색) — content.rs 팔레트와 동일 감각
    const COLORS: [(&str, &str, &str); 8] = [
        ("navy", "grey", "dark brown"),
        ("blue", "blue", "dark brown"),
        ("green", "grey", "black"),
        ("pink", "mauve", "brown"),
        ("purple", "dark grey", "black"),
        ("mustard yellow", "slate", "auburn"),
        ("teal", "grey", "blonde"),
        ("slate grey", "charcoal", "brown"),
    ];
    const POSE: [&str; 6] = [
        "arms relaxed at sides",
        "hands in pockets",
        "one hand raised in a small wave",
        "hands together in front",
        "holding a small green shopping basket",
        "hands behind the back",
    ];
    let (oc, pc, hc) = COLORS[(spec.palette as usize) % 8];
    let (gender, skin, acc) = extended_traits(identity);
    let outfit = match (spec.body as usize) % 6 {
        0 => format!("a {oc} zip-up hoodie with white drawstrings"),
        1 => format!("a {oc} school blazer over a white shirt with a red tie"),
        2 => format!("a plain {oc} t-shirt"),
        3 => format!("a cozy {oc} sweater"),
        4 => format!("a white button-up shirt with {oc} collar and {oc} buttons"),
        _ => format!("{pc} overalls over a white shirt"),
    };
    let acc_part = if acc.is_empty() { String::new() } else { format!(", {acc}") };
    format!(
        "a chibi pixel-art {gender} with {skin}: {hc} {hair}, {eyes}, small smile{acc_part},          wearing {outfit}, {pc} pants, white sneakers, {pose}",
        hair = HAIR[(spec.antenna as usize) % 6],
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
    let desc = character_description(&spec, seed);
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

/// 이미지 생성 — 스타일 앵커 + 인물 묘사. 반환 = PNG 바이트.
pub fn generate(cfg: &SpriteConfig, description: &str) -> Result<Vec<u8>> {
    let ref_b64 = base64::engine::general_purpose::STANDARD.encode(STYLE_REF_JPG);
    let prompt = format!(
        "Using the EXACT same art style as the attached reference image (16-bit pixel art sprite, \
         chibi proportions with large head, clean dark pixel outline, soft cel shading, \
         front-facing full body, centered, plain white background), draw a DIFFERENT character: \
         {description}. Match the reference's pixel density, outline thickness, shading style and \
         proportions exactly. Single character only, no text, no watermark, plain white background."
    );
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
    let resp: serde_json::Value = ureq::post(&format!("{}/chat/completions", cfg.base_url))
        .timeout(std::time::Duration::from_secs(120))
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| anyhow!("sprite 생성 요청 실패: {e}"))?
        .into_json()?;
    let url = resp
        .pointer("/choices/0/message/images/0/image_url/url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("sprite 응답에 이미지 없음"))?;
    let b64 = url
        .split_once(',')
        .map(|(_, b)| b)
        .ok_or_else(|| anyhow!("sprite data URL 형식 아님"))?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| anyhow!("sprite base64 디코드 실패: {e}"))?;
    // 흰 배경 → 투명. 실패해도 캐릭터는 보여야 하므로 원본으로 폴백(무해).
    match make_background_transparent(&png) {
        Ok(t) => Ok(t),
        Err(e) => {
            eprintln!("[sprite] 배경 투명화 실패(원본 사용): {e}");
            Ok(png)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mascot::robot_spec_for;

    #[test]
    fn description_is_deterministic_and_covers_slots() {
        let id = "DESKTOP-X|user";
        let spec = robot_spec_for(id);
        let d1 = character_description(&spec, id);
        let d2 = character_description(&spec, id);
        assert_eq!(d1, d2);
        assert!(d1.contains("chibi pixel-art"));
        assert!(d1.contains("skin"));
        assert!(d1.contains("pants"));
    }

    #[test]
    fn different_seeds_can_differ() {
        let a = character_description(&robot_spec_for("A|a"), "A|a");
        let b = character_description(&robot_spec_for("B|bbbb"), "B|bbbb");
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
    fn extended_traits_add_gender_skin_accessory_axes() {
        // 서로 다른 정체성에서 성별/피부/액세서리 축이 실제로 갈리는지 표본 확인
        let mut genders = std::collections::HashSet::new();
        let mut skins = std::collections::HashSet::new();
        for i in 0..40 {
            let id = format!("HOST-{i}|user{i}");
            let (g, sk, _a) = extended_traits(&id);
            genders.insert(g);
            skins.insert(sk);
        }
        assert!(genders.len() >= 3, "성별 표현 3종 모두 등장");
        assert!(skins.len() >= 3, "피부톤 다양성");
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
}
