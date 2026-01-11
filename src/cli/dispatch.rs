use super::{
    Cli,
    commands::{
        AuthCommand, Command, ConfigCommand, CookiesCommand, HistoryCommand, ServerCommand,
        SessionCommand, StorageCommand,
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
        Command::History { subcommand } => handle_history_command(subcommand, &cli, &config).await,
        Command::Auth { subcommand } => handle_auth_command(subcommand, &cli, &config).await,
        Command::Devices { .. } => handle_devices_command(&cli, &config).await,
        Command::Analyze { trace } => {
            let result = handlers::performance::handle_analyze(&trace)?;
            output::print_output(&result, cli.json, config.output.json_pretty)
        }
        Command::Trace {
            url,
            output,
            user_profile,
            headless,
        } => handle_trace_command(&url, &output, user_profile, headless, &cli, &config).await,
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

async fn handle_auth_command(subcommand: AuthCommand, cli: &Cli, config: &Config) -> Result<()> {
    let socket_path = get_socket_path(config);

    if !is_daemon_running(&socket_path) {
        eprintln!("Starting daemon...");
        start_daemon_background()?;
        tokio::time::sleep(std::time::Duration::from_secs(secs::DAEMON_STARTUP)).await;

        if !is_daemon_running(&socket_path) {
            return Err(ChromeError::Connection("Failed to start daemon".into()));
        }
    }

    let mut client = DaemonClient::connect(&socket_path).await?;
    let session_id = if cli.user_profile {
        let headless = cli.headless.unwrap_or(false);
        client.get_or_create_user_profile_session(headless).await?
    } else if let Some(ref sid) = cli.session {
        sid.clone()
    } else {
        return Err(ChromeError::General(
            "Session required. Use --user-profile or --session".into(),
        ));
    };

    match subcommand {
        AuthCommand::Export { output } => {
            let result = daemon_request(
                &mut client,
                "auth.export",
                &session_id,
                json!({ "output": output.as_ref().map(|p| p.display().to_string()) }),
            )
            .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let cookies = result
                    .get("cookies_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let origins = result
                    .get("origins_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                println!(
                    "{}",
                    output::text::success(&format!(
                        "Exported {} cookies, {} origins",
                        cookies, origins
                    ))
                );
                if let Some(path) = output {
                    println!(
                        "{}",
                        output::text::key_value("Output", &path.display().to_string())
                    );
                }
            }
            Ok(())
        }
        AuthCommand::Import { input } => {
            let result = daemon_request(
                &mut client,
                "auth.import",
                &session_id,
                json!({ "input": input.display().to_string() }),
            )
            .await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let cookies = result
                    .get("cookies_imported")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let origins = result
                    .get("origins_imported")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                println!(
                    "{}",
                    output::text::success(&format!(
                        "Imported {} cookies, {} origins",
                        cookies, origins
                    ))
                );
            }
            Ok(())
        }
    }
}

async fn resolve_session_id(session_id: Option<String>, user_profile: bool) -> Result<String> {
    if let Some(sid) = session_id {
        return Ok(sid);
    }
    if user_profile {
        let socket_path = crate::server::default_socket_path();
        if let Ok(mut client) = crate::client::DaemonClient::connect(&socket_path).await
            && let Ok(Some(sid)) = client.get_user_profile_session_id().await
        {
            return Ok(sid);
        }
        return Err(ChromeError::General(
            "No active user-profile session found. Start a browser with --user-profile first."
                .to_string(),
        ));
    }
    Err(ChromeError::General(
        "Session ID required. Use --user-profile or provide a session ID.".to_string(),
    ))
}

async fn handle_history_command(
    subcommand: HistoryCommand,
    cli: &Cli,
    config: &Config,
) -> Result<()> {
    match subcommand {
        HistoryCommand::List => {
            let result = handlers::sessions::handle_list()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for session in &result.sessions {
                    println!("{}", session.session_id);
                }
            }
        }
        HistoryCommand::Show {
            session_id,
            user_profile,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let result = handlers::sessions::handle_show(&sid)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{:?}", result);
            }
        }
        HistoryCommand::Network {
            session_id,
            user_profile,
            domain,
            status,
            from,
            to,
            last,
            limit,
            offset,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let time_filter = handlers::sessions::TimeFilter::new(from, to, last);
            let result = handlers::sessions::handle_network(
                &sid,
                domain.as_deref(),
                status,
                time_filter,
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
        HistoryCommand::Console {
            session_id,
            user_profile,
            level,
            from,
            to,
            last,
            limit,
            offset,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let time_filter = handlers::sessions::TimeFilter::new(from, to, last);
            let result = handlers::sessions::handle_console(
                &sid,
                level.as_deref(),
                time_filter,
                limit,
                offset,
            )?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for msg in &result.items {
                    println!("[{}] {}", msg.level, msg.text);
                }
            }
        }
        HistoryCommand::Errors {
            session_id,
            user_profile,
            from,
            to,
            last,
            limit,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let time_filter = handlers::sessions::TimeFilter::new(from, to, last);
            let result = handlers::sessions::handle_errors(&sid, time_filter, limit, None)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for err in &result.items {
                    println!("{}", err.message);
                }
            }
        }
        HistoryCommand::Issues {
            session_id,
            user_profile,
            limit,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let result = handlers::sessions::handle_issues(&sid, limit)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for issue in &result.items {
                    println!("[{}] {:?}", issue.code, issue.details);
                }
            }
        }
        HistoryCommand::Delete { session_id } => {
            handlers::sessions::handle_delete(&session_id)?;
            println!("Deleted session: {}", session_id);
        }
        HistoryCommand::Clean { older_than } => {
            let duration = older_than
                .or_else(|| Some(format!("{}h", config.storage.session_ttl_hours)))
                .and_then(|s| handlers::sessions::parse_duration(&s));
            let result = handlers::sessions::handle_clean(duration)?;
            println!("Cleaned {} sessions", result.removed);
        }
        HistoryCommand::Events {
            session_id,
            user_profile,
            r#type,
            from,
            to,
            last,
            recording,
            limit,
            offset,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let time_filter = handlers::sessions::TimeFilter::new(from, to, last);
            let result = handlers::sessions::handle_events(
                &sid,
                r#type.as_deref(),
                time_filter,
                recording.as_deref(),
                limit,
                offset,
            )?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Events ({}/{})", result.items.len(), result.total);
                for event in &result.items {
                    println!("  {:?}", event);
                }
            }
        }
        HistoryCommand::Recordings {
            session_id,
            user_profile,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let result = handlers::sessions::handle_recordings_list(&sid)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", result.format_text());
            }
        }
        HistoryCommand::Recording {
            session_id,
            user_profile,
            recording_id,
            frames,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            if frames {
                let result = handlers::sessions::handle_recording_frames(&sid, &recording_id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("{}", result.format_text());
                }
            } else {
                let result = handlers::sessions::handle_recording_show(&sid, &recording_id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("{}", result.format_text());
                }
            }
        }
        HistoryCommand::Export {
            session_id,
            user_profile,
            recording,
            format,
            output,
        } => {
            let sid = resolve_session_id(session_id, user_profile).await?;
            let result =
                handlers::export::handle_export(&sid, recording.as_deref(), &format, output)?;
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
    let headless = cli.headless.unwrap_or(!cli.user_profile);

    let (session_id, is_new) = if let Some(ref sid) = cli.session {
        (sid.clone(), false)
    } else {
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

    let result = handle_via_daemon(&command, &cli, &config, &session_id).await;

    // user_profile mode: retry once if browser was closed
    if cli.user_profile && is_browser_closed_error(&result) {
        if !cli.json {
            eprintln!("[reconnecting...]");
        }

        // Destroy dead session and create new one
        client
            .request("session.destroy", json!({ "session_id": session_id }))
            .await
            .ok();

        let new_sid = client.get_or_create_user_profile_session(headless).await?;

        if !cli.json {
            eprintln!("[session: {}]", new_sid);
        }

        return handle_via_daemon(&command, &cli, &config, &new_sid).await;
    }

    result
}

fn is_browser_closed_error(result: &Result<()>) -> bool {
    match result {
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            msg.contains("receiver is gone")
                || msg.contains("connection")
                || msg.contains("browser")
        }
        Ok(_) => false,
    }
}

async fn handle_via_daemon(
    command: &Command,
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
                println!("Page reloaded{}", if *hard { " (hard)" } else { "" });
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

            let out_path = out.clone();
            std::fs::write(out, &data)?;

            if cli.json {
                println!(r#"{{"path":"{}"}}"#, out_path.display());
            } else {
                println!("Screenshot saved to: {}", out_path.display());
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
                        "accept": *accept && !*dismiss,
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
                        "attributes": *all || *attributes,
                        "styles": *all || *styles,
                        "box": *all || *r#box,
                        "children": *all || *children
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
            handle_cookies_via_daemon(subcommand.clone(), &mut client, session_id, cli).await?;
        }

        Command::Storage { subcommand } => {
            handle_storage_via_daemon(subcommand.clone(), &mut client, session_id, cli).await?;
        }

        Command::Analyze { .. }
        | Command::Trace { .. }
        | Command::Devices { .. }
        | Command::Config { .. }
        | Command::History { .. }
        | Command::Session { .. }
        | Command::Server { .. }
        | Command::Auth { .. } => {
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

async fn handle_trace_command(
    url: &str,
    output: &std::path::Path,
    user_profile: bool,
    headless: bool,
    cli: &Cli,
    config: &Arc<Config>,
) -> Result<()> {
    let socket_path = get_socket_path(config);

    if !is_daemon_running(&socket_path) {
        eprintln!("Starting daemon...");
        start_daemon_background()?;
        tokio::time::sleep(tokio::time::Duration::from_secs(
            crate::timeouts::secs::DAEMON_STARTUP,
        ))
        .await;

        if !is_daemon_running(&socket_path) {
            return Err(ChromeError::Connection("Failed to start daemon".into()));
        }
    }

    let mut client = DaemonClient::connect(&socket_path).await?;

    let params = if user_profile {
        json!({"headless": headless, "profile_directory": "user"})
    } else {
        json!({"headless": headless})
    };

    let resp = client.request("session.create", params).await?;
    let session_id = resp
        .get("session_id")
        .and_then(|s| s.as_str())
        .ok_or_else(|| ChromeError::General("Failed to get session_id".into()))?
        .to_string();

    let resp = client
        .request("trace.start", json!({"session_id": session_id}))
        .await?;

    if resp.get("error").is_some() {
        return Err(ChromeError::General(
            resp.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Failed to start trace")
                .to_string(),
        ));
    }

    client
        .request(
            "navigate",
            json!({
                "session_id": session_id,
                "url": url,
                "wait_for": "networkidle"
            }),
        )
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let resp = client
        .request("trace.stop", json!({"session_id": session_id}))
        .await?;

    let trace_data = resp.get("result").unwrap_or(&resp);
    let events = trace_data.get("events").cloned().unwrap_or(json!([]));
    let event_count = events.as_array().map(|a| a.len()).unwrap_or(0);

    let output_data = json!({
        "traceEvents": events,
        "metadata": {
            "url": url,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    });

    std::fs::write(output, serde_json::to_string_pretty(&output_data)?)?;

    if !user_profile {
        client
            .request("session.destroy", json!({"session_id": session_id}))
            .await
            .ok();
    }

    if cli.json {
        print_json(&json!({
            "file": output.display().to_string(),
            "events": event_count,
            "url": url,
        }))?;
    } else {
        println!(
            "Trace captured: {} ({} events)",
            output.display(),
            event_count
        );
    }

    Ok(())
}
