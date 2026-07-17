use crate::adapter::ClaudeCodeAdapter;
use std::path::{Path, PathBuf};

/// 하나의 Claude 소스 위치(호스트 라벨 + .claude 루트).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSource {
    pub host: String,         // "Windows" | "wsl:Ubuntu-22.04"
    pub claude_root: PathBuf, // .../.claude
}

impl HostSource {
    /// ~/.claude.json (인벤토리 원천) — .claude 의 형제 파일.
    pub fn claude_json(&self) -> PathBuf {
        match self.claude_root.parent() {
            Some(p) => p.join(".claude.json"),
            None => PathBuf::from(".claude.json"),
        }
    }

    /// settings.json (enabledPlugins 원천).
    pub fn settings_json(&self) -> PathBuf {
        self.claude_root.join("settings.json")
    }

    pub fn adapter(&self) -> ClaudeCodeAdapter {
        ClaudeCodeAdapter { root: self.claude_root.clone(), host: self.host.clone() }
    }
}

/// `wsl.exe -l -q` 출력(보통 UTF-16LE)을 문자열로 디코드. BOM 제거.
fn decode_wsl_output(raw: &[u8]) -> String {
    let has_le_bom = raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE;
    let zeros = raw.iter().filter(|&&b| b == 0).count();
    // ASCII UTF-16LE 는 바이트 절반이 0. UTF-8 ASCII 는 0이 거의 없다.
    let likely_utf16 = has_le_bom || (raw.len() >= 4 && zeros * 3 >= raw.len());
    if likely_utf16 {
        let start = if has_le_bom { 2 } else { 0 };
        let u16s: Vec<u16> = raw[start..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    }
}

/// `wsl.exe -l -q` 출력에서 distro 이름을 뽑는다. 순수·테스트 가능.
pub fn parse_wsl_distros(raw: &[u8]) -> Vec<String> {
    decode_wsl_output(raw)
        .lines()
        .map(|l| l.trim().trim_matches(|c| c == '\0' || c == '\u{feff}').trim())
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// `wsl.exe -l -q` 를 실행해 distro 목록을 얻는다. 실패(미설치 등)는 빈 목록.
pub fn wsl_list_distros() -> Vec<String> {
    // status.success() 확인: 실패/미설치 시 wsl.exe가 에러 메시지를 stdout으로 낼 수 있어,
    // 성공한 경우에만 파싱해 에러문이 distro명으로 오파싱되는 것을 막는다.
    match std::process::Command::new("wsl.exe").args(["-l", "-q"]).output() {
        Ok(o) if o.status.success() => parse_wsl_distros(&o.stdout),
        _ => Vec::new(),
    }
}

/// home 베이스 아래 사용자 홈들을 훑어 `.claude/projects` 를 가진 .claude 루트를 반환.
/// username 하드코딩 금지: `home\*` 를 스캔한다. 접근 불가/부재는 빈 목록.
/// (v0 스코프: 일반 사용자 홈만 대상. root 사용자 홈(`/root/.claude`)은 미커버 — 필요 시 후속.)
pub fn find_claude_roots_under(home_base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(home_base) else {
        return out;
    };
    for e in entries.flatten() {
        let root = e.path().join(".claude");
        if root.join("projects").is_dir() {
            out.push(root);
        }
    }
    out
}

/// WSL distro 의 home 베이스 UNC 경로. localhost=true 면 `\\wsl.localhost\`, 아니면 `\\wsl$\`.
fn wsl_home_base(distro: &str, localhost: bool) -> PathBuf {
    let prefix = if localhost { r"\\wsl.localhost" } else { r"\\wsl$" };
    PathBuf::from(format!(r"{prefix}\{distro}\home"))
}

fn windows_claude_root() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok()?;
    Some(PathBuf::from(home).join(".claude"))
}

/// Windows + 모든 WSL distro 의 Claude 소스를 열거.
pub fn enumerate_hosts() -> Vec<HostSource> {
    let mut out = Vec::new();

    if let Some(root) = windows_claude_root() {
        if root.is_dir() {
            out.push(HostSource { host: "Windows".into(), claude_root: root });
        }
    }

    for distro in wsl_list_distros() {
        // \\wsl.localhost\ 우선, 비면 \\wsl$\ 폴백.
        let mut roots = find_claude_roots_under(&wsl_home_base(&distro, true));
        if roots.is_empty() {
            roots = find_claude_roots_under(&wsl_home_base(&distro, false));
        }
        for root in roots {
            out.push(HostSource { host: format!("wsl:{distro}"), claude_root: root });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le_with_bom(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE]; // UTF-16LE BOM
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn parse_wsl_distros_decodes_utf16le_with_bom_and_crlf() {
        // wsl.exe -l -q 는 보통 UTF-16LE + CRLF 를 낸다.
        let raw = utf16le_with_bom("Ubuntu-22.04\r\nDebian\r\n");
        assert_eq!(parse_wsl_distros(&raw), vec!["Ubuntu-22.04", "Debian"]);
    }

    #[test]
    fn parse_wsl_distros_handles_utf8_fallback_and_blanks() {
        let raw = b"Ubuntu-22.04\n\n  \nDebian\n";
        assert_eq!(parse_wsl_distros(raw), vec!["Ubuntu-22.04", "Debian"]);
    }

    #[test]
    fn parse_wsl_distros_empty_input_is_empty() {
        assert!(parse_wsl_distros(b"").is_empty());
    }

    #[test]
    fn find_claude_roots_scans_home_without_username_hardcoding() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        // 사용자 홈 'jayb' 에만 .claude/projects 존재
        std::fs::create_dir_all(home.join("jayb").join(".claude").join("projects")).unwrap();
        // 'root' 는 .claude 없음
        std::fs::create_dir_all(home.join("root")).unwrap();

        let roots = find_claude_roots_under(home);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], home.join("jayb").join(".claude"));
    }

    #[test]
    fn find_claude_roots_missing_base_is_empty() {
        assert!(find_claude_roots_under(Path::new(r"\\wsl.localhost\nope\home")).is_empty());
    }

    #[test]
    fn host_source_derives_sibling_paths() {
        let hs = HostSource {
            host: "Windows".into(),
            claude_root: PathBuf::from(r"C:\Users\jibin\.claude"),
        };
        assert_eq!(hs.claude_json(), PathBuf::from(r"C:\Users\jibin\.claude.json"));
        assert_eq!(hs.settings_json(), PathBuf::from(r"C:\Users\jibin\.claude\settings.json"));
        let a = hs.adapter();
        assert_eq!(a.host, "Windows");
        assert_eq!(a.root, PathBuf::from(r"C:\Users\jibin\.claude"));
    }
}
