//! notify 파이프라인 순수 함수 — Tauri 무관, 단위 테스트 대상.
//! 실행부(watcher·스케줄)는 agent-mentor-app의 pipeline::runtime 모듈에 있음.

pub enum PipelineMsg {
    FileChanged,
    RunNow,
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "warn" => 2,
        "suggest" => 1,
        _ => 0,
    }
}

/// before: dedup_key→severity 스냅샷. after의 각 항목 중 신규이거나 심각도가 오른 것만.
pub fn diff_findings(
    before: &std::collections::HashMap<String, String>,
    after: &[(String, String)],
) -> Vec<String> {
    after
        .iter()
        .filter(|(k, sev)| match before.get(k) {
            None => true,
            Some(prev) => severity_rank(sev) > severity_rank(prev),
        })
        .map(|(k, _)| k.clone())
        .collect()
}

/// 어제까지 중 다이어리가 없는 날짜들 (오래된 것부터, 최대 lookback일 소급).
pub fn missing_diary_dates(existing: &[String], today: &str, lookback: i64) -> Vec<String> {
    let Ok(today) = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return Vec::new();
    };
    (1..=lookback)
        .rev()
        .map(|i| {
            (today - chrono::Duration::days(i))
                .format("%Y-%m-%d")
                .to_string()
        })
        .filter(|d| !existing.contains(d))
        .collect()
}

/// FileChanged는 window만큼 조용해질 때까지 모았다가 1회 실행. RunNow는 즉시 실행.
/// 채널이 닫히면 (대기 중 burst가 있으면 마저 실행 후) 종료.
pub fn debounce_loop(
    rx: std::sync::mpsc::Receiver<PipelineMsg>,
    window: std::time::Duration,
    mut run: impl FnMut(),
) {
    loop {
        match rx.recv() {
            Ok(PipelineMsg::RunNow) => run(),
            Ok(PipelineMsg::FileChanged) => {
                let mut deadline = std::time::Instant::now() + window;
                let disconnected = loop {
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    match rx.recv_timeout(left) {
                        Ok(PipelineMsg::FileChanged) => {
                            deadline = std::time::Instant::now() + window
                        }
                        Ok(PipelineMsg::RunNow) => break false,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break false,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break true,
                    }
                };
                run();
                if disconnected {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn diff_detects_new_and_escalated_only() {
        let before: HashMap<String, String> = [
            ("a".to_string(), "suggest".to_string()),
            ("b".to_string(), "warn".to_string()),
        ]
        .into();
        let after = vec![
            ("a".to_string(), "warn".to_string()),   // 악화 → 포함
            ("b".to_string(), "warn".to_string()),   // 동일 → 제외
            ("c".to_string(), "info".to_string()),   // 신규 → 포함
        ];
        let mut got = diff_findings(&before, &after);
        got.sort();
        assert_eq!(got, vec!["a", "c"]);
    }

    #[test]
    fn missing_dates_up_to_yesterday_with_lookback() {
        let existing = vec!["2026-07-01".to_string()];
        let got = missing_diary_dates(&existing, "2026-07-03", 3);
        assert_eq!(got, vec!["2026-06-30", "2026-07-02"]); // 오늘(03)은 제외, 01은 존재
    }

    #[test]
    fn debounce_coalesces_bursts_and_runs_once() {
        let (tx, rx) = mpsc::channel();
        for _ in 0..5 {
            tx.send(PipelineMsg::FileChanged).unwrap();
        }
        drop(tx); // 채널 닫힘 → 디바운스 창 소진 후 1회 실행하고 루프 종료
        let mut runs = 0;
        debounce_loop(rx, Duration::from_millis(20), || runs += 1);
        assert_eq!(runs, 1, "burst 5건 → 1회 실행");
    }

    #[test]
    fn run_now_bypasses_debounce() {
        let (tx, rx) = mpsc::channel();
        tx.send(PipelineMsg::RunNow).unwrap();
        drop(tx);
        let mut runs = 0;
        debounce_loop(rx, Duration::from_secs(60), || runs += 1);
        assert_eq!(runs, 1);
    }
}
