//! mascot 창 위치 복원 검증 — 모니터 구성 변경으로 화면 밖에 저장된 좌표 방어 (스펙 §6).

/// 창 사각형(x,y,w,h)이 모니터 목록((mx,my,mw,mh)) 중 하나와 유의미하게(중심점 기준) 겹치면 true.
/// false면 호출측이 기본 우하단 배치로 폴백한다.
pub fn sanitize_pos(x: i32, y: i32, w: i32, h: i32, monitors: &[(i32, i32, i32, i32)]) -> bool {
    let (cx, cy) = (x + w / 2, y + h / 2);
    monitors.iter().any(|&(mx, my, mw, mh)| {
        cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
    })
}

#[cfg(test)]
mod tests {
    use super::sanitize_pos;

    #[test]
    fn inside_primary_is_ok() {
        assert!(sanitize_pos(100, 100, 160, 160, &[(0, 0, 1920, 1080)]));
    }

    #[test]
    fn offscreen_after_monitor_removed_is_rejected() {
        // 좌표가 이전 보조 모니터(x=1920~) 영역 — 이제 primary만 남음
        assert!(!sanitize_pos(2200, 300, 160, 160, &[(0, 0, 1920, 1080)]));
        // 음수 영역(왼쪽 보조 제거)도 거부
        assert!(!sanitize_pos(-500, 300, 160, 160, &[(0, 0, 1920, 1080)]));
    }

    #[test]
    fn secondary_monitor_still_ok() {
        assert!(sanitize_pos(2200, 300, 160, 160, &[(0, 0, 1920, 1080), (1920, 0, 1920, 1080)]));
    }
}
