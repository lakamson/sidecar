// This is where we will create the default webserver for running the binary
// locally

use anyhow::Result;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::Extension;
use clap::Parser;
use sidecar::{
    application::{application::Application, config::configuration::Configuration},
    bg_poll::background_polling::poll_repo_updates,
};
use std::net::SocketAddr;
use tokio::signal;
use tokio::sync::oneshot;
use tower_http::{catch_panic::CatchPanicLayer, cors::CorsLayer};
use tracing::{debug, error, info};

use axum::http::header::AUTHORIZATION;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
pub type Router<S = Application> = axum::Router<S>;

#[tokio::main]
async fn main() -> Result<()> {
    info!("CodeStory 🚀");
    let configuration = Configuration::parse();

    // We get the logging setup first
    debug!("installing logging to local file");
    Application::install_logging(&configuration);

    // We create our scratch-pad directory
    Application::setup_scratch_pad(&configuration).await;

    // Create a oneshot channel
    let (tx, rx) = oneshot::channel();

    // Spawn a task to listen for signals
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("failed to listen for event");
        let _ = tx.send(());
    });

    // We initialize the logging here
    let application = Application::initialize(configuration).await?;
    println!("initialized application");
    debug!("initialized application");

    // Main logic
    tokio::select! {
        // Start the webserver
        _ = run(application) => {
            // Your server logic
        }
        _ = rx => {
            // Signal received, this block will be executed.
            // Drop happens automatically when variables go out of scope.
            debug!("Signal received, cleaning up...");
        }
    }

    Ok(())
}

pub async fn run(application: Application) -> Result<()> {
    let mut joins = tokio::task::JoinSet::new();

    // Start background tasks here
    if application.config.enable_background_polling {
        tokio::spawn(poll_repo_updates(application.clone()));
    }

    joins.spawn(start(application));

    while let Some(result) = joins.join_next().await {
        if let Ok(Err(err)) = result {
            error!(?err, "sidecar failed");
            return Err(err);
        }
    }

    Ok(())
}

// reintroduce when necessary
async fn _auth_middleware<B>(request: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    // Get token from Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    dbg!(&auth_header);

    match auth_header {
        Some(token) => {
            // Check if token starts with "Bearer "
            if let Some(token) = token.strip_prefix("Bearer ") {
                // Validate token here
                if _is_valid_token(token).await {
                    Ok(next.run(request).await)
                } else {
                    Err(StatusCode::UNAUTHORIZED)
                }
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

// Token validation function (implement your own logic)
async fn _is_valid_token(token: &str) -> bool {
    println!("webserver::is_valid_token::token({})", token);

    match _validate_workos_token(token).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

async fn _validate_workos_token(token: &str) -> Result<bool> {
    let client = reqwest::Client::new();

    let auth_proxy_endpoint = "";

    let response = client
        .get(auth_proxy_endpoint)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    dbg!(&response);

    Ok(response.status().is_success())
}

// TODO(skcd): Add routes here which can do the following:
// - when a file changes, it should still be logged and tracked
// - when a file is opened, it should be tracked over here too
pub async fn start(app: Application) -> anyhow::Result<()> {
    println!("Port: {}", app.config.port);
    let bind = SocketAddr::new(app.config.host.parse()?, app.config.port);

    // routes through middleware
    let protected_routes = Router::new()
        .nest("/inline_completion", inline_completion())
        .nest("/agentic", agentic_router())
        .nest("/plan", plan_router())
        .nest("/agent", agent_router());
    // .layer(from_fn(auth_middleware)); // routes through middleware

    // no middleware check
    let public_routes = Router::new()
        .route("/config", get(sidecar::webserver::config::get))
        .route(
            "/reach_the_devs",
            get(sidecar::webserver::config::reach_the_devs),
        )
        .route("/version", get(sidecar::webserver::config::version))
        .nest("/repo", repo_router())
        .nest("/in_editor", in_editor_router())
        .nest("/tree_sitter", tree_sitter_router())
        .nest("/file", file_operations_router());

    // both protected and public merged into api
    let mut api = Router::new().merge(protected_routes).merge(public_routes);

    api = api.route("/health", get(sidecar::webserver::health::health));

    let api = api
        .layer(Extension(app.clone()))
        .with_state(app.clone())
        .with_state(app.clone())
        .layer(CorsLayer::permissive())
        .layer(CatchPanicLayer::new())
        // I want to set the bytes limit here to 20 MB
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024));

    let router = Router::new().nest("/api", api);

    axum::Server::bind(&bind)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}

fn repo_router() -> Router {
    use axum::routing::*;
    Router::new()
        // 127.0.0.1:42424/api/repo/sync?backend=local/{path_absolute}
        .route("/sync", get(sidecar::webserver::repos::sync))
        .route("/status", get(sidecar::webserver::repos::index_status))
        // Gives back the status of the queue
        .route("/queue", get(sidecar::webserver::repos::queue_status))
        // Gives back the repos we know about
        .route("/repo_list", get(sidecar::webserver::repos::repo_status))
}

fn plan_router() -> Router {
    use axum::routing::*;
    Router::new()
        // Probe request routes
        // These routes handle starting and stopping probe requests
        .route(
            "/execute",
            post(sidecar::webserver::agent::execute_plan_until),
        )
        .route("/drop", post(sidecar::webserver::agent::drop_plan_from))
        .route(
            "/append",
            post(sidecar::webserver::agent::handle_append_plan),
        )
        .route(
            "/check_references",
            post(sidecar::webserver::agent::handle_check_references),
        )
}

// Define routes for agentic operations
// Define the router for agentic operations
// This router handles various AI-assisted code operations and benchmarking
fn agentic_router() -> Router {
    use axum::routing::*;
    Router::new()
        // Probe request routes
        // These routes handle starting and stopping probe requests
        .route(
            "/probe_request",
            post(sidecar::webserver::agentic::probe_request),
        )
        .route(
            "/probe_request_stop",
            post(sidecar::webserver::agentic::probe_request_stop),
        )
        // Code editing and sculpting routes
        // These routes handle various AI-assisted code modification operations
        .route(
            "/code_editing",
            post(sidecar::webserver::agentic::code_editing),
        )
        .route(
            "/code_sculpting_followup",
            post(sidecar::webserver::agentic::code_sculpting),
        )
        .route(
            "/code_sculpting_warmup",
            post(sidecar::webserver::agentic::code_sculpting_warmup),
        )
        .route(
            "/code_sculpting_heal",
            post(sidecar::webserver::agentic::code_sculpting_heal),
        )
        // route for push events coming from the editor
        .route(
            "/diagnostics",
            post(sidecar::webserver::agentic::push_diagnostics),
        )
        .route(
            "/context_recording",
            post(sidecar::webserver::agentic::context_recording),
        )
        // SWE bench route
        // This route is for software engineering benchmarking
        .route("/swe_bench", get(sidecar::webserver::agentic::swe_bench))
        .route(
            "/agent_session_chat",
            post(sidecar::webserver::agentic::agent_session_chat),
        )
        .route(
            "/agent_session_edit_anchored",
            post(sidecar::webserver::agentic::agent_session_edit_anchored),
        )
        .route(
            "/agent_session_edit_agentic",
            post(sidecar::webserver::agentic::agent_session_edit_agentic),
        )
        .route(
            "/agent_session_plan",
            post(sidecar::webserver::agentic::agent_session_plan),
        )
        .route(
            "/agent_session_plan_iterate",
            post(sidecar::webserver::agentic::agent_session_plan_iterate),
        )
        .route(
            "/cancel_running_event",
            post(sidecar::webserver::agentic::cancel_running_exchange),
        )
        .route(
            "/user_feedback_on_exchange",
            post(sidecar::webserver::agentic::user_feedback_on_exchange),
        )
        .route(
            "/user_handle_session_undo",
            post(sidecar::webserver::agentic::handle_session_undo),
        )
}

fn agent_router() -> Router {
    use axum::routing::*;
    Router::new()
        .route(
            "/search_agent",
            get(sidecar::webserver::agent::search_agent),
        )
        .route(
            "/hybrid_search",
            get(sidecar::webserver::agent::hybrid_search),
        )
        .route("/explain", get(sidecar::webserver::agent::explain))
        .route(
            "/followup_chat",
            post(sidecar::webserver::agent::followup_chat),
        )
}

fn in_editor_router() -> Router {
    use axum::routing::*;
    Router::new().route(
        "/answer",
        post(sidecar::webserver::in_line_agent::reply_to_user),
    )
}

fn tree_sitter_router() -> Router {
    use axum::routing::*;
    Router::new()
        .route(
            "/documentation_parsing",
            post(sidecar::webserver::tree_sitter::extract_documentation_strings),
        )
        .route(
            "/diagnostic_parsing",
            post(sidecar::webserver::tree_sitter::extract_diagnostics_range),
        )
        .route(
            "/tree_sitter_valid",
            post(sidecar::webserver::tree_sitter::tree_sitter_node_check),
        )
        .route(
            "/valid_xml",
            post(sidecar::webserver::tree_sitter::check_valid_xml),
        )
}

fn file_operations_router() -> Router {
    use axum::routing::*;
    Router::new().route("/edit_file", post(sidecar::webserver::file_edit::file_edit))
}

fn inline_completion() -> Router {
    use axum::routing::*;
    Router::new()
        .route(
            "/inline_completion",
            post(sidecar::webserver::inline_completion::inline_completion),
        )
        .route(
            "/cancel_inline_completion",
            post(sidecar::webserver::inline_completion::cancel_inline_completion),
        )
        .route(
            "/document_open",
            post(sidecar::webserver::inline_completion::inline_document_open),
        )
        .route(
            "/document_content_changed",
            post(sidecar::webserver::inline_completion::inline_completion_file_content_change),
        )
        .route(
            "/get_document_content",
            post(sidecar::webserver::inline_completion::inline_completion_file_content),
        )
        .route(
            "/get_identifier_nodes",
            post(sidecar::webserver::inline_completion::get_identifier_nodes),
        )
        .route(
            "/get_symbol_history",
            post(sidecar::webserver::inline_completion::symbol_history),
        )
}
