//! 사용자 지정 HTTP 헤더 (2026-08-06) — 사내 게이트웨이가 요구하는 신원 헤더
//! (`x-user-id`, `x-dept-name`, `x-service-id` 등)를 LLM 엔진·이미지 모델 호출에 붙인다.
//!
//! 설정 창에서는 **한 줄에 하나씩 `이름: 값`** 으로 적는다. 사내 게이트웨이마다 요구하는
//! 헤더 이름이 달라 필드를 고정하지 않고 자유 입력으로 두었다.
//!
//! ⚠ HTTP 헤더 값은 ASCII만 실을 수 있다. `x-dept-name: s/w개발팀`처럼 한글을 그대로 넣으면
//! ureq이 요청 자체를 거부한다("Bad Header") — 실측 확인(2026-08-06). 그래서 비ASCII 값만
//! **UTF-8 퍼센트 인코딩**해서 보내고, ASCII 값은 손대지 않는다 (`encode_value`).

/// `이름: 값` 줄 목록 → 헤더 쌍. 빈 줄·`#` 주석·콜론 없는 줄·이름이 빈 줄은 버린다.
/// 값에 콜론이 들어갈 수 있으므로(예: URL) **첫 콜론만** 구분자로 쓴다.
/// 사용자가 붙여넣은 따옴표(`"x-user-id": "abc"`)도 실수로 보고 벗겨낸다 — 그대로 두면
/// 서버가 값을 `"abc"`로 받아 인증이 조용히 실패한다.
pub fn parse_headers(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (name, value) = l.split_once(':')?;
            let name = unquote(name.trim());
            let value = unquote(value.trim().trim_end_matches(','));
            if name.is_empty() {
                None
            } else {
                Some((name.to_string(), value.to_string()))
            }
        })
        .collect()
}

/// `.env`는 값에 실제 줄바꿈을 담지 못한다. 환경변수로 넘어온 헤더 목록은 리터럴 `\n`
/// 두 글자를 줄바꿈으로 되돌린 뒤 파싱한다 — 설정 창(진짜 줄바꿈) 경로는 이 변환을 거치지 않는다.
pub fn parse_headers_env(raw: &str) -> Vec<(String, String)> {
    parse_headers(&raw.replace("\\n", "\n"))
}

/// 양끝을 감싼 큰따옴표/작은따옴표 한 겹을 벗긴다. 한쪽만 있으면 그대로 둔다.
fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// HTTP 헤더 이름에 쓸 수 있는 문자인가 (RFC 7230 tchar).
/// 이름이 규격을 벗어나면 요청 자체가 거부되므로 **그 줄만 버린다** —
/// 오타 한 줄 때문에 일기·채팅이 통째로 멈추는 것보다 낫다.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| {
            b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
        })
}

/// 헤더 값이 HTTP 규격(RFC 7230 field-vchar) 안에 있는가 — 공백·탭과 0x21~0x7E.
fn is_plain_ascii_value(v: &str) -> bool {
    v.bytes().all(|b| b == b' ' || b == b'\t' || (0x21..=0x7E).contains(&b))
}

/// 값에 한글 같은 **비ASCII 문자가 있으면 UTF-8 퍼센트 인코딩**해서 돌려준다.
///
/// HTTP 헤더 값은 ASCII가 원칙이고, 이 앱이 쓰는 HTTP 클라이언트(ureq)는 그 밖의 바이트를
/// 아예 거부한다("Bad Header: invalid header ..."). 값을 그대로 보낼 방법이 없으므로,
/// 잃지 않고 되돌릴 수 있는 형태인 퍼센트 인코딩을 택했다 —
/// `s/w개발팀` → `s/w%EA%B0%9C%EB%B0%9C%ED%8C%80` (서버가 UTF-8로 디코드하면 원래 값).
/// ASCII만 있는 값은 **한 글자도 건드리지 않는다**(`service-a`는 그대로 나간다).
pub fn encode_value(value: &str) -> String {
    if is_plain_ascii_value(value) {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() * 2);
    for b in value.bytes() {
        if b == b' ' || b == b'\t' || (0x21..=0x7E).contains(&b) {
            // '%'는 인코딩 결과와 헷갈리지 않도록 함께 이스케이프한다.
            if b == b'%' {
                out.push_str("%25");
            } else {
                out.push(b as char);
            }
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 값에 비ASCII가 섞여 인코딩되어 나갈 헤더 이름 목록 — 설정 화면 안내용.
pub fn encoded_header_names(headers: &[(String, String)]) -> Vec<String> {
    headers
        .iter()
        .filter(|(n, v)| !v.is_empty() && valid_name(n) && !is_plain_ascii_value(v))
        .map(|(n, _)| n.clone())
        .collect()
}

/// 파싱된 헤더를 요청에 얹는다.
/// - 값이 빈 헤더는 보내지 않는다 (서버가 빈 값을 잘못된 신원으로 읽는 것을 막는다).
/// - 이름이 HTTP 규격을 벗어나면 그 줄만 건너뛴다.
/// - 값의 비ASCII는 UTF-8 퍼센트 인코딩한다 (`encode_value` 참고).
pub fn apply_headers(mut req: ureq::Request, headers: &[(String, String)]) -> ureq::Request {
    for (name, value) in headers {
        if !value.is_empty() && valid_name(name) {
            req = req.set(name, &encode_value(value));
        }
    }
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_value_lines() {
        let got = parse_headers("x-user-id: abc\nx-dept-name: s/w개발팀\nx-service-id: service-a");
        assert_eq!(
            got,
            vec![
                ("x-user-id".to_string(), "abc".to_string()),
                ("x-dept-name".to_string(), "s/w개발팀".to_string()),
                ("x-service-id".to_string(), "service-a".to_string()),
            ]
        );
    }

    #[test]
    fn strips_quotes_and_trailing_commas_from_pasted_json_style_lines() {
        // 사내 안내문이 JSON 조각으로 오는 일이 잦다 — 그대로 붙여넣어도 동작해야 한다.
        let got = parse_headers("\"x-user-id\" : \"abc\",\n'x-service-id': 'service-a'");
        assert_eq!(
            got,
            vec![
                ("x-user-id".to_string(), "abc".to_string()),
                ("x-service-id".to_string(), "service-a".to_string()),
            ]
        );
    }

    #[test]
    fn skips_blank_comment_and_malformed_lines() {
        let got = parse_headers("\n# 사내 게이트웨이용\nx-user-id: abc\n콜론없음\n : 이름없음\n");
        assert_eq!(got, vec![("x-user-id".to_string(), "abc".to_string())]);
    }

    #[test]
    fn keeps_colons_inside_the_value() {
        let got = parse_headers("x-origin: https://host:8443/v1");
        assert_eq!(got, vec![("x-origin".to_string(), "https://host:8443/v1".to_string())]);
    }

    #[test]
    fn empty_input_yields_no_headers() {
        assert!(parse_headers("").is_empty());
        assert!(parse_headers("   \n  \n").is_empty());
    }

    #[test]
    fn env_variant_unescapes_literal_backslash_n() {
        // .env 한 줄에 담긴 헤더 목록 — 리터럴 \n을 줄바꿈으로 되돌려 파싱한다.
        let got = parse_headers_env("x-user-id: abc\\nx-service-id: service-a");
        assert_eq!(
            got,
            vec![
                ("x-user-id".to_string(), "abc".to_string()),
                ("x-service-id".to_string(), "service-a".to_string()),
            ]
        );
        // 진짜 줄바꿈도 그대로 동작한다(dotenvy가 이미 풀어준 경우).
        assert_eq!(parse_headers_env("x-user-id: abc"), parse_headers("x-user-id: abc"));
    }

    #[test]
    fn ascii_values_go_out_byte_for_byte() {
        assert_eq!(encode_value("abc"), "abc");
        assert_eq!(encode_value("service-a"), "service-a");
        assert_eq!(encode_value("Bearer sk-1234/xyz+=="), "Bearer sk-1234/xyz+==");
    }

    #[test]
    fn korean_values_are_percent_encoded_as_utf8() {
        // ureq(및 HTTP 규격)는 헤더 값에 비ASCII 바이트를 허용하지 않는다 —
        // 그대로 넣으면 "Bad Header"로 요청 전체가 실패한다(2026-08-06 실측).
        assert_eq!(encode_value("s/w개발팀"), "s/w%EA%B0%9C%EB%B0%9C%ED%8C%80");
        // 되돌릴 수 있어야 한다: 퍼센트 디코드 → 원래 UTF-8
        let enc = encode_value("s/w개발팀");
        let mut bytes = Vec::new();
        let mut it = enc.bytes();
        while let Some(b) = it.next() {
            if b == b'%' {
                let hex: String = [it.next().unwrap() as char, it.next().unwrap() as char].iter().collect();
                bytes.push(u8::from_str_radix(&hex, 16).unwrap());
            } else {
                bytes.push(b);
            }
        }
        assert_eq!(String::from_utf8(bytes).unwrap(), "s/w개발팀");
    }

    #[test]
    fn percent_sign_is_escaped_so_decoding_is_unambiguous() {
        assert_eq!(encode_value("50%할인"), "50%25%ED%95%A0%EC%9D%B8");
    }

    #[test]
    fn reports_which_headers_needed_encoding() {
        let hs = parse_headers("x-user-id: abc\nx-dept-name: s/w개발팀\nx-empty:");
        assert_eq!(encoded_header_names(&hs), vec!["x-dept-name".to_string()]);
    }

    #[test]
    fn header_names_outside_the_spec_are_dropped_not_fatal() {
        // 이름에 공백·콜론 등이 들어가면 요청 자체가 거부되므로 그 줄만 버린다.
        assert!(valid_name("x-user-id"));
        assert!(!valid_name("x user id"));
        assert!(!valid_name(""));
        assert!(!valid_name("한글헤더"));
    }

    #[test]
    fn value_may_be_empty_but_name_survives_parsing() {
        // 파싱은 남기고, 실제 전송(apply_headers)에서 빈 값을 거른다.
        assert_eq!(parse_headers("x-user-id:"), vec![("x-user-id".to_string(), String::new())]);
    }
}
