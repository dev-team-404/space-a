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

impl SpriteConfig {
    /// Engine 설정(OpenRouter 등 OpenAI 호환)을 재사용. 미설정/off면 None.
    pub fn from_env() -> Option<SpriteConfig> {
        if std::env::var("AGENT_MENTOR_SPRITE").map(|v| v == "off").unwrap_or(false) {
            return None;
        }
        let base_url = std::env::var("AGENT_MENTOR_ENGINE_URL").ok()?;
        Some(SpriteConfig {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: std::env::var("AGENT_MENTOR_ENGINE_KEY").unwrap_or_default(),
            model: std::env::var("AGENT_MENTOR_IMAGE_MODEL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "google/gemini-2.5-flash-image".into()),
        })
    }
}

/// 시드 스펙 → 인물 묘사 (v6 슬롯 의미와 동일한 매핑 — 결정론).
pub fn character_description(spec: &crate::mascot::RobotSpec) -> String {
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
    let outfit = match (spec.body as usize) % 6 {
        0 => format!("a {oc} zip-up hoodie with white drawstrings"),
        1 => format!("a {oc} school blazer over a white shirt with a red tie"),
        2 => format!("a plain {oc} t-shirt"),
        3 => format!("a cozy {oc} sweater"),
        4 => format!("a white button-up shirt with {oc} collar and {oc} buttons"),
        _ => format!("{pc} overalls over a white shirt"),
    };
    format!(
        "a chibi pixel-art person: {hc} {hair}, {eyes}, small smile, wearing {outfit}, {pc} pants, white sneakers, {pose}",
        hair = HAIR[(spec.antenna as usize) % 6],
        eyes = EYES[(spec.eyes as usize) % 6],
        pose = POSE[(spec.arms as usize) % 6],
    )
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
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| anyhow!("sprite base64 디코드 실패: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mascot::robot_spec_for;

    #[test]
    fn description_is_deterministic_and_covers_slots() {
        let spec = robot_spec_for("DESKTOP-X|user");
        let d1 = character_description(&spec);
        let d2 = character_description(&spec);
        assert_eq!(d1, d2);
        assert!(d1.contains("chibi pixel-art person"));
        assert!(d1.contains("pants"));
    }

    #[test]
    fn different_seeds_can_differ() {
        let a = character_description(&robot_spec_for("A|a"));
        let b = character_description(&robot_spec_for("B|bbbb"));
        // 시드가 다르면 대부분 묘사가 달라진다 (같을 수도 있으나 이 두 시드는 다름을 고정)
        assert_ne!(a, b);
    }

    #[test]
    fn style_ref_asset_is_bundled() {
        assert!(STYLE_REF_JPG.len() > 10_000, "스타일 앵커 이미지 번들 확인");
    }
}
