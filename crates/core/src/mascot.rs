//! 마스코트 로봇 절차 생성 — 안정적 ID 해시로 파츠 조합을 결정한다.
//! 파츠 도트 데이터 자체는 프론트(TS)에 있고, 여기선 "어떤 파츠 조합인지"만 결정.
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const ANTENNA_VARIANTS: u8 = 6;
pub const HEAD_VARIANTS: u8 = 6;
pub const EYES_VARIANTS: u8 = 6;
pub const BODY_VARIANTS: u8 = 6;
pub const ARMS_VARIANTS: u8 = 6;
pub const PALETTE_VARIANTS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RobotSpec {
    pub antenna: u8,
    pub head: u8,
    pub eyes: u8,
    pub body: u8,
    pub arms: u8,
    pub palette: u8,
}

/// 호스트명+사용자명 — 계정 없이 사용자마다 안정적인 시드.
pub fn stable_identity() -> String {
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".into());
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    format!("{host}|{user}")
}

pub fn robot_spec_for(identity: &str) -> RobotSpec {
    let d = Sha256::digest(identity.as_bytes());
    RobotSpec {
        antenna: d[0] % ANTENNA_VARIANTS,
        head: d[1] % HEAD_VARIANTS,
        eyes: d[2] % EYES_VARIANTS,
        body: d[3] % BODY_VARIANTS,
        arms: d[4] % ARMS_VARIANTS,
        palette: d[5] % PALETTE_VARIANTS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_deterministic_and_in_range() {
        let a = robot_spec_for("HOSTA|alice");
        let b = robot_spec_for("HOSTA|alice");
        assert_eq!(a, b, "같은 identity → 같은 스펙");
        assert!(a.antenna < ANTENNA_VARIANTS && a.head < HEAD_VARIANTS
            && a.eyes < EYES_VARIANTS && a.body < BODY_VARIANTS
            && a.arms < ARMS_VARIANTS && a.palette < PALETTE_VARIANTS);
    }

    #[test]
    fn different_identity_differs() {
        // SHA-256 기반이므로 이 두 입력은 최소 한 슬롯이 다르다 (사전 확인된 페어).
        assert_ne!(robot_spec_for("HOSTA|alice"), robot_spec_for("HOSTB|bob"));
    }

    #[test]
    fn identity_uses_env_or_fallback() {
        let id = stable_identity();
        assert!(id.contains('|'));
    }
}
