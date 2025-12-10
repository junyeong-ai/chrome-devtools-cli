use crate::Result;
use crate::config::Config;
use crate::handlers;
use crate::handlers::input::InteractionMode;
use crate::server::adapter::{ToResponse, opt_bool, opt_str, opt_u64};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use super::http::{DEFAULT_HTTP_PORT, HttpServer};
use super::ipc::{ClientId, IpcServer};
use super::protocol::{Request, Response, error_codes};
use super::session_pool::SessionPool;

const DEFAULT_SOCKET_PATH: &str = "/tmp/cdtcli.sock";

pub struct DaemonConfig {
    pub socket_path: PathBuf,
    pub http_port: u16,
    pub extension_path: Option<PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
            http_port: DEFAULT_HTTP_PORT,
            extension_path: None,
        }
    }
}

pub struct Daemon {
    config: Arc<Config>,
    daemon_config: DaemonConfig,
    session_pool: Arc<SessionPool>,
    ipc_server: Arc<IpcServer>,
    http_server: HttpServer,
    shutdown_tx: broadcast::Sender<()>,
}

impl Daemon {
    pub fn new(config: Arc<Config>, daemon_config: DaemonConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let session_pool = Arc::new(SessionPool::new(Arc::clone(&config)));
        let ipc_server = Arc::new(IpcServer::new(daemon_config.socket_path.clone()));
        let http_server = HttpServer::new(daemon_config.http_port, Arc::clone(&session_pool));

        Self {
            config,
            daemon_config,
            session_pool,
            ipc_server,
            http_server,
            shutdown_tx,
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.cleanup_stale_storage()?;
        self.write_pid_file()?;

        tracing::info!(
            "Daemon starting on {}",
            self.daemon_config.socket_path.display()
        );

        let listener = self.ipc_server.bind().await?;

        let pool = self.session_pool.clone();
        let ext_path = self.daemon_config.extension_path.clone();
        let config = self.config.clone();
        let ipc_server = self.ipc_server.clone();

        self.ipc_server
            .accept(&listener, move |client_id, request| {
                let pool = pool.clone();
                let ext_path = ext_path.clone();
                let config = config.clone();
                let ipc = ipc_server.clone();
                async move {
                    handle_request(client_id, request, &pool, ext_path.as_ref(), &config, &ipc)
                        .await
                }
            })
            .await?;

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let cleanup_pool = self.session_pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let expired = cleanup_pool.cleanup_expired_ephemeral().await;
                if expired > 0 {
                    tracing::info!("Cleaned up {} idle ephemeral sessions", expired);
                }
            }
        });

        tokio::select! {
            result = self.start() => result,
            result = self.http_server.run() => result,
            _ = shutdown_rx.recv() => {
                self.stop().await
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received shutdown signal");
                self.stop().await
            }
        }
    }

    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Daemon stopping...");

        let cleaned = self.session_pool.cleanup_all().await;
        tracing::info!("Cleaned up {} sessions", cleaned);

        self.ipc_server.shutdown();
        self.remove_pid_file();

        self.cleanup_stale_storage()?;

        tracing::info!("Daemon stopped");
        Ok(())
    }

    pub fn shutdown(&self) {
        self.shutdown_tx.send(()).ok();
    }

    fn cleanup_stale_storage(&self) -> Result<usize> {
        let cleaned = self.session_pool.cleanup_stale_storage()?;
        if cleaned > 0 {
            tracing::info!("Cleaned up {} stale session directories", cleaned);
        }
        Ok(cleaned)
    }

    fn pid_file_path(&self) -> PathBuf {
        self.daemon_config.socket_path.with_extension("pid")
    }

    fn write_pid_file(&self) -> Result<()> {
        let pid = std::process::id();
        std::fs::write(self.pid_file_path(), pid.to_string())?;
        Ok(())
    }

    fn remove_pid_file(&self) {
        std::fs::remove_file(self.pid_file_path()).ok();
    }

    pub fn is_running(socket_path: &Path) -> bool {
        let pid_path = socket_path.with_extension("pid");

        if !pid_path.exists() {
            return false;
        }

        if let Ok(pid_str) = std::fs::read_to_string(&pid_path)
            && let Ok(pid) = pid_str.trim().parse::<u32>()
        {
            return Self::process_exists(pid);
        }

        false
    }

    fn process_exists(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
            || std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
    }
}

async fn handle_request(
    _client_id: ClientId,
    request: Request,
    pool: &SessionPool,
    extension_path: Option<&PathBuf>,
    _config: &Config,
    ipc: &IpcServer,
) -> Response {
    let id = request.id;
    let params = &request.params;

    macro_rules! get_session {
        () => {
            match params.get("session_id").and_then(|v| v.as_str()) {
                Some(sid) => match pool.get(sid).await {
                    Some(s) => s,
                    None => {
                        return Response::error(
                            id,
                            error_codes::SESSION_NOT_FOUND,
                            "Session not found",
                        )
                    }
                },
                None => {
                    return Response::error(id, error_codes::INVALID_PARAMS, "session_id required")
                }
            }
        };
    }

    macro_rules! require_str {
        ($name:literal) => {
            match params.get($name).and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    return Response::error(
                        id,
                        error_codes::INVALID_PARAMS,
                        concat!($name, " required"),
                    )
                }
            }
        };
    }

    match request.method.as_str() {
        // === Session Management ===
        "session.create" => {
            let headless = opt_bool!(params, "headless", true);
            let profile_directory = opt_str!(params, "profile_directory").map(|s| s.to_string());

            let result = if profile_directory.is_some() {
                pool.get_or_create_user_profile_session(headless, extension_path)
                    .await
            } else {
                pool.create_ephemeral(headless, extension_path).await
            };

            match result {
                Ok(session) => {
                    let info = session.info().await;
                    Response::success(
                        id,
                        json!({
                            "session_id": info.id,
                            "cdp_port": info.cdp_port,
                            "headless": info.headless
                        }),
                    )
                }
                Err(e) => Response::error(id, error_codes::BROWSER_ERROR, e.to_string()),
            }
        }

        "session.list" => {
            let sessions = pool.list().await;
            let list: Vec<Value> = sessions
                .into_iter()
                .map(|s| {
                    json!({
                        "session_id": s.id,
                        "cdp_port": s.cdp_port,
                        "page_count": s.page_count,
                        "headless": s.headless,
                        "uses_user_profile": s.uses_user_profile
                    })
                })
                .collect();
            Response::success(id, json!(list))
        }

        "session.destroy" => {
            let session_id = require_str!("session_id");
            match pool.destroy(session_id).await {
                Ok(_) => Response::success(id, json!({"success": true})),
                Err(e) => Response::error(id, error_codes::BROWSER_ERROR, e.to_string()),
            }
        }

        "session.get" => {
            let session = get_session!();
            let info = session.info().await;
            Response::success(
                id,
                json!({
                    "session_id": info.id,
                    "cdp_port": info.cdp_port,
                    "page_count": info.page_count,
                    "headless": info.headless
                }),
            )
        }

        // === Navigation (delegated to handlers) ===
        "navigate" => {
            let session = get_session!();
            let url = require_str!("url");
            let wait_for = opt_str!(params, "wait_for");
            let timeout = opt_u64!(params, "timeout", 30000) / 1000;
            handlers::navigation::handle_navigate(session.as_ref(), url, wait_for, timeout)
                .await
                .to_response(id)
        }

        "reload" => {
            let session = get_session!();
            let hard = opt_bool!(params, "hard", false);
            handlers::navigation::handle_reload(session.as_ref(), hard)
                .await
                .to_response(id)
        }

        "back" => {
            let session = get_session!();
            handlers::navigation::handle_back(session.as_ref())
                .await
                .to_response(id)
        }

        "forward" => {
            let session = get_session!();
            handlers::navigation::handle_forward(session.as_ref())
                .await
                .to_response(id)
        }

        // === Input (delegated to handlers) ===
        "click" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let mode = opt_str!(params, "mode")
                .and_then(|m| m.parse().ok())
                .unwrap_or(InteractionMode::Auto);
            handlers::input::handle_click(session.as_ref(), selector, mode)
                .await
                .to_response(id)
        }

        "fill" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let text = require_str!("text");
            let mode = opt_str!(params, "mode")
                .and_then(|m| m.parse().ok())
                .unwrap_or(InteractionMode::Auto);
            handlers::input::handle_fill(session.as_ref(), selector, text, mode)
                .await
                .to_response(id)
        }

        "type" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let text = require_str!("text");
            let delay = params.get("delay").and_then(|v| v.as_u64());
            let mode = opt_str!(params, "mode")
                .and_then(|m| m.parse().ok())
                .unwrap_or(InteractionMode::Auto);
            handlers::input::handle_type(session.as_ref(), selector, text, delay, mode)
                .await
                .to_response(id)
        }

        "hover" => {
            let session = get_session!();
            let selector = require_str!("selector");
            handlers::input::handle_hover(session.as_ref(), selector)
                .await
                .to_response(id)
        }

        "press" => {
            let session = get_session!();
            let key = require_str!("key");
            handlers::input::handle_press(session.as_ref(), key)
                .await
                .to_response(id)
        }

        // === Inspect (delegated to handlers) ===
        "inspect" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let attributes = opt_bool!(params, "attributes", false);
            let styles = opt_bool!(params, "styles", false);
            let show_box = opt_bool!(params, "box", false);
            let children = opt_bool!(params, "children", false);
            handlers::inspect::handle_inspect(
                session.as_ref(),
                selector,
                attributes,
                styles,
                show_box,
                children,
            )
            .await
            .to_response(id)
        }

        "listeners" => {
            let session = get_session!();
            let selector = require_str!("selector");
            handlers::inspect::handle_listeners(session.as_ref(), selector)
                .await
                .to_response(id)
        }

        "query" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let count_only = opt_bool!(params, "count", false);
            let limit = opt_u64!(params, "limit", 20) as usize;
            handlers::inspect::handle_query(session.as_ref(), selector, count_only, Some(limit))
                .await
                .to_response(id)
        }

        "dom" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let depth = opt_u64!(params, "depth", 3) as u32;
            handlers::inspect::handle_dom(session.as_ref(), selector, depth)
                .await
                .to_response(id)
        }

        "a11y" => {
            let session = get_session!();
            let selector = opt_str!(params, "selector");
            let depth = opt_u64!(params, "depth", 5) as u32;
            let interactable = opt_bool!(params, "interactable", false);
            handlers::a11y::handle_a11y(session.as_ref(), selector, depth, interactable)
                .await
                .to_response(id)
        }

        // === Extras (delegated to handlers) ===
        "scroll" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let behavior = opt_str!(params, "behavior").unwrap_or("smooth");
            let block = opt_str!(params, "block").unwrap_or("center");
            handlers::extras::handle_scroll(session.as_ref(), selector, behavior, block)
                .await
                .to_response(id)
        }

        "select" => {
            let session = get_session!();
            let selector = require_str!("selector");
            let value = opt_str!(params, "value");
            let index = params
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|i| i as usize);
            let label = opt_str!(params, "label");
            handlers::extras::handle_select(session.as_ref(), selector, value, index, label)
                .await
                .to_response(id)
        }

        "html" => {
            let session = get_session!();
            let selector = opt_str!(params, "selector");
            let inner = opt_bool!(params, "inner", false);
            handlers::extras::handle_html(session.as_ref(), selector, inner)
                .await
                .to_response(id)
        }

        "pdf" => {
            let session = get_session!();
            let output = require_str!("output");
            let format = opt_str!(params, "format").unwrap_or("A4");
            let landscape = opt_bool!(params, "landscape", false);
            let print_background = opt_bool!(params, "print_background", false);
            handlers::extras::handle_pdf(
                session.as_ref(),
                std::path::Path::new(output),
                format,
                landscape,
                print_background,
            )
            .await
            .to_response(id)
        }

        // === Cookies (delegated to handlers) ===
        "cookies.list" => {
            let session = get_session!();
            handlers::extras::handle_cookies_list(session.as_ref())
                .await
                .to_response(id)
        }

        "cookies.get" => {
            let session = get_session!();
            let name = require_str!("name");
            handlers::extras::handle_cookies_get(session.as_ref(), name)
                .await
                .to_response(id)
        }

        "cookies.set" => {
            let session = get_session!();
            let name = require_str!("name");
            let value = require_str!("value");
            let domain = opt_str!(params, "domain");
            let path = opt_str!(params, "path");
            let secure = opt_bool!(params, "secure", false);
            let http_only = opt_bool!(params, "http_only", false);
            handlers::extras::handle_cookies_set(
                session.as_ref(),
                name,
                value,
                domain,
                path,
                secure,
                http_only,
            )
            .await
            .to_response(id)
        }

        "cookies.delete" => {
            let session = get_session!();
            let name = require_str!("name");
            handlers::extras::handle_cookies_delete(session.as_ref(), name)
                .await
                .to_response(id)
        }

        "cookies.clear" => {
            let session = get_session!();
            handlers::extras::handle_cookies_clear(session.as_ref())
                .await
                .to_response(id)
        }

        // === Storage (delegated to handlers) ===
        "storage.list" => {
            let session = get_session!();
            let session_storage = opt_bool!(params, "session_storage", false);
            handlers::extras::handle_storage_list(session.as_ref(), session_storage)
                .await
                .to_response(id)
        }

        "storage.get" => {
            let session = get_session!();
            let key = require_str!("key");
            let session_storage = opt_bool!(params, "session_storage", false);
            handlers::extras::handle_storage_get(session.as_ref(), key, session_storage)
                .await
                .to_response(id)
        }

        "storage.set" => {
            let session = get_session!();
            let key = require_str!("key");
            let value = require_str!("value");
            let session_storage = opt_bool!(params, "session_storage", false);
            handlers::extras::handle_storage_set(session.as_ref(), key, value, session_storage)
                .await
                .to_response(id)
        }

        "storage.delete" => {
            let session = get_session!();
            let key = require_str!("key");
            let session_storage = opt_bool!(params, "session_storage", false);
            handlers::extras::handle_storage_delete(session.as_ref(), key, session_storage)
                .await
                .to_response(id)
        }

        "storage.clear" => {
            let session = get_session!();
            let session_storage = opt_bool!(params, "session_storage", false);
            handlers::extras::handle_storage_clear(session.as_ref(), session_storage)
                .await
                .to_response(id)
        }

        // === Utility (delegated to handlers) ===
        "screenshot" => {
            let session = get_session!();
            let full_page = opt_bool!(params, "full_page", false);
            let output = opt_str!(params, "output").unwrap_or("screenshot.png");
            let format = opt_str!(params, "format");
            let quality = params
                .get("quality")
                .and_then(|v| v.as_u64())
                .map(|q| q as u8);
            let selector = opt_str!(params, "selector");
            handlers::screenshot::handle_screenshot(
                session.as_ref(),
                output,
                full_page,
                selector,
                format,
                quality,
            )
            .await
            .to_response(id)
        }

        "emulate" => {
            let session = get_session!();
            let device = require_str!("device");
            handlers::emulation::handle_emulate(session.as_ref(), device)
                .await
                .to_response(id)
        }

        "viewport" => {
            let session = get_session!();
            let width = match params
                .get("width")
                .and_then(|v| v.as_u64())
                .map(|w| w as u32)
            {
                Some(w) => w,
                None => return Response::error(id, error_codes::INVALID_PARAMS, "width required"),
            };
            let height = match params
                .get("height")
                .and_then(|v| v.as_u64())
                .map(|h| h as u32)
            {
                Some(h) => h,
                None => return Response::error(id, error_codes::INVALID_PARAMS, "height required"),
            };
            let pixel_ratio = params.get("pixel_ratio").and_then(|v| v.as_f64());
            handlers::emulation::handle_viewport(session.as_ref(), width, height, pixel_ratio)
                .await
                .to_response(id)
        }

        "dialog" => {
            let session = get_session!();
            let accept = opt_bool!(params, "accept", false);
            let text = opt_str!(params, "text").map(|s| s.to_string());
            handlers::dialog::handle_dialog_action(session.as_ref(), accept, text)
                .await
                .to_response(id)
        }

        "eval" => {
            let session = get_session!();
            let expression = require_str!("expression");
            handlers::script::handle_eval(session.as_ref(), expression)
                .await
                .to_response(id)
        }

        "wait" => {
            let session = get_session!();
            let condition = require_str!("condition");
            let selector = opt_str!(params, "selector");
            let timeout = opt_u64!(params, "timeout", 30000);
            handlers::script::handle_wait(session.as_ref(), condition, selector, timeout)
                .await
                .to_response(id)
        }

        "console" => {
            let session = get_session!();
            let filter = opt_str!(params, "filter");
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|l| l as usize);
            handlers::console::handle_console(session.as_ref(), filter, limit)
                .await
                .to_response(id)
        }

        "network" => {
            let session = get_session!();
            let domain = opt_str!(params, "domain");
            let status = params
                .get("status")
                .and_then(|v| v.as_u64())
                .map(|s| s as u16);
            handlers::network::handle_list(session.as_ref(), domain, status)
                .await
                .to_response(id)
        }

        // === Page Management (daemon-specific) ===
        "page.list" => {
            let session = get_session!();
            let pages = session.list_pages().await;
            let list: Vec<Value> = pages
                .into_iter()
                .map(|p| {
                    json!({
                        "index": p.index,
                        "url": p.url,
                        "title": p.title,
                        "active": p.active
                    })
                })
                .collect();
            Response::success(id, json!({"pages": list, "count": list.len()}))
        }

        "page.new" => {
            let session = get_session!();
            let url = opt_str!(params, "url");
            let timeout_ms = opt_u64!(params, "timeout", 30000);

            match session.new_page(None).await {
                Ok(page) => {
                    let pages = session.list_pages().await;
                    let page_index = pages.len() - 1;

                    if let Some(target_url) = url
                        && !target_url.is_empty()
                        && target_url != "about:blank"
                    {
                        match tokio::time::timeout(
                            Duration::from_millis(timeout_ms),
                            page.goto(target_url),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                let current_url = page.url().await.unwrap_or_default();
                                return Response::success(
                                    id,
                                    json!({"success": true, "index": page_index, "url": current_url}),
                                );
                            }
                            Ok(Err(e)) => {
                                return Response::error(
                                    id,
                                    error_codes::BROWSER_ERROR,
                                    e.to_string(),
                                );
                            }
                            Err(_) => {
                                return Response::error(
                                    id,
                                    error_codes::TIMEOUT,
                                    "Navigation timed out",
                                );
                            }
                        }
                    }

                    Response::success(
                        id,
                        json!({"success": true, "index": page_index, "url": "about:blank"}),
                    )
                }
                Err(e) => Response::error(id, error_codes::BROWSER_ERROR, e.to_string()),
            }
        }

        "page.select" => {
            let session = get_session!();
            let index = match params
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|i| i as usize)
            {
                Some(i) => i,
                None => return Response::error(id, error_codes::INVALID_PARAMS, "index required"),
            };
            match session.select_page(index).await {
                Ok(_) => Response::success(id, json!({"success": true, "index": index})),
                Err(e) => Response::error(id, error_codes::BROWSER_ERROR, e.to_string()),
            }
        }

        "page.close" => {
            let session = get_session!();
            let index = match params
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|i| i as usize)
            {
                Some(i) => i,
                None => return Response::error(id, error_codes::INVALID_PARAMS, "index required"),
            };
            match session.close_page(index).await {
                Ok(_) => Response::success(id, json!({"success": true, "index": index})),
                Err(e) => Response::error(id, error_codes::BROWSER_ERROR, e.to_string()),
            }
        }

        // === Infrastructure ===
        "ping" => Response::success(id, json!({"pong": true})),

        "shutdown" => {
            ipc.shutdown();
            Response::success(id, json!({"shutting_down": true}))
        }

        "devices" => {
            let devices: Vec<Value> = crate::devices::DEVICE_PRESETS
                .iter()
                .map(|d| {
                    json!({
                        "name": d.name,
                        "width": d.width,
                        "height": d.height,
                        "pixel_ratio": d.pixel_ratio,
                        "mobile": d.mobile
                    })
                })
                .collect();
            Response::success(id, json!({"devices": devices}))
        }

        // === Extension (daemon-specific) ===
        "extension.events" => {
            let session = get_session!();
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|l| l as usize);
            let since = params
                .get("since")
                .and_then(|v| v.as_u64())
                .map(|t| t as usize);

            let storage = session.storage();
            let events: Vec<crate::chrome::collectors::ExtensionEvent> =
                storage.read_all("extension").unwrap_or_default();
            let filtered: Vec<_> = events
                .into_iter()
                .skip(since.unwrap_or(0))
                .take(limit.unwrap_or(100))
                .collect();
            let count = session.collectors().extension.count();

            Response::success(
                id,
                json!({"events": filtered, "total": count, "returned": filtered.len()}),
            )
        }

        "extension.await" => {
            let session = get_session!();
            let timeout_ms = opt_u64!(params, "timeout", 30000);

            let mut rx = session.collectors().extension.subscribe();
            match tokio::time::timeout(Duration::from_millis(timeout_ms), rx.recv()).await {
                Ok(Ok(event)) => {
                    Response::success(id, serde_json::to_value(event).unwrap_or_default())
                }
                Ok(Err(_)) => Response::error(id, error_codes::INTERNAL_ERROR, "Channel closed"),
                Err(_) => Response::error(id, error_codes::TIMEOUT, "No event received"),
            }
        }

        "extension.count" => {
            let session = get_session!();
            let count = session.collectors().extension.count();
            Response::success(id, json!({"count": count}))
        }

        // === Trace ===
        "trace.start" => {
            let session = get_session!();
            let categories = params.get("categories").and_then(|c| {
                c.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
            });
            let page = match session.get_or_create_page().await {
                Ok(p) => p,
                Err(e) => return Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            };
            match session.collectors().trace.start(&page, categories).await {
                Ok(trace_id) => Response::success(id, json!({"trace_id": trace_id})),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            }
        }

        "trace.stop" => {
            let session = get_session!();
            let page = match session.get_or_create_page().await {
                Ok(p) => p,
                Err(e) => return Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            };
            match session.collectors().trace.stop(&page).await {
                Ok(data) => Response::success(id, serde_json::to_value(&data).unwrap_or_default()),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            }
        }

        "trace.status" => {
            let session = get_session!();
            let status = session.collectors().trace.status().await;
            Response::success(id, serde_json::to_value(&status).unwrap_or_default())
        }

        _ => Response::error(
            id,
            error_codes::METHOD_NOT_FOUND,
            format!("Unknown method: {}", request.method),
        ),
    }
}

pub fn default_socket_path() -> PathBuf {
    PathBuf::from(DEFAULT_SOCKET_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();
        assert_eq!(config.socket_path, PathBuf::from("/tmp/cdtcli.sock"));
    }

    #[test]
    fn test_default_socket_path() {
        assert_eq!(default_socket_path(), PathBuf::from("/tmp/cdtcli.sock"));
    }
}
