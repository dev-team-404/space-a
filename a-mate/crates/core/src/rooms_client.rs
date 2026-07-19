//! Space A hub의 방 방문(rooms) API 클라이언트. 설계: docs/design/room-visit.md
//!
//! 얇은 HTTP 래퍼 — 도메인 판단 없음. 호출자는 Tauri 커맨드(락 밖 네트워크 규율 동일).

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct RoomsClient {
    pub base_url: String,
    pub token: String,
    /// 관문(ROOM_SERVER_API_KEY)이 켜진 서버용 x-api-key. 비우면 미첨부 (하위호환).
    pub api_key: Option<String>,
}

fn err_of(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: Value = resp.into_json().unwrap_or(Value::Null);
            let msg = body
                .pointer("/error/message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let ecode = body
                .pointer("/error/code")
                .and_then(|c| c.as_str())
                .unwrap_or("http_error")
                .to_string();
            anyhow!("{ecode}: {msg} (HTTP {code})")
        }
        other => anyhow!("hub 연결 실패: {other}"),
    }
}

fn base(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// 관문(ROOM_SERVER_API_KEY)이 켜진 서버는 x-api-key 헤더를 요구한다. 키가 비어 있으면
/// (관문 없는 서버) None을 돌려주어 헤더를 붙이지 않는다 — 하위호환. 공백만 있는 값도 무시.
fn api_key_header(api_key: Option<&str>) -> Option<&str> {
    api_key.map(str::trim).filter(|k| !k.is_empty())
}

/// 요청에 x-api-key를 조건부로 첨부한다 (자유 함수·메서드 공용).
fn with_api_key(req: ureq::Request, api_key: Option<&str>) -> ureq::Request {
    match api_key_header(api_key) {
        Some(key) => req.set("x-api-key", key),
        None => req,
    }
}

/// 등록은 토큰이 없는 상태에서 호출된다. mascot_seed = 이 클라이언트의 로봇 시드
/// (어느 방에서든 내 데스크톱 마스코트와 같은 모습으로 보이게).
/// api_key는 관문이 켜진 서버용 — 비우면 미첨부.
pub fn register(base_url: &str, api_key: Option<&str>, name: &str, mascot_seed: &str) -> Result<Value> {
    let req = ureq::post(&format!("{}/rooms/register", base(base_url)))
        .timeout(std::time::Duration::from_secs(10));
    with_api_key(req, api_key)
        .send_json(json!({ "name": name, "mascot_seed": mascot_seed }))
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

/// 방 목록은 공개 — 토큰 불필요. 단, 관문이 켜진 서버는 x-api-key를 요구한다.
pub fn list_rooms(base_url: &str, api_key: Option<&str>) -> Result<Value> {
    let req = ureq::get(&format!("{}/rooms", base(base_url)))
        .timeout(std::time::Duration::from_secs(10));
    with_api_key(req, api_key)
        .call()
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

impl RoomsClient {
    fn req(&self, method: &str, path: &str) -> ureq::Request {
        let req = ureq::request(method, &format!("{}{}", base(&self.base_url), path))
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {}", self.token));
        with_api_key(req, self.api_key.as_deref())
    }

    pub fn me(&self) -> Result<Value> {
        self.req("GET", "/rooms/me").call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn rename(&self, name: &str) -> Result<Value> {
        self.req("PATCH", "/rooms/me")
            .send_json(json!({ "name": name }))
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn room_state(&self, room_id: &str) -> Result<Value> {
        self.req("GET", &format!("/rooms/{room_id}"))
            .call()
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn enter(&self, room_id: &str, cell: Option<(i64, i64)>) -> Result<Value> {
        let body = match cell {
            Some((x, y)) => json!({ "cell": [x, y] }),
            None => json!({ "cell": null }),
        };
        self.req("POST", &format!("/rooms/{room_id}/enter"))
            .send_json(body)
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn move_to(&self, room_id: &str, cell: (i64, i64)) -> Result<Value> {
        self.req("POST", &format!("/rooms/{room_id}/move"))
            .send_json(json!({ "cell": [cell.0, cell.1] }))
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn capabilities(&self) -> Result<Value> {
        self.req("GET", "/capabilities").call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn save_design(&self, room_id: &str, design: Value) -> Result<Value> {
        self.req("PUT", &format!("/rooms/{room_id}/design"))
            .send_json(design)
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_header_present_when_set() {
        assert_eq!(api_key_header(Some("secret")), Some("secret"));
    }

    #[test]
    fn api_key_header_absent_when_none() {
        // 관문 없는 서버 — 키 미설정이면 헤더를 붙이지 않는다 (하위호환)
        assert_eq!(api_key_header(None), None);
    }

    #[test]
    fn api_key_header_absent_when_blank() {
        // 빈/공백 값은 미설정과 동일하게 취급
        assert_eq!(api_key_header(Some("")), None);
        assert_eq!(api_key_header(Some("   ")), None);
    }

    #[test]
    fn api_key_header_trims_surrounding_whitespace() {
        assert_eq!(api_key_header(Some("  secret  ")), Some("secret"));
    }
}
