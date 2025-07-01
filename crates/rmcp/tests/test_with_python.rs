use rmcp::{
    ServiceExt,
    transport::{ConfigureCommandExt, TokioChildProcess, sse_server::SseServerConfig},
};

// Import framework-specific types
#[cfg(feature = "axum")]
use rmcp::transport::AxumSseServer;
#[cfg(feature = "actix-web")]
use rmcp::transport::ActixSseServer;

use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod common;
use common::calculator::Calculator;

async fn init() -> anyhow::Result<()> {
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".to_string().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init();
    tokio::process::Command::new("uv")
        .args(["sync"])
        .current_dir("tests/test_with_python")
        .spawn()?
        .wait()
        .await?;
    Ok(())
}

// Common test logic for Python client
async fn test_with_python_client_common(bind_address: &str, ct: CancellationToken) -> anyhow::Result<()> {
    let status = tokio::process::Command::new("uv")
        .arg("run")
        .arg("client.py")
        .arg(format!("http://{bind_address}/sse"))
        .current_dir("tests/test_with_python")
        .spawn()?
        .wait()
        .await?;
    assert!(status.success());
    ct.cancel();
    Ok(())
}

#[cfg(feature = "axum")]
#[tokio::test]
async fn test_with_python_client_axum() -> anyhow::Result<()> {
    init().await?;

    const BIND_ADDRESS: &str = "127.0.0.1:8000";

    let ct = AxumSseServer::serve(BIND_ADDRESS.parse()?)
        .await?
        .with_service(Calculator::default);

    test_with_python_client_common(BIND_ADDRESS, ct).await
}

#[cfg(feature = "actix-web")]
#[tokio::test]
async fn test_with_python_client_actix() -> anyhow::Result<()> {
    init().await?;

    const BIND_ADDRESS: &str = "127.0.0.1:8000";

    let ct = ActixSseServer::serve(BIND_ADDRESS.parse()?)
        .await?
        .with_service(Calculator::default);

    test_with_python_client_common(BIND_ADDRESS, ct).await
}

/// Test the SSE server in a nested Axum router.
#[cfg(feature = "axum")]
#[tokio::test]
async fn test_nested_with_python_client() -> anyhow::Result<()> {
    use axum::Router;
    
    init().await?;

    const BIND_ADDRESS: &str = "127.0.0.1:8001";

    // Create an SSE router
    let sse_config = SseServerConfig {
        bind: BIND_ADDRESS.parse()?,
        sse_path: "/sse".to_string(),
        post_path: "/message".to_string(),
        ct: CancellationToken::new(),
        sse_keep_alive: None,
    };

    let listener = tokio::net::TcpListener::bind(&sse_config.bind).await?;

    let (sse_server, sse_router) = AxumSseServer::new(sse_config);
    let ct = sse_server.with_service(Calculator::default);

    let main_router = Router::new().nest("/nested", sse_router);

    let server_ct = ct.clone();
    let server = axum::serve(listener, main_router).with_graceful_shutdown(async move {
        server_ct.cancelled().await;
        tracing::info!("sse server cancelled");
    });

    tokio::spawn(async move {
        let _ = server.await;
        tracing::info!("sse server shutting down");
    });

    // Spawn the process with timeout, as failure to access the '/message' URL
    // causes the client to never exit.
    let status = timeout(
        tokio::time::Duration::from_secs(5),
        tokio::process::Command::new("uv")
            .arg("run")
            .arg("client.py")
            .arg(format!("http://{BIND_ADDRESS}/nested/sse"))
            .current_dir("tests/test_with_python")
            .spawn()?
            .wait(),
    )
    .await?;
    assert!(status?.success());
    ct.cancel();
    Ok(())
}

#[tokio::test]
async fn test_with_python_server() -> anyhow::Result<()> {
    init().await?;

    let transport = TokioChildProcess::new(tokio::process::Command::new("uv").configure(|cmd| {
        cmd.arg("run")
            .arg("server.py")
            .current_dir("tests/test_with_python");
    }))?;

    let client = ().serve(transport).await?;
    let resources = client.list_all_resources().await?;
    tracing::info!("{:#?}", resources);
    let tools = client.list_all_tools().await?;
    tracing::info!("{:#?}", tools);
    client.cancel().await?;
    Ok(())
}
