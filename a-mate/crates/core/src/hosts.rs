use crate::adapter::ClaudeCodeAdapter;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build a background child process without flashing a console window in the
/// packaged Windows GUI application.
pub(crate) fn background_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

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

    /// 세션 cwd(호스트 관점 절대경로)를 로컬 Windows에서 접근 가능한 경로로 변환.
    /// Windows 호스트는 그대로. WSL 호스트(`wsl:<distro>`)의 리눅스 cwd(`/home/...`)는
    /// distro UNC 경로로 바꾼다 — 그래야 프로젝트 `.claude/skills` 를 실제로 찾을 수 있다.
    /// UNC prefix(`\\wsl.localhost` vs `\\wsl$`)는 claude_root 가 쓴 것과 동일하게 유지한다.
    pub fn resolve_cwd(&self, cwd: &str) -> PathBuf {
        match self.host.strip_prefix("wsl:") {
            None => PathBuf::from(cwd),
            Some(distro) => {
                let localhost = !self.claude_root.to_string_lossy().starts_with(r"\\wsl$");
                wsl_path_to_unc(distro, cwd, localhost)
            }
        }
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
    match background_command("wsl.exe").args(["-l", "-q"]).output() {
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

/// WSL 절대경로(리눅스 cwd)를 Windows에서 접근 가능한 UNC 경로로 변환. 순수·테스트 가능.
/// 예: ("Ubuntu-22.04", "/home/jay/proj", true) → `\\wsl.localhost\Ubuntu-22.04\home\jay\proj`.
pub fn wsl_path_to_unc(distro: &str, wsl_path: &str, localhost: bool) -> PathBuf {
    let prefix = if localhost { r"\\wsl.localhost" } else { r"\\wsl$" };
    let rel = wsl_path.trim_start_matches('/').replace('/', r"\");
    PathBuf::from(format!(r"{prefix}\{distro}\{rel}"))
}

/// 경로의 마지막 세그먼트(basename). '/' 와 '\\' 둘 다 구분자로 취급.
pub fn path_basename(p: &str) -> String {
    p.rsplit(|c| c == '/' || c == '\\')
        .find(|s| !s.is_empty())
        .unwrap_or(p)
        .to_string()
}

/// s 가 prefix(ASCII) 로 시작하면(대소문자 무시) 나머지 슬라이스를 반환.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() >= prefix.len()
        && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes()))
    .then(|| &s[prefix.len()..])
}

/// WSL UNC 경로를 (distro, 리눅스 절대경로)로 되돌린다. `wsl_path_to_unc` 의 역.
/// `\\wsl.localhost\Ubuntu-22.04\home\jay\proj` → ("Ubuntu-22.04", "/home/jay/proj").
/// `\\wsl$\Debian\home\x` 도 지원. UNC 서버명은 대소문자 무시. WSL UNC 가 아니면 None.
pub fn unc_to_wsl_path(p: &str) -> Option<(String, String)> {
    let rest = strip_prefix_ci(p, r"\\wsl.localhost\")
        .or_else(|| strip_prefix_ci(p, r"\\wsl$\"))?;
    let mut it = rest.splitn(2, '\\');
    let distro = it.next().filter(|s| !s.is_empty())?.to_string();
    let tail = it.next().unwrap_or("");
    Some((distro, format!("/{}", tail.replace('\\', "/"))))
}

/// (host, cwd) → (정규화 프로젝트 키, 표시 이름).
/// 같은 프로젝트의 WSL 직접 세션과 Windows(WSL UNC 경로) 세션을 같은 키로 통합한다.
/// - WSL 직접: host=`wsl:<distro>`, cwd=리눅스경로 → key=`wsl:<distro>:<linux>`
/// - Windows(WSL UNC): cwd=`\\wsl.localhost\<distro>\...` → 위와 동일 key 로 통합
/// - 일반 Windows: key=`win:<소문자 경로>` (리눅스 경로 키는 대소문자 유지)
pub fn project_identity(host: &str, cwd: &str) -> (String, String) {
    // distro 는 대소문자 무시로 통합(WSL distro 이름은 case-insensitive), 리눅스 tail 은 케이스 유지.
    if let Some(distro) = host.strip_prefix("wsl:") {
        return (format!("wsl:{}:{cwd}", distro.to_lowercase()), path_basename(cwd));
    }
    if let Some((distro, linux)) = unc_to_wsl_path(cwd) {
        let name = path_basename(&linux);
        return (format!("wsl:{}:{linux}", distro.to_lowercase()), name);
    }
    (format!("win:{}", cwd.to_lowercase()), path_basename(cwd))
}

/// `--git-common-dir` 출력에서 저장소 루트를 유도한다 (순수 — 실행은 `git_repo_root`).
///
/// 링크된 worktree도 공용 디렉터리는 **본 저장소의 `.git`** 이라 같은 루트로 합쳐진다.
/// 서브모듈은 `<super>/.git/modules/<name>` 이 나오므로 합치지 않고 None — 호출부가
/// `--show-toplevel`(서브모듈 자신)로 폴백한다.
pub fn repo_root_from_common_dir(common_dir: &str) -> Option<String> {
    let p = common_dir.trim();
    let norm = p.replace('\\', "/");
    if norm.contains("/.git/modules/") {
        return None;
    }
    let root = norm.strip_suffix("/.git")?;
    (!root.is_empty()).then(|| root.to_string())
}

/// (host, cwd)의 git 저장소 루트 경로. best-effort — git이 없거나 저장소가 아니면 None.
///
/// **프로젝트 이름의 기준**이다: 같은 저장소면 `space-a/`에서 일한 사람과 `space-a/a-mate/`에서
/// 일한 사람이 한 프로젝트로 묶인다. cwd basename으로는 사람마다 다른 이름이 나온다.
/// host가 `wsl:<distro>`면 일기의 커밋 수집과 같은 방식으로 WSL 안에서 실행한다.
pub fn git_repo_root(host: &str, cwd: &str) -> Option<String> {
    let run = |args: &[&str]| -> Option<String> {
        let mut cmd = match host.strip_prefix("wsl:") {
            Some(distro) => {
                let mut c = background_command("wsl");
                c.args(["-d", distro, "--", "git"]);
                c
            }
            None => background_command("git"),
        };
        cmd.args(["-C", cwd])
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    // --path-format 은 git 2.31+ — 없으면 실패하고 아래 --show-toplevel 로 내려간다.
    run(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .and_then(|d| repo_root_from_common_dir(&d))
        .or_else(|| run(&["rev-parse", "--show-toplevel"]))
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
    fn wsl_path_to_unc_maps_linux_cwd_to_distro_unc() {
        assert_eq!(
            wsl_path_to_unc("Ubuntu-22.04", "/home/jay/proj", true).to_string_lossy(),
            r"\\wsl.localhost\Ubuntu-22.04\home\jay\proj"
        );
        assert_eq!(
            wsl_path_to_unc("Debian", "/home/jay/proj", false).to_string_lossy(),
            r"\\wsl$\Debian\home\jay\proj"
        );
    }

    #[test]
    fn resolve_cwd_passes_windows_through_and_converts_wsl() {
        // Windows 호스트: cwd 그대로.
        let win = HostSource { host: "Windows".into(), claude_root: PathBuf::from(r"C:\Users\jay\.claude") };
        assert_eq!(win.resolve_cwd(r"D:\work\proj"), PathBuf::from(r"D:\work\proj"));

        // WSL 호스트(localhost 루트): 리눅스 cwd → distro UNC.
        let wsl = HostSource {
            host: "wsl:Ubuntu-22.04".into(),
            claude_root: PathBuf::from(r"\\wsl.localhost\Ubuntu-22.04\home\jay\.claude"),
        };
        assert_eq!(
            wsl.resolve_cwd("/home/jay/proj").to_string_lossy(),
            r"\\wsl.localhost\Ubuntu-22.04\home\jay\proj"
        );

        // WSL 호스트(\\wsl$ 폴백 루트): 동일 prefix 유지.
        let wsl_dollar = HostSource {
            host: "wsl:Debian".into(),
            claude_root: PathBuf::from(r"\\wsl$\Debian\home\jay\.claude"),
        };
        assert_eq!(
            wsl_dollar.resolve_cwd("/home/jay/proj").to_string_lossy(),
            r"\\wsl$\Debian\home\jay\proj"
        );
    }

    /// Windows 실경로 문자열로 형제 경로 파생을 검증한다.
    /// Windows 전용인 이유: Unix에서는 `\`가 경로 구분자가 아니라
    /// `C:\Users\jibin\.claude`가 단일 컴포넌트로 취급돼 parent()가 다르게 나온다.
    /// 플랫폼 중립 검증은 아래 `host_source_derives_sibling_paths_platform_native`가 담당한다.
    #[cfg(windows)]
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

    /// 같은 파생 규칙을 플랫폼 네이티브 경로로 검증 — 전 플랫폼에서 돈다.
    #[test]
    fn host_source_derives_sibling_paths_platform_native() {
        let home = PathBuf::from("home").join("jibin");
        let hs = HostSource { host: "Windows".into(), claude_root: home.join(".claude") };
        assert_eq!(hs.claude_json(), home.join(".claude.json"));
        assert_eq!(hs.settings_json(), home.join(".claude").join("settings.json"));
        let a = hs.adapter();
        assert_eq!(a.host, "Windows");
        assert_eq!(a.root, home.join(".claude"));
    }

    #[test]
    fn unc_to_wsl_path_reverses_wsl_path_to_unc() {
        assert_eq!(
            unc_to_wsl_path(r"\\wsl.localhost\Ubuntu-22.04\home\jay\proj"),
            Some(("Ubuntu-22.04".to_string(), "/home/jay/proj".to_string()))
        );
        assert_eq!(
            unc_to_wsl_path(r"\\wsl$\Debian\home\x"),
            Some(("Debian".to_string(), "/home/x".to_string()))
        );
        assert_eq!(unc_to_wsl_path(r"D:\work\proj"), None);
    }

    #[test]
    fn project_identity_unifies_wsl_direct_and_windows_unc() {
        let (k1, n1) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/agent-meter");
        let (k2, n2) = project_identity(
            "Windows",
            r"\\wsl.localhost\Ubuntu-22.04\home\jayb\work\agent-meter",
        );
        assert_eq!(k1, k2, "같은 프로젝트는 같은 키로 통합");
        assert_eq!(n1, "agent-meter");
        assert_eq!(n2, "agent-meter");
    }

    #[test]
    fn project_identity_plain_windows_is_case_insensitive() {
        let (k1, n1) = project_identity("Windows", r"D:\Project\space-a");
        let (k2, _) = project_identity("Windows", r"d:\project\space-a");
        assert_eq!(k1, k2, "Windows 경로 키는 대소문자 무시");
        assert_eq!(n1, "space-a");
    }

    #[test]
    fn project_identity_distinct_projects_differ() {
        let (a, _) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/agent-meter");
        let (b, _) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/agenttoolbox");
        assert_ne!(a, b);
    }

    #[test]
    fn repo_root_from_common_dir_strips_the_git_directory() {
        assert_eq!(
            repo_root_from_common_dir("/home/kimmy/core/space-a/.git\n"),
            Some("/home/kimmy/core/space-a".to_string())
        );
        // 링크된 worktree도 공용 디렉터리는 본 저장소 것 — 같은 프로젝트로 합쳐진다.
        assert_eq!(
            repo_root_from_common_dir(r"D:\proj\space-a\.git"),
            Some("D:/proj/space-a".to_string())
        );
        // 서브모듈은 합치지 않는다 — 호출부가 --show-toplevel 로 폴백한다.
        assert_eq!(repo_root_from_common_dir("/home/k/super/.git/modules/lib"), None);
        assert_eq!(repo_root_from_common_dir("/home/k/bare.git"), None);
    }

    #[test]
    fn git_repo_root_is_the_same_from_any_subdirectory() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("space-a");
        let sub = root.join("a-mate").join("crates");
        std::fs::create_dir_all(&sub).unwrap();
        let root_s = root.to_str().unwrap();
        if !Command::new("git")
            .args(["init", "-q", root_s])
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return; // git 없는 환경에서는 건너뛴다 — best-effort 경로다
        }

        let from_root = git_repo_root("Windows", root_s);
        let from_sub = git_repo_root("Windows", sub.to_str().unwrap());
        assert_eq!(from_root, from_sub, "하위 디렉터리에서 일해도 같은 프로젝트여야 한다");
        assert_eq!(
            from_sub.as_deref().map(path_basename),
            Some("space-a".to_string()),
            "프로젝트 이름은 저장소 루트의 이름"
        );
    }

    #[test]
    fn git_repo_root_is_none_outside_a_repository() {
        let tmp = tempfile::tempdir().unwrap();
        // 임시 디렉터리가 어쩌다 어떤 저장소 안에 있으면(/tmp는 아니지만) 이 단언은 의미가 없다.
        if git_repo_root("Windows", tmp.path().to_str().unwrap()).is_some() {
            return;
        }
        assert_eq!(git_repo_root("Windows", tmp.path().to_str().unwrap()), None);
    }

    #[test]
    fn unc_to_wsl_path_is_case_insensitive_on_server() {
        // UNC 서버명(wsl.localhost)은 대소문자 무시 — 대문자 변종도 파싱해야 한다.
        assert_eq!(
            unc_to_wsl_path(r"\\WSL.LOCALHOST\Ubuntu-22.04\home\x"),
            Some(("Ubuntu-22.04".to_string(), "/home/x".to_string()))
        );
    }

    #[test]
    fn project_identity_unifies_across_distro_and_unc_case() {
        // distro 케이스가 다르거나 UNC 서버가 대문자여도 같은 프로젝트로 통합.
        let (k1, _) = project_identity("wsl:Ubuntu-22.04", "/home/jayb/work/x");
        let (k2, _) = project_identity("Windows", r"\\WSL.LOCALHOST\ubuntu-22.04\home\jayb\work\x");
        assert_eq!(k1, k2, "distro·UNC 서버 대소문자 무관 통합");
    }
}
