//! mascot 창 위치 복원 검증 — 모니터 구성 변경으로 화면 밖에 저장된 좌표 방어 (스펙 §6).

/// 저장된 마스코트 창 위치(x,y + 창 크기 w,h)가 쓸 만한지 검사한다.
/// 상호작용 대상은 창 전체가 아니라 **창 우하단의 robot_side 정사각(로봇)** 이므로,
/// 그 정사각이 어느 한 모니터 안에 온전히 들어오는지로 판단한다. 창 중심만 보면
/// 로봇이 화면 밖(오른쪽/아래)으로 밀린 좌표도 통과해 창을 잡거나 끌 수 없다.
/// false면 호출측이 기본 우하단 배치로 폴백한다.
pub fn sanitize_pos(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    robot_side: i32,
    monitors: &[(i32, i32, i32, i32)],
) -> bool {
    // 저장 좌표(DB)·시스템 값이 극단적(예: i32::MAX)이어도 오버플로 패닉이 없도록 i64로 계산.
    let side = robot_side as i64;
    let (rx, ry) = (x as i64 + w as i64 - side, y as i64 + h as i64 - side); // 로봇 정사각 좌상단
    monitors.iter().any(|&(mx, my, mw, mh)| {
        let (mx, my, mw, mh) = (mx as i64, my as i64, mw as i64, mh as i64);
        rx >= mx && ry >= my && rx + side <= mx + mw && ry + side <= my + mh
    })
}

/// 모니터 배치와 DPI 배율을 안정적으로 비교하기 위한 지문.
/// 열거 순서가 달라져도 같은 구성이 되도록 정렬한다.
pub fn display_layout_signature(monitors: &[(i32, i32, i32, i32, u32)]) -> String {
    let mut displays = monitors.to_vec();
    displays.sort_unstable();
    displays
        .iter()
        .map(|(x, y, w, h, scale_milli)| format!("{x},{y},{w},{h},{scale_milli}"))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::{display_layout_signature, sanitize_pos};

    // 실제 상수와 동일: 창 280×280, 로봇 160.
    const W: i32 = 280;
    const H: i32 = 280;
    const R: i32 = 160;

    #[test]
    fn robot_inside_primary_is_ok() {
        // 기본 우하단 배치 근처 — 로봇 정사각이 화면 안
        assert!(sanitize_pos(3104, 1146, W, H, R, &[(0, 0, 3440, 1440)]));
    }

    #[test]
    fn center_on_screen_but_robot_clipped_is_rejected() {
        // 회귀 방지: 창 중심(3374,1325)은 화면 안이지만 로봇 우측이 3534로 3440을 넘어 잘린다.
        // 구버전(중심점 검사)은 통과시켜 마스코트가 구석에 박혀 잡히지 않았다.
        assert!(!sanitize_pos(3214, 1210, W, H, R, &[(0, 0, 3440, 1440)]));
    }

    #[test]
    fn offscreen_after_monitor_removed_is_rejected() {
        // 이전 보조 모니터(x=1920~) 영역 — 이제 primary만 남음
        assert!(!sanitize_pos(2200, 300, W, H, R, &[(0, 0, 1920, 1080)]));
        // 음수 영역(왼쪽 보조 제거)도 거부
        assert!(!sanitize_pos(-500, 300, W, H, R, &[(0, 0, 1920, 1080)]));
    }

    #[test]
    fn secondary_monitor_still_ok() {
        assert!(sanitize_pos(
            2200,
            300,
            W,
            H,
            R,
            &[(0, 0, 1920, 1080), (1920, 0, 1920, 1080)]
        ));
    }

    #[test]
    fn extreme_coords_do_not_overflow_panic() {
        // 손상된 저장 좌표(i32 극단값)에도 패닉 없이 거부해야 한다 — sanitize는 방어 함수다.
        assert!(!sanitize_pos(
            i32::MAX,
            i32::MAX,
            W,
            H,
            R,
            &[(0, 0, 3440, 1440)]
        ));
        assert!(!sanitize_pos(
            i32::MIN,
            i32::MIN,
            W,
            H,
            R,
            &[(0, 0, 3440, 1440)]
        ));
    }

    #[test]
    fn display_layout_signature_is_order_independent() {
        let a = [(0, 0, 1920, 1080, 1000), (1920, 0, 2560, 1440, 1250)];
        let b = [(1920, 0, 2560, 1440, 1250), (0, 0, 1920, 1080, 1000)];
        assert_eq!(display_layout_signature(&a), display_layout_signature(&b));
    }

    #[test]
    fn display_layout_signature_changes_with_scale() {
        assert_ne!(
            display_layout_signature(&[(0, 0, 1920, 1080, 1000)]),
            display_layout_signature(&[(0, 0, 1920, 1080, 1250)])
        );
    }
}
