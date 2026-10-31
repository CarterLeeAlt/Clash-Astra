/// 窗口状态转换只处理业务意图，不调用 Win32 或 Tauri，便于验证事件交错。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowPhase {
    Missing,
    Creating,
    Alive,
    UnexpectedDestroyed,
    RecreateBackoff,
}

#[derive(Clone, Debug)]
pub struct WindowLifecycle {
    pub phase: WindowPhase,
    pub generation: u64,
    pub desired_visible: bool,
    pub quitting: bool,
    pub ready_listening: bool,
    pub frontend_ready: bool,
    pub show_request: u64,
    pub pending_show: u64,
    pub showing: Option<u64>,
    pub hide_until: u64,
    pub last_create: u64,
    pub last_destroy: u64,
    pub destroy_window_start: u64,
    pub destroy_count: u64,
    pub heartbeat: u64,
}

impl Default for WindowLifecycle {
    fn default() -> Self {
        Self {
            phase: WindowPhase::Missing,
            generation: 0,
            desired_visible: true,
            quitting: false,
            ready_listening: false,
            frontend_ready: false,
            show_request: 0,
            pending_show: 0,
            showing: None,
            hide_until: 0,
            last_create: 0,
            last_destroy: 0,
            destroy_window_start: 0,
            destroy_count: 0,
            heartbeat: 0,
        }
    }
}

impl WindowLifecycle {
    pub fn request_show(&mut self) -> Option<u64> {
        if self.quitting {
            return None;
        }
        self.hide_until = 0;
        self.desired_visible = true;
        self.show_request += 1;
        self.pending_show = self.show_request;
        Some(self.show_request)
    }

    pub fn request_hide(&mut self, until: u64) {
        self.desired_visible = false;
        self.hide_until = until;
        self.pending_show = 0;
        self.show_request += 1;
    }

    pub fn can_retry_show(&self, request: u64) -> bool {
        !self.quitting && self.desired_visible && self.show_request == request
    }

    pub fn quit(&mut self) {
        self.quitting = true;
        self.request_hide(0);
    }

    pub fn destroys_in_window(&self, now: u64) -> u64 {
        if self.destroy_window_start > 0 && now.saturating_sub(self.destroy_window_start) <= 60_000
        {
            self.destroy_count
        } else {
            0
        }
    }

    pub fn can_create(&mut self, now: u64) -> bool {
        if self.quitting || self.phase == WindowPhase::Creating {
            return false;
        }
        if self.destroys_in_window(now) >= 3 {
            return false;
        }
        if self.phase == WindowPhase::RecreateBackoff {
            self.phase = WindowPhase::Missing;
        }
        self.last_destroy == 0
            || self.last_create == 0
            || now.saturating_sub(self.last_create) >= 3_000
    }

    pub fn begin_create(&mut self, now: u64) -> Option<u64> {
        if !self.can_create(now) {
            return None;
        }
        self.phase = WindowPhase::Creating;
        self.last_create = now;
        self.generation += 1;
        self.frontend_ready = false;
        Some(self.generation)
    }

    pub fn destroyed(&mut self, now: u64, planned: bool) {
        self.generation += 1;
        self.frontend_ready = false;
        self.showing = None;
        self.pending_show = 0;
        self.show_request += 1;
        if planned || self.quitting {
            self.phase = WindowPhase::Missing;
            return;
        }
        if self.destroys_in_window(now) == 0 {
            self.destroy_window_start = now;
            self.destroy_count = 0;
        }
        self.destroy_count += 1;
        self.last_destroy = now;
        self.phase = if self.destroy_count >= 3 {
            WindowPhase::RecreateBackoff
        } else {
            WindowPhase::UnexpectedDestroyed
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_and_quit_invalidate_pending_show_retries() {
        let mut state = WindowLifecycle::default();
        let request = state.request_show().unwrap();
        assert!(state.can_retry_show(request));
        state.request_hide(1500);
        assert!(!state.can_retry_show(request));
        assert_eq!(state.pending_show, 0);
        let request = state.request_show().unwrap();
        state.quit();
        assert!(!state.can_retry_show(request));
        assert!(state.request_show().is_none());
        assert!(state.begin_create(10_000).is_none());
    }

    #[test]
    fn recreation_backoff_expires_and_planned_recreation_is_not_a_crash() {
        let mut state = WindowLifecycle::default();
        state.destroyed(100, true);
        assert_eq!(state.destroy_count, 0);
        for at in [1000, 5000, 9000] {
            state.destroyed(at, false);
        }
        assert!(!state.can_create(10_000));
        assert!(state.can_create(62_000));
        assert_eq!(state.phase, WindowPhase::Missing);
    }

    #[test]
    fn only_one_creation_can_own_a_generation() {
        let mut state = WindowLifecycle::default();
        let first = state.begin_create(100).unwrap();
        assert!(state.begin_create(200).is_none());
        state.destroyed(300, true);
        let second = state.begin_create(400).unwrap();
        assert!(second > first);
    }
}
