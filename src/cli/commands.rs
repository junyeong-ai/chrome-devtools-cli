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

    #[command(about = "Capture screenshot")]
    Screenshot {
        #[arg(short, long, help = "Output file path")]
        output: PathBuf,
        #[arg(long, help = "Capture full page")]
        full_page: bool,
        #[arg(long, help = "CSS selector to capture")]
        selector: Option<String>,
        #[arg(long, help = "Image format (png, jpeg, webp)")]
        format: Option<String>,
        #[arg(long, help = "JPEG/WebP quality (1-100)")]
        quality: Option<u8>,
    },

    #[command(about = "View console messages")]
    Console {
        #[arg(long, help = "Filter by level (log, debug, info, warning, error)")]
        filter: Option<String>,
        #[arg(long, help = "Limit number of messages")]
        limit: Option<usize>,
    },

    #[command(about = "Capture and save performance trace")]
    Trace {
        #[arg(help = "URL to navigate and trace")]
        url: String,
        #[arg(short, long, help = "Output trace file")]
        output: PathBuf,
        #[arg(long, help = "Trace categories")]
        categories: Option<Vec<String>>,
    },

    #[command(about = "Analyze performance trace file")]
    Analyze {
        #[arg(help = "Trace file to analyze")]
        trace: PathBuf,
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

    #[command(about = "List network requests")]
    Network {
        #[arg(long, help = "Filter by domain")]
        domain: Option<String>,
        #[arg(long, help = "Filter by status code")]
        status: Option<u16>,
    },

    #[command(about = "Click element")]
    Click {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, default_value = "auto", help = "Interaction mode: auto, cdp, js")]
        mode: String,
    },

    #[command(about = "Hover over element")]
    Hover {
        #[arg(help = "CSS selector")]
        selector: String,
    },

    #[command(about = "Press keyboard key")]
    Press {
        #[arg(help = "Key to press (e.g., Enter, Tab, Escape, ArrowDown)")]
        key: String,
    },

    #[command(about = "Fill input field")]
    Fill {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(help = "Text to fill")]
        text: String,
        #[arg(long, default_value = "auto", help = "Interaction mode: auto, cdp, js")]
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
        #[arg(long, default_value = "auto", help = "Interaction mode: auto, cdp, js")]
        mode: String,
    },

    #[command(about = "Handle JavaScript dialog (alert, confirm, prompt)")]
    Dialog {
        #[arg(long, help = "Accept the dialog")]
        accept: bool,
        #[arg(long, help = "Dismiss the dialog")]
        dismiss: bool,
        #[arg(long, help = "Text to enter for prompt dialogs")]
        text: Option<String>,
    },

    #[command(about = "Execute JavaScript expression")]
    Eval {
        #[arg(help = "JavaScript expression to evaluate")]
        expression: String,
    },

    #[command(about = "Wait for condition")]
    Wait {
        #[arg(help = "Condition to wait for (selector, visible, hidden, stable)")]
        condition: String,
        #[arg(long, help = "CSS selector (required for selector/visible/hidden)")]
        selector: Option<String>,
        #[arg(long, default_value = "30000", help = "Timeout in milliseconds")]
        timeout: u64,
    },

    #[command(about = "Configuration management")]
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommand,
    },

    #[command(about = "Stop browser session")]
    Stop,

    #[command(about = "Get current session information")]
    SessionInfo,

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

    #[command(about = "Record browser activity as video")]
    Record {
        #[arg(
            short,
            long,
            help = "Output path (creates .frames directory for JPEG frames, or .mp4 file with --mp4)"
        )]
        output: PathBuf,

        #[arg(
            long,
            help = "Recording duration in seconds (omit for interactive mode - press Ctrl+C to stop)"
        )]
        duration: Option<u64>,

        #[arg(long, default_value = "10", help = "Frames per second")]
        fps: u32,

        #[arg(long, default_value = "80", help = "JPEG quality (0-100)")]
        quality: u8,

        #[arg(long, help = "Track browser activities (navigation, page loads)")]
        track_activity: bool,

        #[arg(long, help = "Encode output as MP4 (requires ffmpeg)")]
        mp4: bool,
    },

    #[command(about = "Manage saved sessions")]
    Sessions {
        #[command(subcommand)]
        subcommand: SessionsCommand,
    },

    #[command(about = "Inspect element properties")]
    Inspect {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, short = 'a', help = "Include all HTML attributes")]
        attributes: bool,
        #[arg(long, short = 's', help = "Include computed styles")]
        styles: bool,
        #[arg(long, short = 'b', help = "Include bounding box")]
        r#box: bool,
        #[arg(long, short = 'c', help = "Include children summary")]
        children: bool,
        #[arg(long, help = "Include all information")]
        all: bool,
    },

    #[command(about = "Get event listeners for element")]
    Listeners {
        #[arg(help = "CSS selector")]
        selector: String,
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

    #[command(about = "Get DOM tree structure")]
    Dom {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(long, default_value = "3", help = "Tree depth")]
        depth: u32,
    },

    #[command(about = "Get accessibility tree")]
    A11y {
        #[arg(help = "CSS selector (optional, defaults to page)")]
        selector: Option<String>,
        #[arg(long, default_value = "5", help = "Tree depth")]
        depth: u32,
        #[arg(long, short = 'i', help = "Show only interactive elements")]
        interactable: bool,
    },

    #[command(about = "Scroll element into view")]
    Scroll {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(
            long,
            default_value = "smooth",
            help = "Scroll behavior (smooth, instant, auto)"
        )]
        behavior: String,
        #[arg(
            long,
            default_value = "center",
            help = "Scroll block position (start, center, end, nearest)"
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

    #[command(about = "Get page HTML")]
    Html {
        #[arg(help = "CSS selector (optional, defaults to document)")]
        selector: Option<String>,
        #[arg(long, help = "Get innerHTML instead of outerHTML")]
        inner: bool,
    },

    #[command(about = "Export page as PDF")]
    Pdf {
        #[arg(short, long, help = "Output file path")]
        output: std::path::PathBuf,
        #[arg(long, default_value = "A4", help = "Page format (A4, Letter, Legal)")]
        format: String,
        #[arg(long, help = "Landscape orientation")]
        landscape: bool,
        #[arg(long, help = "Print background graphics")]
        print_background: bool,
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
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum SessionsCommand {
    #[command(about = "List all saved sessions")]
    List,

    #[command(about = "Show session summary")]
    Show {
        #[arg(help = "Session ID")]
        session_id: String,
    },

    #[command(about = "Query network requests from session")]
    Network {
        #[arg(help = "Session ID")]
        session_id: String,
        #[arg(long, help = "Filter by domain")]
        domain: Option<String>,
        #[arg(long, help = "Filter by status code")]
        status: Option<u16>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
        #[arg(long, help = "Offset for pagination")]
        offset: Option<usize>,
    },

    #[command(about = "Query console messages from session")]
    Console {
        #[arg(help = "Session ID")]
        session_id: String,
        #[arg(long, help = "Filter by level (debug, info, warning, error)")]
        level: Option<String>,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
        #[arg(long, help = "Offset for pagination")]
        offset: Option<usize>,
    },

    #[command(about = "Query page errors from session")]
    Errors {
        #[arg(help = "Session ID")]
        session_id: String,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
    },

    #[command(about = "Query DevTools issues from session")]
    Issues {
        #[arg(help = "Session ID")]
        session_id: String,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
    },

    #[command(about = "Delete a session")]
    Delete {
        #[arg(help = "Session ID")]
        session_id: String,
    },

    #[command(about = "Clean old sessions")]
    Clean {
        #[arg(
            long,
            default_value = "86400",
            help = "Max age in seconds (default: 24h)"
        )]
        older_than: u64,
    },

    #[command(about = "Query extension events from session")]
    Extension {
        #[arg(help = "Session ID")]
        session_id: String,
        #[arg(long, help = "Limit results")]
        limit: Option<usize>,
        #[arg(long, help = "Offset for pagination")]
        offset: Option<usize>,
    },

    #[command(about = "Export session to automation script")]
    Export {
        #[arg(help = "Session ID")]
        session_id: String,
        #[arg(
            long,
            short,
            default_value = "playwright",
            help = "Output format (playwright)"
        )]
        format: String,
        #[arg(long, short, help = "Output file path")]
        output: Option<String>,
    },
}

#[derive(clap::Subcommand, Debug, Clone)]
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
        #[arg(long, help = "Domain (defaults to current page)")]
        domain: Option<String>,
        #[arg(long, help = "Path (defaults to /)")]
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

#[derive(clap::Subcommand, Debug, Clone)]
pub enum StorageCommand {
    #[command(about = "List localStorage keys")]
    List {
        #[arg(
            long = "session-storage",
            short = 'S',
            help = "Use sessionStorage instead"
        )]
        session_storage: bool,
    },

    #[command(about = "Get storage value")]
    Get {
        #[arg(help = "Key name")]
        key: String,
        #[arg(
            long = "session-storage",
            short = 'S',
            help = "Use sessionStorage instead"
        )]
        session_storage: bool,
    },

    #[command(about = "Set storage value")]
    Set {
        #[arg(help = "Key name")]
        key: String,
        #[arg(help = "Value")]
        value: String,
        #[arg(
            long = "session-storage",
            short = 'S',
            help = "Use sessionStorage instead"
        )]
        session_storage: bool,
    },

    #[command(about = "Delete storage key")]
    Delete {
        #[arg(help = "Key name")]
        key: String,
        #[arg(
            long = "session-storage",
            short = 'S',
            help = "Use sessionStorage instead"
        )]
        session_storage: bool,
    },

    #[command(about = "Clear all storage")]
    Clear {
        #[arg(
            long = "session-storage",
            short = 'S',
            help = "Use sessionStorage instead"
        )]
        session_storage: bool,
    },
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum ConfigCommand {
    #[command(about = "Initialize config file with defaults")]
    Init,

    #[command(about = "Show current configuration")]
    Show,

    #[command(about = "Edit configuration file")]
    Edit,

    #[command(about = "Show config file path")]
    Path,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum ServerCommand {
    #[command(about = "Start daemon server")]
    Start {
        #[arg(long, help = "Socket path")]
        socket: Option<std::path::PathBuf>,
    },

    #[command(about = "Stop daemon server")]
    Stop,

    #[command(about = "Show daemon status")]
    Status,
}

#[derive(clap::Subcommand, Debug, Clone)]
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
