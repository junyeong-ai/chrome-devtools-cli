use super::{
    Cli,
    commands::{
        Command, ConfigCommand, CookiesCommand, ServerCommand, SessionCommand, SessionsCommand,
        StorageCommand,
    },
};
use crate::{
    ChromeError, Result,
    client::{DaemonClient, is_daemon_running},
    config::Config,
    handlers, output,
    output::OutputFormatter,
    server::{Daemon, DaemonConfig, default_socket_path},
    timeouts::secs,
};
use serde_json::{Value, json};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;

async fn daemon_request(
    client: &mut DaemonClient,
    method: &str,
    session_id: &str,
    extra: Value,
) -> Result<Value> {
    let mut params = json!({"session_id": session_id});
    if let Value::Object(map) = extra {
        for (k, v) in map {
            params[k] = v;
        }
    }
    client.request(method, params).await
}

fn get_socket_path(config: &Config) -> std::path::PathBuf {
    config
        .server
        .socket_path
        .clone()
        .unwrap_or_else(default_socket_path)
}

fn print_json(result: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(result)?);
    Ok(())
}

fn print_json_or(result: &Value, json_output: bool, text: &str) -> Result<()> {
    if json_output {
        print_json(result)
    } else {
        println!("{}", text);
        Ok(())
    }
}

fn start_daemon_background() -> Result<()> {
    let exe = std::env::current_exe()
        .map_err(|e| ChromeError::General(format!("Failed to get executable path: {}", e)))?;

    ProcessCommand::new(exe)
        .args(["server", "start"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ChromeError::General(format!("Failed to spawn daemon: {}", e)))?;

    Ok(())
}

pub async fn dispatch(mut cli: Cli, config: Arc<Config>) -> Result<()> {
    let command = match cli.command.take() {
        Some(cmd) => cmd,
        None => {
            eprintln!("No command provided. Use --help for usage.");
            std::process::exit(1);
        }
    };

    match command {
        Command::Server { subcommand } => handle_server_command(subcommand, &config).await,
        Command::Session { subcommand } => handle_session_command(subcommand, &cli, &config).await,
        Command::Config { subcommand } => handle_config_command(subcommand, &cli).await,
        Command::Sessions { subcommand } => handle_sessions_command(subcommand, &cli).await,
        Command::Devices { .. } => handle_devices_command(&cli, &config).await,
        Command::Analyze { trace } => {
            let result = handlers::performance::handle_analyze(&trace)?;
            output::print_output(&result, cli.json, config.output.json_pretty)
        }
        _ => handle_browser_command(command, cli, config).await,
    }
}

async fn handle_server_command(subcommand: ServerCommand, config: &Arc<Config>) -> Result<()> {
    match subcommand {
        ServerCommand::Start { socket } => {
            let socket_path = socket.unwrap_or_else(|| get_socket_path(config));

            if is_daemon_running(&socket_path) {
                println!("Daemon already running at {}", socket_path.display());
                return Ok(());
            }

            let extension_path = config.browser.extension_path.clone();

            let daemon_config = DaemonConfig {
                socket_path,
                http_port: crate::server::DEFAULT_HTTP_PORT,
                extension_path,
            };

            let daemon = Daemon::new(Arc::clone(config), daemon_config);
            println!("Starting daemon server...");
            daemon.run().await
        }

        ServerCommand::Stop => {
            let socket_path = get_socket_path(config);

            if !is_daemon_running(&socket_path) {
                println!("Daemon not running");
                return Ok(());
            }

            let mut client = DaemonClient::connect(&socket_path).await?;
            client.shutdown_daemon().await?;
            println!("Daemon stopped");
            Ok(())
        }

        ServerCommand::Status => {
            let socket_path = get_socket_path(config);

            if is_daemon_running(&socket_path) {
                let mut client = DaemonClient::connect(&socket_path).await?;
                if client.ping().await.is_ok() {
                    println!("Daemon running at {}", socket_path.display());
                    let sessions = client.list_sessions().await?;
                    println!("Active sessions: {}", sessions.len());
                    return Ok(());
                }
            }

            println!("Daemon not running");
            Ok(())
        }
    }
}

async fn handle_session_command(
    subcommand: SessionCommand,
    cli: &Cli,
    config: &Config,
) -> Result<()> {
    let socket_path = get_socket_path(config);

    if !is_daemon_running(&socket_path) {
        eprintln!("Starting daemon...");
        start_daemon_background()?;
        tokio::time::sleep(std::time::Duration::from_secs(secs::DAEMON_STARTUP)).await;

        if !is_daemon_running(&socket_path) {
            return Err(crate::ChromeError::Connection(
                "Failed to start daemon".into(),
            ));
        }
    }

    let mut client = DaemonClient::connect(&socket_path).await?;

    match subcommand {
        SessionCommand::Create { headless, profile } => {
            let session_id = client
                .create_session_with_profile(headless, profile)
                .await?;
            if cli.json {
                println!(r#"{{"session_id":"{}"}}"#, session_id);
            } else {
                println!("Created session: {}", session_id);
            }
        }

        SessionCommand::List => {
            let sessions = client.list_sessions().await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else if sessions.is_empty() {
                println!("No active sessions");
            } else {
                println!("Active Sessions:");
                for session in sessions {
                    let id = session
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let port = session
                        .get("cdp_port")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let pages = session
                        .get("page_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let headless = session
                        .get("headless")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    println!(
                        "  {} (port: {}, pages: {}, headless: {})",
                        id, port, pages, headless
                    );
                }
            }
        }

        SessionCommand::Destroy { session_id } => {
            client.destroy_session(&session_id).await?;
            println!("Destroyed session: {}", session_id);
        }

        SessionCommand::Info { session_id } => {
            let result = client
                .request("session.get", json!({"session_id": session_id}))
                .await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Session: {}", session_id);
                if let Some(port) = result.get("cdp_port") {
                    println!("  CDP Port: {}", port);
                }
                if let Some(pages) = result.get("page_count") {
                    println!("  Pages: {}", pages);
                }
                if let Some(headless) = result.get("headless") {
                    println!("  Headless: {}", headless);
                }
            }
        }
    }

    Ok(())
}

async fn handle_config_command(subcommand: ConfigCommand, cli: &Cli) -> Result<()> {
    match subcommand {
        ConfigCommand::Init => {
            let result = handlers::config_handler::handle_config_init()?;
            output::print_output(&result, cli.json, true)
        }
        ConfigCommand::Show => {
            let config = crate::config::Config::load()?;
            let result = handlers::config_handler::handle_config_show(&config)?;
            output::print_output(&result, cli.json, true)
        }
        ConfigCommand::Edit => {
            let result = handlers::config_handler::handle_config_edit()?;
            output::print_output(&result, cli.json, true)
        }
        ConfigCommand::Path => {
            let result = handlers::config_handler::handle_config_path()?;
            output::print_output(&result, cli.json, true)
        }
    }
}

async fn handle_sessions_command(subcommand: SessionsCommand, cli: &Cli) -> Result<()> {
    match subcommand {
        SessionsCommand::List => {
            let result = handlers::sessions::handle_list()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for session in &result.sessions {
                    println!("{}", session.session_id);
                }
            }
        }
        SessionsCommand::Show { session_id } => {
            let result = handlers::sessions::handle_show(&session_id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{:?}", result);
            }
        }
        SessionsCommand::Network {
            session_id,
            domain,
            status,
            limit,
            offset,
        } => {
            let result = handlers::sessions::handle_network(
                &session_id,
                domain.as_deref(),
                status,
                limit,
                offset,
            )?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for req in &result.items {
                    println!("{} {} - {}", req.method, req.status.unwrap_or(0), req.url);
                }
            }
        }
        SessionsCommand::Console {
            session_id,
            level,
            limit,
            offset,
        } => {
            let result =
                handlers::sessions::handle_console(&session_id, level.as_deref(), limit, offset)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for msg in &result.items {
                    println!("[{}] {}", msg.level, msg.text);
                }
            }
        }
        SessionsCommand::Errors { session_id, limit } => {
            let result = handlers::sessions::handle_errors(&session_id, limit)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for err in &result.items {
                    println!("{}", err.message);
                }
            }
        }
        SessionsCommand::Issues { session_id, limit } => {
            let result = handlers::sessions::handle_issues(&session_id, limit)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for issue in &result.items {
                    println!("[{}] {:?}", issue.code, issue.details);
                }
            }
        }
        SessionsCommand::Delete { session_id } => {
            handlers::sessions::handle_delete(&session_id)?;
            println!("Deleted session: {}", session_id);
        }
        SessionsCommand::Clean { older_than } => {
            let result = handlers::sessions::handle_clean(older_than)?;
            println!("Cleaned {} sessions", result.removed);
        }
        SessionsCommand::Extension {
            session_id,
            limit,
            offset,
        } => {
            let result = handlers::sessions::handle_extension(&session_id, limit, offset)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Extension events ({}/{})", result.items.len(), result.total);
                for event in &result.items {
                    println!("  {:?}", event);
                }
            }
        }
        SessionsCommand::Export {
            session_id,
            format,
            output,
        } => {
            let result = handlers::export::handle_export(&session_id, &format, output)?;
            if cli.json {
                println!("{}", result.format_json(true)?);
            } else {
                println!("{}", result.format_text());
            }
        }
    }
    Ok(())
}

async fn handle_devices_command(cli: &Cli, _config: &Config) -> Result<()> {
    let devices = crate::devices::list_all_devices(false)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&devices)?);
    } else {
        println!("\nAvailable Devices\n─────────────────");
        for device in &devices {
            println!(
                "  {} - {}x{} @ {}x",
                device.name, device.width, device.height, device.pixel_ratio
            );
        }
    }
    Ok(())
}

async fn handle_browser_command(command: Command, cli: Cli, config: Arc<Config>) -> Result<()> {
    let socket_path = get_socket_path(&config);

    if !is_daemon_running(&socket_path) {
        start_daemon_background()?;
        tokio::time::sleep(std::time::Duration::from_secs(secs::DAEMON_STARTUP)).await;

        if !is_daemon_running(&socket_path) {
            return Err(ChromeError::General("Failed to start daemon".to_string()));
        }
    }

    let mut client = DaemonClient::connect(&socket_path).await?;

    let (session_id, is_new) = if let Some(ref sid) = cli.session {
        (sid.clone(), false)
    } else {
        let headless = cli.headless.unwrap_or(!cli.user_profile);
        let sid = if cli.user_profile {
            client.get_or_create_user_profile_session(headless).await?
        } else {
            client.create_session(headless).await?
        };
        (sid, true)
    };

    if is_new && !cli.json {
        eprintln!("[session: {}]", session_id);
    }

    handle_via_daemon(command, &cli, &config, &session_id).await
}

async fn handle_via_daemon(
    command: Command,
    cli: &Cli,
    config: &Config,
    session_id: &str,
) -> Result<()> {
    let socket_path = get_socket_path(config);
    let mut client = DaemonClient::connect(&socket_path).await?;

    match command {
        Command::Navigate { url, .. } => {
            let result = client
                .request(
                    "navigate",
                    json!({
                        "session_id": session_id,
                        "url": url
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
                println!("Navigated to: {} ({})", url, title);
            }
        }

        Command::Reload { hard } => {
            let result = client
                .request(
                    "reload",
                    json!({
                        "session_id": session_id,
                        "hard": hard
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Page reloaded{}", if hard { " (hard)" } else { "" });
            }
        }

        Command::Back => {
            let result = daemon_request(&mut client, "back", session_id, json!({})).await?;
            print_json_or(&result, cli.json, "Navigated back")?;
        }

        Command::Forward => {
            let result = daemon_request(&mut client, "forward", session_id, json!({})).await?;
            print_json_or(&result, cli.json, "Navigated forward")?;
        }

        Command::Screenshot {
            output: out,
            full_page,
            ..
        } => {
            let result = client
                .request(
                    "screenshot",
                    json!({
                        "session_id": session_id,
                        "full_page": full_page
                    }),
                )
                .await?;

            let base64_data = result
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ChromeError::General("Invalid response".into()))?;

            let data =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data)
                    .map_err(|e| ChromeError::General(e.to_string()))?;

            std::fs::write(&out, data)?;

            if cli.json {
                println!(r#"{{"path":"{}"}}"#, out.display());
            } else {
                println!("Screenshot saved to: {}", out.display());
            }
        }

        Command::Click { selector, .. } => {
            let result = daemon_request(
                &mut client,
                "click",
                session_id,
                json!({"selector": selector}),
            )
            .await?;
            print_json_or(&result, cli.json, &format!("Clicked: {}", selector))?;
        }

        Command::Hover { selector } => {
            let result = daemon_request(
                &mut client,
                "hover",
                session_id,
                json!({"selector": selector}),
            )
            .await?;
            print_json_or(&result, cli.json, &format!("Hovered: {}", selector))?;
        }

        Command::Fill { selector, text, .. } => {
            let result = daemon_request(
                &mut client,
                "fill",
                session_id,
                json!({"selector": selector, "text": text}),
            )
            .await?;
            print_json_or(
                &result,
                cli.json,
                &format!("Filled '{}' into: {}", text, selector),
            )?;
        }

        Command::Type {
            selector,
            text,
            delay,
            ..
        } => {
            let result = daemon_request(
                &mut client,
                "type",
                session_id,
                json!({"selector": selector, "text": text, "delay": delay.unwrap_or(50)}),
            )
            .await?;
            print_json_or(
                &result,
                cli.json,
                &format!("Typed '{}' into: {}", text, selector),
            )?;
        }

        Command::Press { key } => {
            let result =
                daemon_request(&mut client, "press", session_id, json!({"key": key})).await?;
            print_json_or(&result, cli.json, &format!("Pressed: {}", key))?;
        }

        Command::Eval { expression } => {
            let result = client
                .request(
                    "eval",
                    json!({
                        "session_id": session_id,
                        "expression": expression
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let value = result.get("result").unwrap_or(&serde_json::Value::Null);
                println!("Result: {}", value);
            }
        }

        Command::Wait {
            condition,
            selector,
            timeout,
        } => {
            let result = client
                .request(
                    "wait",
                    json!({
                        "session_id": session_id,
                        "condition": condition,
                        "selector": selector,
                        "timeout": timeout
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Wait completed: {}", condition);
            }
        }

        Command::Console { filter, limit } => {
            let result = client
                .request(
                    "console",
                    json!({
                        "session_id": session_id,
                        "filter": filter,
                        "limit": limit
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(messages) = result.get("messages").and_then(|m| m.as_array()) {
                for msg in messages {
                    let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("?");
                    let text = msg.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    println!("[{}] {}", level, text);
                }
            }
        }

        Command::Network { domain, status } => {
            let result = client
                .request(
                    "network",
                    json!({
                        "session_id": session_id,
                        "domain": domain,
                        "status": status
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(requests) = result.get("requests").and_then(|r| r.as_array()) {
                for req in requests {
                    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("?");
                    let status = req.get("status").and_then(|s| s.as_u64()).unwrap_or(0);
                    let url = req.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    println!("{} {} - {}", method, status, url);
                }
            }
        }

        Command::Emulate { device } => {
            let result = client
                .request(
                    "emulate",
                    json!({
                        "session_id": session_id,
                        "device": device
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let w = result.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
                let h = result.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("Emulating: {} ({}x{})", device, w, h);
            }
        }

        Command::Viewport {
            width,
            height,
            pixel_ratio,
        } => {
            let result = client
                .request(
                    "viewport",
                    json!({
                        "session_id": session_id,
                        "width": width,
                        "height": height,
                        "pixel_ratio": pixel_ratio.unwrap_or(1.0)
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Viewport set: {}x{}", width, height);
            }
        }

        Command::Dialog {
            accept,
            dismiss,
            text,
        } => {
            let result = client
                .request(
                    "dialog",
                    json!({
                        "session_id": session_id,
                        "accept": accept && !dismiss,
                        "text": text
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let action = result
                    .get("action")
                    .and_then(|a| a.as_str())
                    .unwrap_or("handled");
                println!("Dialog {}", action);
            }
        }

        Command::Pages => {
            let result = client
                .request(
                    "page.list",
                    json!({
                        "session_id": session_id
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(pages) = result.get("pages").and_then(|p| p.as_array()) {
                println!("Pages:");
                for page in pages {
                    let idx = page.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                    let url = page
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("about:blank");
                    let active = page
                        .get("active")
                        .and_then(|a| a.as_bool())
                        .unwrap_or(false);
                    let marker = if active { "*" } else { " " };
                    println!("  {} [{}] {}", marker, idx, url);
                }
            }
        }

        Command::NewPage { url } => {
            let result = client
                .request(
                    "page.new",
                    json!({
                        "session_id": session_id,
                        "url": url
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let idx = result.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                println!("Created page: {}", idx);
            }
        }

        Command::SelectPage { index } => {
            client
                .request(
                    "page.select",
                    json!({
                        "session_id": session_id,
                        "index": index
                    }),
                )
                .await?;

            if !cli.json {
                println!("Selected page: {}", index);
            }
        }

        Command::ClosePage { index } => {
            client
                .request(
                    "page.close",
                    json!({
                        "session_id": session_id,
                        "index": index
                    }),
                )
                .await?;

            if !cli.json {
                println!("Closed page: {}", index);
            }
        }

        Command::SessionInfo => {
            let result = client
                .request(
                    "session.get",
                    json!({
                        "session_id": session_id
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Session: {}", session_id);
                if let Some(port) = result.get("cdp_port") {
                    println!("  CDP Port: {}", port);
                }
                if let Some(pages) = result.get("page_count") {
                    println!("  Pages: {}", pages);
                }
            }
        }

        Command::Stop => {
            client
                .request(
                    "session.destroy",
                    json!({
                        "session_id": session_id
                    }),
                )
                .await?;

            if !cli.json {
                println!("Session destroyed: {}", session_id);
            }
        }

        Command::Inspect {
            selector,
            attributes,
            styles,
            r#box,
            children,
            all,
        } => {
            let result = client
                .request(
                    "inspect",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "attributes": all || attributes,
                        "styles": all || styles,
                        "box": all || r#box,
                        "children": all || children
                    }),
                )
                .await?;
            print_json(&result)?;
        }

        Command::Listeners { selector } => {
            let result = client
                .request(
                    "listeners",
                    json!({
                        "session_id": session_id,
                        "selector": selector
                    }),
                )
                .await?;
            print_json(&result)?;
        }

        Command::Query {
            selector,
            count,
            limit,
        } => {
            let result = client
                .request(
                    "query",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "count": count,
                        "limit": limit
                    }),
                )
                .await?;
            print_json(&result)?;
        }

        Command::Dom { selector, depth } => {
            let result = client
                .request(
                    "dom",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "depth": depth
                    }),
                )
                .await?;
            print_json(&result)?;
        }

        Command::A11y {
            selector,
            depth,
            interactable,
        } => {
            let result = client
                .request(
                    "a11y",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "depth": depth,
                        "interactable": interactable
                    }),
                )
                .await?;
            print_json(&result)?;
        }

        Command::Scroll {
            selector,
            behavior,
            block,
        } => {
            let result = client
                .request(
                    "scroll",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "behavior": behavior,
                        "block": block
                    }),
                )
                .await?;
            print_json_or(&result, cli.json, &format!("Scrolled to: {}", selector))?;
        }

        Command::Select {
            selector,
            value,
            index,
            label,
        } => {
            let result = client
                .request(
                    "select",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "value": value,
                        "index": index,
                        "label": label
                    }),
                )
                .await?;
            print_json(&result)?;
        }

        Command::Html { selector, inner } => {
            let result = client
                .request(
                    "html",
                    json!({
                        "session_id": session_id,
                        "selector": selector,
                        "inner": inner
                    }),
                )
                .await?;

            if cli.json {
                print_json(&result)?;
            } else if let Some(html) = result.get("html").and_then(|h| h.as_str()) {
                println!("{}", html);
            }
        }

        Command::Pdf {
            output: out,
            format,
            landscape,
            print_background,
        } => {
            let result = client
                .request(
                    "pdf",
                    json!({
                        "session_id": session_id,
                        "output": out.display().to_string(),
                        "format": format,
                        "landscape": landscape,
                        "print_background": print_background
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("");
                println!("PDF saved to: {}", path);
            }
        }

        Command::Cookies { subcommand } => {
            handle_cookies_via_daemon(subcommand, &mut client, session_id, cli).await?;
        }

        Command::Storage { subcommand } => {
            handle_storage_via_daemon(subcommand, &mut client, session_id, cli).await?;
        }

        Command::Record {
            output: out,
            duration,
            fps,
            quality,
            mp4,
            ..
        } => {
            let result = client
                .request(
                    "record",
                    json!({
                        "session_id": session_id,
                        "output": out.to_str().unwrap_or("recording"),
                        "duration": duration.unwrap_or(10),
                        "fps": fps,
                        "quality": quality,
                        "mp4": mp4
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let frames = result.get("frames").and_then(|f| f.as_u64()).unwrap_or(0);
                let duration = result
                    .get("duration_seconds")
                    .and_then(|d| d.as_u64())
                    .unwrap_or(0);
                println!("Recorded {} frames over {} seconds", frames, duration);
            }
        }

        Command::Trace {
            url,
            output: out,
            categories,
        } => {
            let result = client
                .request(
                    "trace",
                    json!({
                        "session_id": session_id,
                        "url": url,
                        "output": out.to_str().unwrap_or("trace.json"),
                        "categories": categories
                    }),
                )
                .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let path = result.get("path").and_then(|p| p.as_str()).unwrap_or("");
                println!("Trace saved to: {}", path);
            }
        }

        Command::Analyze { .. }
        | Command::Devices { .. }
        | Command::Config { .. }
        | Command::Sessions { .. }
        | Command::Session { .. }
        | Command::Server { .. } => {
            unreachable!("These commands are handled separately in dispatch()")
        }
    }

    Ok(())
}

async fn handle_cookies_via_daemon(
    subcommand: CookiesCommand,
    client: &mut DaemonClient,
    session_id: &str,
    cli: &Cli,
) -> Result<()> {
    let result = match subcommand {
        CookiesCommand::List => {
            client
                .request("cookies.list", json!({"session_id": session_id}))
                .await?
        }
        CookiesCommand::Get { name } => {
            client
                .request(
                    "cookies.get",
                    json!({"session_id": session_id, "name": name}),
                )
                .await?
        }
        CookiesCommand::Set {
            name,
            value,
            domain,
            path,
            secure,
            http_only,
        } => {
            client
                .request(
                    "cookies.set",
                    json!({
                        "session_id": session_id,
                        "name": name,
                        "value": value,
                        "domain": domain,
                        "path": path,
                        "secure": secure,
                        "http_only": http_only
                    }),
                )
                .await?
        }
        CookiesCommand::Delete { name } => {
            client
                .request(
                    "cookies.delete",
                    json!({"session_id": session_id, "name": name}),
                )
                .await?
        }
        CookiesCommand::Clear => {
            client
                .request("cookies.clear", json!({"session_id": session_id}))
                .await?
        }
    };

    if cli.json {
        print_json(&result)?;
    } else {
        let action = result
            .get("action")
            .and_then(|a| a.as_str())
            .unwrap_or("Done");
        println!("{}", action);
    }
    Ok(())
}

async fn handle_storage_via_daemon(
    subcommand: StorageCommand,
    client: &mut DaemonClient,
    session_id: &str,
    cli: &Cli,
) -> Result<()> {
    let result = match subcommand {
        StorageCommand::List { session_storage } => {
            client
                .request(
                    "storage.list",
                    json!({"session_id": session_id, "session_storage": session_storage}),
                )
                .await?
        }
        StorageCommand::Get { key, session_storage } => {
            client
                .request(
                    "storage.get",
                    json!({"session_id": session_id, "key": key, "session_storage": session_storage}),
                )
                .await?
        }
        StorageCommand::Set { key, value, session_storage } => {
            client
                .request(
                    "storage.set",
                    json!({"session_id": session_id, "key": key, "value": value, "session_storage": session_storage}),
                )
                .await?
        }
        StorageCommand::Delete { key, session_storage } => {
            client
                .request(
                    "storage.delete",
                    json!({"session_id": session_id, "key": key, "session_storage": session_storage}),
                )
                .await?
        }
        StorageCommand::Clear { session_storage } => {
            client
                .request(
                    "storage.clear",
                    json!({"session_id": session_id, "session_storage": session_storage}),
                )
                .await?
        }
    };

    if cli.json {
        print_json(&result)?;
    } else {
        let action = result
            .get("action")
            .and_then(|a| a.as_str())
            .unwrap_or("Done");
        println!("{}", action);
    }
    Ok(())
}
