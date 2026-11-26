use std::fs;
use std::path::PathBuf;

use clipo_lib::models::PopupPosition;
use clipo_lib::windows_mgr::{
    compute_popup_position, resolve_clipoimg_blob, resolve_popup_size, ScreenRect,
};
use uuid::Uuid;

fn assert_within_work_area(pos: (i32, i32), popup_size: (u32, u32), work_area: ScreenRect) {
    let (x, y) = pos;
    let max_x = work_area.x + work_area.width.saturating_sub(popup_size.0) as i32;
    let max_y = work_area.y + work_area.height.saturating_sub(popup_size.1) as i32;
    assert!(
        x >= work_area.x && x <= max_x,
        "x={x} out of work area [{}, {}]",
        work_area.x,
        max_x
    );
    assert!(
        y >= work_area.y && y <= max_y,
        "y={y} out of work area [{}, {}]",
        work_area.y,
        max_y
    );
}

#[test]
fn test_clamp_popup_rect_within_work_area() {
    let work_area = ScreenRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    let popup_size = (440, 520);
    let corner_cursor = (1900, 1060);

    // Cursor near bottom-right corner must clamp strictly inside 1920x1040 work area.
    let pos_cursor = compute_popup_position(
        PopupPosition::Cursor,
        corner_cursor,
        None,
        None,
        work_area,
        popup_size,
    );
    assert_within_work_area(pos_cursor, popup_size, work_area);
    assert_eq!(pos_cursor, (1920 - 440, 1040 - 520));

    // Menu icon near right edge must clamp horizontally and stay inside work area.
    let tray_rect = Some(ScreenRect {
        x: 1850,
        y: 0,
        width: 28,
        height: 24,
    });
    let pos_menu = compute_popup_position(
        PopupPosition::MenuIcon,
        corner_cursor,
        tray_rect,
        None,
        work_area,
        popup_size,
    );
    assert_within_work_area(pos_menu, popup_size, work_area);

    // Menu icon fallback when tray_rect is None uses cursor clamped cleanly.
    let pos_menu_fallback = compute_popup_position(
        PopupPosition::MenuIcon,
        corner_cursor,
        None,
        None,
        work_area,
        popup_size,
    );
    assert_within_work_area(pos_menu_fallback, popup_size, work_area);

    // Screen center centers inside work area.
    let pos_center = compute_popup_position(
        PopupPosition::ScreenCenter,
        corner_cursor,
        tray_rect,
        None,
        work_area,
        popup_size,
    );
    assert_within_work_area(pos_center, popup_size, work_area);
    assert_eq!(pos_center, ((1920 - 440) / 2, (1040 - 520) / 2));

    // Active window positioned off the bottom-right still clamps within work area.
    let active_win = Some(ScreenRect {
        x: 1700,
        y: 800,
        width: 400,
        height: 400,
    });
    let pos_active = compute_popup_position(
        PopupPosition::ActiveWindow,
        corner_cursor,
        tray_rect,
        active_win,
        work_area,
        popup_size,
    );
    assert_within_work_area(pos_active, popup_size, work_area);

    // Multi-monitor negative offset & cursor before work_area origin.
    let left_monitor = ScreenRect {
        x: -1920,
        y: 25,
        width: 1920,
        height: 1055,
    };
    let pos_neg = compute_popup_position(
        PopupPosition::Cursor,
        (-2000, 0),
        None,
        None,
        left_monitor,
        popup_size,
    );
    assert_within_work_area(pos_neg, popup_size, left_monitor);
    assert_eq!(pos_neg, (-1920, 25));

    // Oversized popup (> monitor work area) must not underflow or panic, clamping to work_area origin.
    let tiny_monitor = ScreenRect {
        x: 100,
        y: 50,
        width: 300,
        height: 200,
    };
    let pos_oversized = compute_popup_position(
        PopupPosition::ScreenCenter,
        (250, 150),
        None,
        None,
        tiny_monitor,
        (440, 520),
    );
    assert_eq!(pos_oversized, (100, 50));
}

fn temp_blobs_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("clipo-winmgr-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp blobs dir");
    dir
}

#[test]
fn test_clipoimg_protocol_rejects_path_traversal() {
    let blobs_dir = temp_blobs_dir();
    let sample_png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    fs::write(blobs_dir.join("deadbeef.thumb.png"), &sample_png).expect("write valid png blob");

    // Traversal, encoded traversal, absolute paths, and backslashes must return 400 or 403.
    let bad_uris = [
        "clipoimg://localhost/../secret.png",
        "clipoimg://../secret.png",
        "clipoimg://localhost/%2e%2e/secret.png",
        "clipoimg://localhost/sub/../../secret.png",
        "clipoimg://localhost//etc/passwd",
        "http://clipoimg.localhost/..\\secret.png",
        "clipoimg://localhost/foo\\bar.png",
        "clipoimg://localhost/",
        "clipoimg://localhost",
    ];
    for uri in bad_uris {
        let resp = resolve_clipoimg_blob(&blobs_dir, uri);
        assert!(
            resp.status == 400 || resp.status == 403,
            "expected 400/403 for `{uri}`, got {}",
            resp.status
        );
    }

    // Valid relative path returns 200 with image/png and exact bytes (even with query/fragment).
    let ok_resp = resolve_clipoimg_blob(&blobs_dir, "clipoimg://localhost/deadbeef.thumb.png?v=1#hash");
    assert_eq!(ok_resp.status, 200);
    assert_eq!(ok_resp.content_type, "image/png");
    assert_eq!(ok_resp.body, sample_png);

    // Windows custom protocol host form also resolves valid files.
    let win_resp = resolve_clipoimg_blob(&blobs_dir, "http://clipoimg.localhost/deadbeef.thumb.png");
    assert_eq!(win_resp.status, 200);
    assert_eq!(win_resp.content_type, "image/png");
    assert_eq!(win_resp.body, sample_png);

    // Valid relative filename that does not exist returns 404.
    let missing = resolve_clipoimg_blob(&blobs_dir, "clipoimg://localhost/missing.webp");
    assert_eq!(missing.status, 404);

    let _ = fs::remove_dir_all(&blobs_dir);
}

#[test]
fn test_resolve_popup_size_ignores_zero_dimensions() {
    assert_eq!(resolve_popup_size(None, 1.0), (440, 520));
    assert_eq!(resolve_popup_size(Some((0, 0)), 2.0), (880, 1040));
    assert_eq!(resolve_popup_size(Some((440, 0)), 1.5), (660, 780));
    assert_eq!(resolve_popup_size(Some((0, 520)), 1.0), (440, 520));
    assert_eq!(resolve_popup_size(Some((500, 620)), 2.0), (500, 620));
    assert_eq!(resolve_popup_size(Some((0, 0)), 0.0), (440, 520));
}
