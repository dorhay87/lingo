use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use crate::capture::capture_with_timeout;
use crate::types::Lang;

/// Card width from the design; the window adds a transparent halo around the
/// card so the CSS drop shadow has room to render.
pub const CARD_WIDTH: f64 = 380.0;
pub const SHADOW_MARGIN: f64 = 24.0;
pub const MAX_CARD_HEIGHT: f64 = 450.0;
/// Header + source input. A hidden webview can report a bogus zero height
/// before its first paint; never let the window shrink below the empty card.
pub const MIN_CARD_HEIGHT: f64 = 82.0;
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(300);
/// Vertical anchor: card top sits at this fraction of the work area height,
/// launcher-style, so the popup doesn't jump when its height changes.
const VERTICAL_ANCHOR: f64 = 0.30;

#[derive(Clone, serde::Serialize)]
struct PopupShowEvent {
    seed_text: Option<String>,
    src: Lang,
    tgt: Lang,
}

pub fn popup_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("popup")
}

pub fn toggle(app: &AppHandle, with_capture: bool) {
    let Some(window) = popup_window(app) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide(app);
    } else {
        show_at_cursor(app, with_capture);
    }
}

/// A show is in flight from hotkey press until the popup:show emit; further
/// presses inside that window (selection capture can take up to 300ms) are
/// dropped instead of spawning parallel show flows.
static SHOW_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Capture the selection (bounded), center the pre-created hidden window on
/// the monitor the cursor is on, seed the frontend, then show+focus.
/// Capture runs before the window takes focus so the selection isn't lost.
pub fn show_at_cursor(app: &AppHandle, with_capture: bool) {
    if SHOW_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<crate::AppState>();

        let seed_text = if with_capture {
            let capture = state.capture.clone();
            capture_with_timeout(capture, CAPTURE_TIMEOUT).await
        } else {
            None
        };

        let (src, tgt) = {
            let config = state.config.read().expect("config lock poisoned");
            ("auto".to_string(), config.target_lang.clone())
        };
        {
            let mut session = state.session.lock().expect("session lock poisoned");
            session.pinned = false;
            session.src = src.clone();
            session.tgt = tgt.clone();
            session.last_primary = None;
        }

        let Some(window) = popup_window(&app) else {
            SHOW_IN_FLIGHT.store(false, Ordering::SeqCst);
            return;
        };
        position_centered(&app, &window);
        log::debug!(
            "showing popup (seed: {} chars, tgt: {tgt})",
            seed_text.as_deref().map_or(0, str::len)
        );
        // The frontend resets its state and size for this show, then calls
        // popup_ready -> reveal(). Revealing only after that first clean
        // paint is what makes the open feel smooth: no stale-content flash,
        // no mid-resize frame.
        let _ = app.emit("popup:show", PopupShowEvent { seed_text, src, tgt });
        SHOW_IN_FLIGHT.store(false, Ordering::SeqCst);

        // Safety net: if the webview is stalled (e.g. still loading) and the
        // handshake never arrives, show anyway rather than swallow the press.
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if !window.is_visible().unwrap_or(false) {
                log::debug!("popup_ready handshake missed, revealing anyway");
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    });
}

/// Second half of the show handshake: the frontend has painted the fresh
/// state and the window can become visible.
pub fn reveal(app: &AppHandle) {
    if let Some(window) = popup_window(app) {
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = popup_window(app) {
        if window.is_visible().unwrap_or(false) {
            log::debug!("hiding popup");
            let _ = window.hide();
        }
    }
}

/// Blur hides the popup unless pinned.
pub fn hide_on_blur(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let pinned = state.session.lock().expect("session lock poisoned").pinned;
    if !pinned {
        hide(app);
    }
}

/// Center the popup on the monitor the cursor is on: horizontally centered,
/// card top at VERTICAL_ANCHOR of the work area, clamped to stay on screen.
fn position_centered(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|cursor| app.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let Ok(size) = window.outer_size() else {
        return;
    };

    let halo = (SHADOW_MARGIN * scale) as i32;
    let desired_x =
        work_area.position.x + (work_area.size.width as i32 - size.width as i32) / 2;
    // Anchor the visible card's top, not the shadow halo's.
    let desired_y = work_area.position.y
        + (work_area.size.height as f64 * VERTICAL_ANCHOR) as i32
        - halo;

    let (x, y) = clamp_to_work_area(
        desired_x,
        desired_y,
        size.width,
        size.height,
        (work_area.position.x, work_area.position.y),
        (work_area.size.width, work_area.size.height),
        halo,
    );
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Keep the visible card inside the monitor work area. The window itself may
/// overhang by up to `halo` px per edge, since that ring is just shadow.
pub fn clamp_to_work_area(
    desired_x: i32,
    desired_y: i32,
    width: u32,
    height: u32,
    area_pos: (i32, i32),
    area_size: (u32, u32),
    halo: i32,
) -> (i32, i32) {
    let min_x = area_pos.0 - halo;
    let min_y = area_pos.1 - halo;
    let max_x = area_pos.0 + area_size.0 as i32 - width as i32 + halo;
    let max_y = area_pos.1 + area_size.1 as i32 - height as i32 + halo;
    // max() second so windows taller than the work area pin to the top edge.
    (
        desired_x.min(max_x).max(min_x),
        desired_y.min(max_y).max(min_y),
    )
}

/// Frontend measures its card and asks for a matching window height (logical
/// px, card only); Rust adds the shadow halo and clamps to the max height.
pub fn resize(app: &AppHandle, card_height: f64) {
    let Some(window) = popup_window(app) else {
        return;
    };
    log::debug!("resize popup: card {card_height}px");
    let card_height = card_height.clamp(MIN_CARD_HEIGHT, MAX_CARD_HEIGHT);
    let _ = window.set_size(tauri::LogicalSize::new(
        CARD_WIDTH + SHADOW_MARGIN * 2.0,
        card_height + SHADOW_MARGIN * 2.0,
    ));
    keep_in_work_area(app, &window);
}

/// After a resize the bottom edge can cross the monitor edge; nudge back in.
fn keep_in_work_area(app: &AppHandle, window: &WebviewWindow) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let center_x = pos.x + size.width as i32 / 2;
    let center_y = pos.y + size.height as i32 / 2;
    let monitor = app
        .monitor_from_point(center_x as f64, center_y as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work_area = monitor.work_area();
    let halo = (SHADOW_MARGIN * monitor.scale_factor()) as i32;
    let (x, y) = clamp_to_work_area(
        pos.x,
        pos.y,
        size.width,
        size.height,
        (work_area.position.x, work_area.position.y),
        (work_area.size.width, work_area.size.height),
        halo,
    );
    if (x, y) != (pos.x, pos.y) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: ((i32, i32), (u32, u32)) = ((0, 0), (1920, 1040));

    #[test]
    fn leaves_fitting_position_alone() {
        assert_eq!(
            clamp_to_work_area(500, 300, 428, 300, AREA.0, AREA.1, 24),
            (500, 300)
        );
    }

    #[test]
    fn clamps_right_and_bottom_edges() {
        let (x, y) = clamp_to_work_area(1900, 1030, 428, 300, AREA.0, AREA.1, 24);
        assert_eq!(x, 1920 - 428 + 24);
        assert_eq!(y, 1040 - 300 + 24);
    }

    #[test]
    fn clamps_left_and_top_edges() {
        let (x, y) = clamp_to_work_area(-500, -500, 428, 300, AREA.0, AREA.1, 24);
        assert_eq!(x, -24);
        assert_eq!(y, -24);
    }

    #[test]
    fn clamps_on_secondary_monitor_with_negative_origin() {
        // Monitor to the left of primary: origin (-1920, 0).
        let (x, y) = clamp_to_work_area(-30, 500, 428, 300, (-1920, 0), (1920, 1040), 24);
        assert_eq!(x, -428 + 24); // right edge of that monitor
        assert_eq!(y, 500);
    }

    #[test]
    fn oversized_window_pins_to_top_edge() {
        // Taller than the work area: y pins to top; fitting x is untouched.
        let (x, y) = clamp_to_work_area(100, 100, 500, 2000, AREA.0, AREA.1, 0);
        assert_eq!((x, y), (100, 0));
    }
}
