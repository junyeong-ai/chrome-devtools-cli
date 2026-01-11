use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    #[command(about = "Navigate to URL")]
    Navigate {
        #[arg(help = "URL to navigate to")]
        url: String,
        #[arg(long, help = "Wait condition: load (default), domcontentloaded")]
        wait_for: Option<String>,
    },

    #[command(about = "Reload current page")]
    Reload {
        #[arg(long, help = "Hard reload (clear cache)")]
        hard: bool,
    },

    #[command(about = "Go back in history")]
    Back,

    #[command(about = "Go forward in history")]
    Forward,

    #[command(about = "Stop browser session")]
    Stop,

    #[command(about = "List all open pages")]
    Pages,

    #[command(about = "Select active page")]
    SelectPage {
        #[arg(help = "Page index (0-based)")]
        index: usize,
    },

    #[command(about = "Create new page")]
    NewPage {
        #[arg(help = "Optional URL to navigate")]
        url: Option<String>,
    },

    #[command(about = "Close page by index")]
    ClosePage {
        #[arg(help = "Page index (0-based)")]
        index: usize,
    },

    #[command(about = "Click element")]
    Click {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, default_value = "auto", help = "Mode: auto, cdp, js")]
        mode: String,
    },

    #[command(about = "Hover over element")]
    Hover {
        #[arg(help = "CSS selector")]
        selector: String,
    },

    #[command(about = "Fill input field")]
    Fill {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(help = "Text to fill")]
        text: String,
        #[arg(long, default_value = "auto", help = "Mode: auto, cdp, js")]
        mode: String,
    },

    #[command(about = "Type text with delays")]
    Type {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(help = "Text to type")]
        text: String,
        #[arg(long, help = "Delay between keystrokes (ms)")]
        delay: Option<u64>,
        #[arg(long, default_value = "auto", help = "Mode: auto, cdp, js")]
        mode: String,
    },

    #[command(about = "Press keyboard key")]
    Press {
        #[arg(help = "Key to press (Enter, Tab, Escape, ArrowDown, etc.)")]
        key: String,
    },

    #[command(about = "Scroll element into view")]
    Scroll {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(
            long,
            default_value = "smooth",
            help = "Behavior: smooth, instant, auto"
        )]
        behavior: String,
        #[arg(
            long,
            default_value = "center",
            help = "Block: start, center, end, nearest"
        )]
        block: String,
    },

    #[command(about = "Select option in dropdown")]
    Select {
        #[arg(help = "CSS selector for select element")]
        selector: String,
        #[arg(help = "Value to select")]
        value: Option<String>,
        #[arg(long, help = "Select by index (0-based)")]
        index: Option<usize>,
        #[arg(long, help = "Select by visible text")]
        label: Option<String>,
    },

    #[command(about = "Handle JavaScript dialog")]
    Dialog {
        #[arg(long, help = "Accept the dialog")]
        accept: bool,
        #[arg(long, help = "Dismiss the dialog")]
        dismiss: bool,
        #[arg(long, help = "Text for prompt dialogs")]
        text: Option<String>,
    },

    #[command(about = "Query elements by selector")]
    Query {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, help = "Show only count")]
        count: bool,
        #[arg(long, default_value = "20", help = "Limit results")]
        limit: usize,
    },

    #[command(about = "Inspect element properties")]
    Inspect {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, short = 'a', help = "Include HTML attributes")]
        attributes: bool,
        #[arg(long, help = "Include computed styles")]
        styles: bool,
        #[arg(long, short = 'b', help = "Include bounding box")]
        r#box: bool,
        #[arg(long, short = 'c', help = "Include children summary")]
        children: bool,
        #[arg(long, help = "Include all information")]
        all: bool,
    },

    #[command(about = "Get DOM tree structure")]
    Dom {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, default_value = "3", help = "Tree depth")]
        depth: u32,
    },

    #[command(about = "Get accessibility tree")]
    A11y {
        #[arg(help = "CSS selector (optional)")]
        selector: Option<String>,
        #[arg(long, default_value = "5", help = "Tree depth")]
        depth: u32,
        #[arg(long, short = 'i', help = "Show only interactive elements")]
        interactable: bool,
    },

    #[command(about = "Get event listeners for element")]
    Listeners {
        #[arg(help = "CSS selector")]
        selector: String,
    },

    #[command(about = "Get page HTML")]
    Html {
        #[arg(help = "CSS selector (optional)")]
        selector: Option<String>,
        #[arg(long, help = "Get innerHTML instead of outerHTML")]
        inner: bool,
    },

    #[command(about = "Execute JavaScript expression")]
    Eval {
        #[arg(help = "JavaScript expression")]
        expression: String,
    },

    #[command(about = "Wait for condition")]
    Wait {
        #[arg(help = "Condition: selector, visible, hidden, stable")]
        condition: String,
        #[arg(long, help = "CSS selector")]
        selector: Option<String>,
        #[arg(long, default_value = "30000", help = "Timeout (ms)")]
        timeout: u64,
    },

    #[command(about = "Capture screenshot")]
    Screenshot {
        #[arg(short, long, help = "Output file path")]
        output: PathBuf,
        #[arg(long, help = "Capture full page")]
        full_page: bool,
        #[arg(long, help = "CSS selector to capture")]
        selector: Option<String>,
        #[arg(long, help = "Format: png, jpeg, webp")]
        format: Option<String>,
        #[arg(long, help = "Quality (1-100)")]
        quality: Option<u8>,
    },

    #[command(about = "Export page as PDF")]
    Pdf {
        #[arg(short, long, help = "Output file path")]
        output: PathBuf,
        #[arg(long, default_value = "A4", help = "Format: A4, Letter, Legal")]
        format: String,
        #[arg(long, help = "Landscape orientation")]
        landscape: bool,
        #[arg(long, help = "Print background graphics")]
        print_background: bool,
    },

    #[command(about = "Capture performance trace")]
    Trace {
        #[arg(help = "URL to trace")]
        url: String,

        #[arg(short, long, help = "Output trace file")]
        output: PathBuf,

        #[arg(long, help = "Use user profile session")]
        user_profile: bool,

        #[arg(long, help = "Show browser window", default_value = "true")]
        headless: bool,
    },

    #[command(about = "Analyze performance trace")]
    Analyze {
        #[arg(help = "Trace file to analyze")]
        trace: PathBuf,
    },

    #[command(about = "View console messages")]
    Console {
        #[arg(long, help = "Filter by level: log, debug, info, warning, error")]
        filter: Option<String>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
    },

    #[command(about = "List network requests")]
    Network {
        #[arg(long, help = "Filter by domain")]
        domain: Option<String>,
        #[arg(long, help = "Filter by status code")]
        status: Option<u16>,
    },

    #[command(about = "Manage cookies")]
    Cookies {
        #[command(subcommand)]
        subcommand: CookiesCommand,
    },

    #[command(about = "Access browser storage")]
    Storage {
        #[command(subcommand)]
        subcommand: StorageCommand,
    },

    #[command(about = "Emulate device")]
    Emulate {
        #[arg(help = "Device name")]
        device: String,
    },

    #[command(about = "Set viewport size")]
    Viewport {
        #[arg(help = "Width")]
        width: u32,
        #[arg(help = "Height")]
        height: u32,
        #[arg(long, help = "Pixel ratio")]
        pixel_ratio: Option<f64>,
    },

    #[command(about = "List available devices")]
    Devices {
        #[arg(long, help = "Include custom devices")]
        include_custom: bool,
    },

    #[command(about = "Query saved session data")]
    History {
        #[command(subcommand)]
        subcommand: HistoryCommand,
    },

    #[command(about = "Get current session information")]
    SessionInfo,

    #[command(about = "Configuration management")]
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommand,
    },

    #[command(about = "Daemon server management")]
    Server {
        #[command(subcommand)]
        subcommand: ServerCommand,
    },

    #[command(about = "Session management (daemon mode)")]
    Session {
        #[command(subcommand)]
        subcommand: SessionCommand,
    },

    #[command(about = "Authentication state management (Playwright storageState)")]
    Auth {
        #[command(subcommand)]
        subcommand: AuthCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum HistoryCommand {
    #[command(about = "List saved sessions")]
    List,

    #[command(about = "Show session summary")]
    Show {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
    },

    #[command(about = "Query events from session")]
    Events {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(
            long,
            help = "Filter by type: click, scroll, navigate, input, recording_start, recording_stop"
        )]
        r#type: Option<String>,
        #[arg(long, help = "Start time (ISO8601 or HH:MM)")]
        from: Option<String>,
        #[arg(long, help = "End time (ISO8601 or HH:MM)")]
        to: Option<String>,
        #[arg(long, help = "Last N minutes/hours (e.g., 30m, 2h)")]
        last: Option<String>,
        #[arg(long, help = "Filter by recording ID")]
        recording: Option<String>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
        #[arg(long, help = "Offset for pagination")]
        offset: Option<usize>,
    },

    #[command(about = "Query network requests from session")]
    Network {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(long, help = "Filter by domain")]
        domain: Option<String>,
        #[arg(long, help = "Filter by status code")]
        status: Option<u16>,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Last N minutes/hours")]
        last: Option<String>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
        #[arg(long, help = "Offset for pagination")]
        offset: Option<usize>,
    },

    #[command(about = "Query console messages from session")]
    Console {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(long, help = "Filter by level: debug, info, warning, error")]
        level: Option<String>,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Last N minutes/hours")]
        last: Option<String>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
        #[arg(long, help = "Offset for pagination")]
        offset: Option<usize>,
    },

    #[command(about = "Query page errors from session")]
    Errors {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Last N minutes/hours")]
        last: Option<String>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
    },

    #[command(about = "Query DevTools issues from session")]
    Issues {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
    },

    #[command(about = "List recordings in session")]
    Recordings {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
    },

    #[command(about = "Show recording details")]
    Recording {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(help = "Recording ID")]
        recording_id: String,
        #[arg(long, short, help = "Show frame list")]
        frames: bool,
    },

    #[command(about = "Export to automation script")]
    Export {
        #[arg(help = "Session ID (optional with --user-profile)")]
        session_id: Option<String>,
        #[arg(long, help = "Use current user-profile session")]
        user_profile: bool,
        #[arg(long, help = "Recording ID (uses latest if not specified)")]
        recording: Option<String>,
        #[arg(long, short, default_value = "playwright", help = "Format: playwright")]
        format: String,
        #[arg(long, short, help = "Output file path")]
        output: Option<String>,
    },

    #[command(about = "Delete a session")]
    Delete {
        #[arg(help = "Session ID")]
        session_id: String,
    },

    #[command(about = "Clean old sessions")]
    Clean {
        #[arg(long, help = "Max age (e.g., 7d, 24h)")]
        older_than: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum CookiesCommand {
    #[command(about = "List all cookies")]
    List,

    #[command(about = "Get a specific cookie")]
    Get {
        #[arg(help = "Cookie name")]
        name: String,
    },

    #[command(about = "Set a cookie")]
    Set {
        #[arg(help = "Cookie name")]
        name: String,
        #[arg(help = "Cookie value")]
        value: String,
        #[arg(long, help = "Domain")]
        domain: Option<String>,
        #[arg(long, help = "Path")]
        path: Option<String>,
        #[arg(long, help = "Secure flag")]
        secure: bool,
        #[arg(long, help = "HttpOnly flag")]
        http_only: bool,
    },

    #[command(about = "Delete a cookie")]
    Delete {
        #[arg(help = "Cookie name")]
        name: String,
    },

    #[command(about = "Clear all cookies")]
    Clear,
}

#[derive(Subcommand, Debug, Clone)]
pub enum StorageCommand {
    #[command(about = "List storage keys")]
    List {
        #[arg(long = "session-storage", short = 'S', help = "Use sessionStorage")]
        session_storage: bool,
    },

    #[command(about = "Get storage value")]
    Get {
        #[arg(help = "Key name")]
        key: String,
        #[arg(long = "session-storage", short = 'S', help = "Use sessionStorage")]
        session_storage: bool,
    },

    #[command(about = "Set storage value")]
    Set {
        #[arg(help = "Key name")]
        key: String,
        #[arg(help = "Value")]
        value: String,
        #[arg(long = "session-storage", short = 'S', help = "Use sessionStorage")]
        session_storage: bool,
    },

    #[command(about = "Delete storage key")]
    Delete {
        #[arg(help = "Key name")]
        key: String,
        #[arg(long = "session-storage", short = 'S', help = "Use sessionStorage")]
        session_storage: bool,
    },

    #[command(about = "Clear all storage")]
    Clear {
        #[arg(long = "session-storage", short = 'S', help = "Use sessionStorage")]
        session_storage: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommand {
    #[command(about = "Initialize config file")]
    Init,

    #[command(about = "Show current configuration")]
    Show,

    #[command(about = "Edit configuration file")]
    Edit,

    #[command(about = "Show config file path")]
    Path,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServerCommand {
    #[command(about = "Start daemon server")]
    Start {
        #[arg(long, help = "Socket path")]
        socket: Option<PathBuf>,
    },

    #[command(about = "Stop daemon server")]
    Stop,

    #[command(about = "Show daemon status")]
    Status,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SessionCommand {
    #[command(about = "Create new browser session")]
    Create {
        #[arg(long, default_value = "true", num_args = 0..=1, default_missing_value = "true", value_parser = clap::builder::BoolishValueParser::new())]
        headless: bool,
        #[arg(long)]
        profile: Option<String>,
    },

    #[command(about = "List active sessions")]
    List,

    #[command(about = "Destroy session")]
    Destroy {
        #[arg(help = "Session ID")]
        session_id: String,
    },

    #[command(about = "Show session info")]
    Info {
        #[arg(help = "Session ID")]
        session_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum AuthCommand {
    #[command(about = "Export auth state to Playwright storageState format")]
    Export {
        #[arg(
            short,
            long,
            help = "Output file path (e.g., playwright/.auth/user.json)"
        )]
        output: Option<PathBuf>,
    },

    #[command(about = "Import auth state from Playwright storageState file")]
    Import {
        #[arg(help = "Input storageState file")]
        input: PathBuf,
    },
}
