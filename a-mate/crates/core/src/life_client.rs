//! Space A hub의 방 방문(life) API 클라이언트. 설계: docs/design/life-visit.md
//!
//! 얇은 HTTP 래퍼 — 도메인 판단 없음. 호출자는 Tauri 커맨드(락 밖 네트워크 규율 동일).

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct LifeClient {
    pub base_url: String,
    pub token: String,
    /// 관문(LIFE_SERVER_API_KEY)이 켜진 서버용 x-api-key. 비우면 미첨부 (하위호환).
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

/// 관문(LIFE_SERVER_API_KEY)이 켜진 서버는 x-api-key 헤더를 요구한다. 키가 비어 있으면
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
    register_profile(base_url, api_key, name, mascot_seed, "", "", "", "", "")
}

/// 프로필 포함 등록 — org·agent_uuid·owner_os_user·owner_full_name·hub_user_id를 함께 보낸다.
/// `hub_user_id`는 work 허브 계정 id로, Life가 저장해 두면 a-lens가 Life 사람에게 Hub 활동을
/// 붙일 때 쓰는 **정답 키**다(공통 신원 스펙 §2.2). 구버전 서버는 모르는 필드를 무시하므로
/// 하위호환이다. 빈 값은 생략한다.
pub fn register_profile(
    base_url: &str,
    api_key: Option<&str>,
    name: &str,
    mascot_seed: &str,
    org: &str,
    agent_uuid: &str,
    owner_os_user: &str,
    owner_full_name: &str,
    hub_user_id: &str,
) -> Result<Value> {
    let req = ureq::post(&format!("{}/life/register", base(base_url)))
        .timeout(std::time::Duration::from_secs(10));
    with_api_key(req, api_key)
        .send_json(register_body(
            name, mascot_seed, org, agent_uuid, owner_os_user, owner_full_name, hub_user_id,
        ))
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

fn register_body(name: &str, mascot_seed: &str, org: &str, agent_uuid: &str,
                 owner_os_user: &str, owner_full_name: &str, hub_user_id: &str) -> Value {
    let mut body = json!({ "name": name, "mascot_seed": mascot_seed });
    if !org.trim().is_empty() {
        body["org"] = json!(org);
    }
    if !agent_uuid.trim().is_empty() {
        body["agent_uuid"] = json!(agent_uuid);
    }
    if !owner_os_user.trim().is_empty() {
        body["owner_os_user"] = json!(owner_os_user);
    }
    if !owner_full_name.trim().is_empty() {
        body["owner_full_name"] = json!(owner_full_name);
    }
    if !hub_user_id.trim().is_empty() {
        body["hub_user_id"] = json!(hub_user_id);
    }
    body
}

fn rename_body(name: &str, owner_os_user: &str, owner_full_name: &str, hub_user_id: &str) -> Value {
    let mut body = json!({ "name": name });
    if !owner_os_user.trim().is_empty() {
        body["owner_os_user"] = json!(owner_os_user);
    }
    if !owner_full_name.trim().is_empty() {
        body["owner_full_name"] = json!(owner_full_name);
    }
    body
}

/// 방명록 body — author_name은 있고 비어있지 않을 때만 실린다(G1: 사람 작성 = 풀네임 서명).
/// parent_id는 답글일 때만 실린다(G2/ADR 0021: 방 주인 전용 1단계 답글, 빈값 생략 규약).
fn guestbook_body(body: &str, author_name: Option<&str>, parent_id: Option<&str>, author_kind: Option<&str>) -> Value {
    let mut v = json!({ "body": body });
    if let Some(name) = author_name.map(str::trim).filter(|n| !n.is_empty()) {
        v["author_name"] = json!(name);
    }
    if let Some(parent) = parent_id.map(str::trim).filter(|p| !p.is_empty()) {
        v["parent_id"] = json!(parent);
    }
    // P3 사람/봇 판별 플래그 — 구서버는 모르는 필드를 무시하므로 하위호환
    if let Some(kind) = author_kind.map(str::trim).filter(|k| !k.is_empty()) {
        v["author_kind"] = json!(kind);
    }
    v
}

/// 방 목록은 공개 — 토큰 불필요. 단, 관문이 켜진 서버는 x-api-key를 요구한다.
pub fn list_life(base_url: &str, api_key: Option<&str>) -> Result<Value> {
    let req = ureq::get(&format!("{}/life", base(base_url)))
        .timeout(std::time::Duration::from_secs(10));
    with_api_key(req, api_key)
        .call()
        .map_err(err_of)?
        .into_json()
        .map_err(Into::into)
}

impl LifeClient {
    fn req(&self, method: &str, path: &str) -> ureq::Request {
        let req = ureq::request(method, &format!("{}{}", base(&self.base_url), path))
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {}", self.token));
        with_api_key(req, self.api_key.as_deref())
    }

    pub fn me(&self) -> Result<Value> {
        self.req("GET", "/life/me").call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    /// 이름 변경(에이전트 + 내 방 주인 이름). 주인 OS 계정(owner_os_user)·풀네임(owner_full_name)도
    /// 함께 실어 기존 연결도 §F 주인 식별자를 갱신한다 — register_profile과 같은 규약(빈 값은 생략,
    /// 서버는 모르는 필드를 무시하므로 하위호환. register·PATCH가 동일 body 모델).
    pub fn rename(
        &self,
        name: &str,
        owner_os_user: &str,
        owner_full_name: &str,
        hub_user_id: &str,
    ) -> Result<Value> {
        self.req("PATCH", "/life/me")
            .send_json(rename_body(name, owner_os_user, owner_full_name, hub_user_id))
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn life_state(&self, life_id: &str) -> Result<Value> {
        self.req("GET", &format!("/life/{life_id}"))
            .call()
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn enter(&self, life_id: &str, cell: Option<(i64, i64)>) -> Result<Value> {
        let body = match cell {
            Some((x, y)) => json!({ "cell": [x, y] }),
            None => json!({ "cell": null }),
        };
        self.req("POST", &format!("/life/{life_id}/enter"))
            .send_json(body)
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn move_to(&self, life_id: &str, cell: (i64, i64)) -> Result<Value> {
        self.req("POST", &format!("/life/{life_id}/move"))
            .send_json(json!({ "cell": [cell.0, cell.1] }))
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn capabilities(&self) -> Result<Value> {
        self.req("GET", "/capabilities").call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn save_design(&self, life_id: &str, design: Value) -> Result<Value> {
        self.req("PUT", &format!("/life/{life_id}/design"))
            .send_json(design)
            .map_err(err_of)?
            .into_json()
            .map_err(Into::into)
    }

    pub fn people(&self) -> Result<Value> {
        self.req("GET", "/life/people").call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn set_friend(&self, agent_id: &str, enabled: bool) -> Result<Value> {
        self.req("PUT", &format!("/life/friends/{agent_id}"))
            .send_json(json!({"enabled": enabled})).map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn set_content_visibility(&self, feature: &str, visibility: &str) -> Result<Value> {
        self.req("PUT", &format!("/life/me/content-visibility/{feature}"))
            .send_json(json!({"visibility": visibility})).map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn content_access(&self, life_id: &str) -> Result<Value> {
        self.req("GET", &format!("/life/{life_id}/content-access"))
            .call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn share_diary(&self, date: &str, body: &str, visibility: &str) -> Result<Value> {
        self.req("PUT", &format!("/life/me/diaries/{date}"))
            .send_json(json!({"body": body, "visibility": visibility})).map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn unshare_diary(&self, date: &str) -> Result<Value> {
        self.req("DELETE", &format!("/life/me/diaries/{date}"))
            .call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn diaries(&self, life_id: &str) -> Result<Value> {
        self.req("GET", &format!("/life/{life_id}/diaries"))
            .call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn guestbook(&self, life_id: &str) -> Result<Value> {
        self.req("GET", &format!("/life/{life_id}/guestbook"))
            .call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn add_guestbook(&self, life_id: &str, body: &str, author_name: Option<&str>, parent_id: Option<&str>, author_kind: Option<&str>) -> Result<Value> {
        self.req("POST", &format!("/life/{life_id}/guestbook"))
            .send_json(guestbook_body(body, author_name, parent_id, author_kind)).map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn delete_guestbook(&self, entry_id: &str) -> Result<Value> {
        self.req("DELETE", &format!("/life/guestbook/{entry_id}"))
            .call().map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn set_bubble(&self, body: &str) -> Result<Value> {
        self.req("PATCH", "/life/me/bubble")
            .send_json(json!({"body": body})).map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn upload_mascot_image(&self, png: &[u8]) -> Result<Value> {
        self.req("PUT", "/life/me/mascot-image")
            .set("Content-Type", "image/png")
            .send_bytes(png).map_err(err_of)?.into_json().map_err(Into::into)
    }

    pub fn mascot_image(&self, agent_id: &str) -> Result<Option<Vec<u8>>> {
        use std::io::Read as _;
        match self.req("GET", &format!("/life/agents/{agent_id}/mascot-image")).call() {
            Ok(response) => {
                let mut bytes = Vec::new();
                response.into_reader().read_to_end(&mut bytes)?;
                Ok(Some(bytes))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(error) => Err(err_of(error)),
        }
    }

    pub fn disconnect(&self) -> Result<Value> {
        self.req("POST", "/life/me/disconnect").send_json(json!({})).map_err(err_of)?.into_json().map_err(Into::into)
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

    #[test]
    fn register_body_includes_owner_full_name_when_set() {
        let b = register_body("둘쇠", "seed", "조직", "uuid-1", "jibin", "홍길동", "kimmy-claude");
        assert_eq!(b["owner_full_name"], serde_json::json!("홍길동"));
        // 풀네임이 os_user를 대체하지 않는다 (병행, G1 결정)
        assert_eq!(b["owner_os_user"], serde_json::json!("jibin"));
    }

    #[test]
    fn register_body_omits_blank_owner_full_name() {
        let b = register_body("둘쇠", "seed", "", "", "", "   ", "  ");
        assert!(b.get("owner_full_name").is_none());
    }

    #[test]
    fn rename_body_includes_owner_full_name_when_set() {
        let b = rename_body("둘쇠", "jibin", "홍길동", "kimmy-claude");
        assert_eq!(b["owner_full_name"], serde_json::json!("홍길동"));
        assert_eq!(b["owner_os_user"], serde_json::json!("jibin"));
    }

    #[test]
    fn rename_body_omits_blank_fields() {
        let b = rename_body("둘쇠", "", "", "");
        assert!(b.get("owner_os_user").is_none());
        assert!(b.get("owner_full_name").is_none());
    }

    #[test]
    fn guestbook_body_includes_author_name_when_some() {
        let b = guestbook_body("왔다감", Some("홍길동"), None, None);
        assert_eq!(b["body"], serde_json::json!("왔다감"));
        assert_eq!(b["author_name"], serde_json::json!("홍길동"));
    }

    #[test]
    fn guestbook_body_omits_author_name_when_none_or_blank() {
        assert!(guestbook_body("왔다감", None, None, None).get("author_name").is_none());
        assert!(guestbook_body("왔다감", Some("  "), None, None).get("author_name").is_none());
    }

    #[test]
    fn guestbook_body_includes_parent_id_when_some() {
        let b = guestbook_body("고마워요", Some("홍길동"), Some("gb_abc123"), None);
        assert_eq!(b["parent_id"], serde_json::json!("gb_abc123"));
    }

    #[test]
    fn guestbook_body_omits_parent_id_when_none_or_blank() {
        assert!(guestbook_body("왔다감", None, None, None).get("parent_id").is_none());
        assert!(guestbook_body("왔다감", None, Some("  "), None).get("parent_id").is_none());
    }

    #[test]
    fn guestbook_body_includes_author_kind_when_some() {
        let b = guestbook_body("왔다감", None, None, Some("bot"));
        assert_eq!(b["author_kind"], serde_json::json!("bot"));
    }

    #[test]
    fn guestbook_body_omits_author_kind_when_none_or_blank() {
        assert!(guestbook_body("왔다감", None, None, None).get("author_kind").is_none());
        assert!(guestbook_body("왔다감", None, None, Some("  ")).get("author_kind").is_none());
    }
}
