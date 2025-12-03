pub mod ms {
    pub const POLL_INTERVAL: u64 = 100;
    pub const STABILITY_DURATION: u64 = 300;
    pub const NETWORK_IDLE: u64 = 500;
    pub const SELECTOR_TIMEOUT: u64 = 5000;
    pub const CDP_ACTION: u64 = 3000;
    pub const SCREENSHOT_WAIT: u64 = 5000;
    pub const RETRY_DELAY: u64 = 200;
    pub const VIEWPORT_SETTLE: u64 = 50;
    pub const PAGE_LOAD_SETTLE: u64 = 300;
    pub const PAGE_CLOSE_SETTLE: u64 = 100;
    pub const SESSION_CLEANUP_INTERVAL: u64 = 500;
}

pub mod secs {
    pub const DAEMON_STARTUP: u64 = 2;
    pub const HISTORY_NAVIGATION: u64 = 5;
    pub const READY_STATE: u64 = 5;
    pub const EMULATION_RELOAD: u64 = 10;
    pub const NAVIGATION: u64 = 30;
    pub const REQUEST: u64 = 120;
    pub const SESSION_MAX_AGE: u64 = 3600;
    pub const PERFORMANCE_WAIT: u64 = 3;
    pub const PERFORMANCE_TIMEOUT: u64 = 5;
}
