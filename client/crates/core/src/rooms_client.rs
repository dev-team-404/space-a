//! Space A hub의 방 방문(rooms) API 클라이언트. 설계: docs/design/room-visit.md
//!
//! 얇은 HTTP 래퍼 — 도메인 판단 없음. 호출자는 Tauri 커맨드(락 밖 네트워크 규율 동일).

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct RoomsClient {
    pub base_url: String,
    pub token: String,
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

/// 등록은 토큰이 없는 상태에서 호출된다.
pub fn register(base_url: &str, name: &str) -> Result<Value> {
    ureq::post(&format!("{}/rooms/register", base(base_url)))
        .timeout(std::time::Duration::from_secs(10))
        .send_json(json!({ "name": name }))
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

/// 방 목록은 공개 — 토큰 불필요.
pub fn list_rooms(base_url: &str) -> Result<Value> {
    ureq::get(&format!("{}/rooms", base(base_url)))
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

impl RoomsClient {
    fn req(&self, method: &str, path: &str) -> ureq::Request {
        ureq::request(method, &format!("{}{}", base(&self.base_url), path))
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {}", self.token))
    }

    pub fn me(&self) -> Result<Value> {
        self.req("GET", "/rooms/me").call().map_err(err_of)?.into_json().map_err(Into::into)
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
}
