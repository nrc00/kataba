use kataba_lsp::{
    client::LspClient,
    handlers::{LspHandlers, HandlerError},
    state::DocumentVersionTracker,
    transport::ServerConfig,
};
use kataba_core::encoding::PositionEncoding;
use lsp_types::{
    ApplyWorkspaceEditParams, ApplyWorkspaceEditResponse, CodeActionResponse, CompletionResponse,
    ConfigurationParams, DidChangeConfigurationParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DocumentSymbolResponse, ExecuteCommandParams, GotoDefinitionResponse,
    Hover, InitializeResult, LogMessageParams, Position, ProgressParams, PublishDiagnosticsParams,
    Range, RegistrationParams, ShowMessageParams, SymbolInformation, TextDocumentIdentifier,
    TextDocumentPositionParams, UnregistrationParams, Url, WorkspaceFolder,
    WorkspaceSymbolParams,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

#[derive(Debug, Default)]
struct TestHandlers {
    diagnostics: Arc<Mutex<Vec<PublishDiagnosticsParams>>>,
    log_messages: Arc<Mutex<Vec<LogMessageParams>>>,
    show_messages: Arc<Mutex<Vec<ShowMessageParams>>>,
    progress: Arc<Mutex<Vec<ProgressParams>>>,
}

impl TestHandlers {
    fn new() -> Self {
        Self::default()
    }

    async fn get_diagnostics(&self) -> Vec<PublishDiagnosticsParams> {
        self.diagnostics.lock().unwrap().clone()
    }
}

impl LspHandlers for TestHandlers {
    fn on_diagnostics(&self, params: PublishDiagnosticsParams) {
        let mut diags = self.diagnostics.lock().unwrap();
        diags.push(params);
    }

    fn on_progress(&self, params: ProgressParams) {
        let mut progs = self.progress.lock().unwrap();
        progs.push(params);
    }

    fn on_apply_edit(&self, _params: ApplyWorkspaceEditParams) -> ApplyWorkspaceEditResponse {
        ApplyWorkspaceEditResponse {
            applied: true,
            failure_reason: None,
            failed_change: None,
        }
    }

    fn on_register_capability(&self, _params: RegistrationParams) {}
    fn on_unregister_capability(&self, _params: UnregistrationParams) {}
    fn on_workspace_folders_change(&self, _params: DidChangeWorkspaceFoldersParams) {}
    fn on_config_change(&self, _params: DidChangeConfigurationParams) {}
    fn on_watched_files_change(&self, _params: DidChangeWatchedFilesParams) {}

    fn on_log_message(&self, params: LogMessageParams) {
        let mut msgs = self.log_messages.lock().unwrap();
        msgs.push(params);
    }

    fn on_show_message(&self, params: ShowMessageParams) {
        let mut msgs = self.show_messages.lock().unwrap();
        msgs.push(params);
    }

    fn on_configuration(&self, _params: ConfigurationParams) -> Vec<serde_json::Value> {
        Vec::new()
    }

    fn on_execute_command(&self, _params: ExecuteCommandParams) -> Result<Option<serde_json::Value>, HandlerError> {
        Err(HandlerError {
            code: -32601,
            message: "workspace/executeCommand not implemented".to_string(),
            data: None,
        })
    }
}

fn setup_test_project() -> (TempDir, Url) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_dir = temp_dir.path().join("test_project");
    std::fs::create_dir_all(&project_dir).expect("Failed to create project dir");

    // Create a simple Rust file
    let rust_file = project_dir.join("main.rs");
    std::fs::write(&rust_file, r#"
fn main() {
    let x = 42;
    println!("Hello, {}", x);
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#).expect("Failed to write test file");

    // Create Cargo.toml
    let cargo_toml = project_dir.join("Cargo.toml");
    std::fs::write(&cargo_toml, r#"
[package]
name = "test_project"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "test_project"
path = "main.rs"

[dependencies]
"#).expect("Failed to write Cargo.toml");

    let root_uri = Url::from_file_path(&project_dir).expect("Failed to create root URI");
    
    // Ensure root_uri ends with slash for proper joining
    let root_uri = if root_uri.path().ends_with('/') {
        root_uri
    } else {
        let mut url = root_uri.clone();
        url.set_path(&format!("{}/", root_uri.path()));
        url
    };
    (temp_dir, root_uri)
}

fn create_client(root_uri: Url) -> (LspClient, Arc<TestHandlers>) {
    let handlers = Arc::new(TestHandlers::new());
    
    let config = ServerConfig {
        command: "rust-analyzer".to_string(),
        args: vec![],
        env: vec![],
        current_dir: Some(root_uri.to_file_path().expect("Failed to convert URI to path")),
        root_markers: vec!["Cargo.toml".to_string()],
        filetypes: vec!["rust".to_string()],
        init_options: None,
        settings: None,
    };

    let client = LspClient::start(&config, handlers.clone())
        .expect("Failed to start LSP client");

    // Give the server time to start
    std::thread::sleep(Duration::from_millis(500));

    (client, handlers)
}

async fn wait_for_diagnostics(handlers: &Arc<TestHandlers>, timeout: Duration) -> Vec<PublishDiagnosticsParams> {
    let start = std::time::Instant::now();
    loop {
        let diagnostics = handlers.get_diagnostics().await;
        if !diagnostics.is_empty() {
            return diagnostics;
        }
        if start.elapsed() > timeout {
            return diagnostics;
        }
        sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn test_initialize_and_basic_requests() {
    // Keep temp dir alive for the duration of the test
    let (_temp_dir, root_uri) = setup_test_project();
    let (client, handlers) = create_client(root_uri.clone());

    // Initialize
    let _init_result: InitializeResult = client
        .initialize(
            Some(root_uri.clone()),
            Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "test_project".to_string(),
            }]),
            None,
            None,
        )
        .await
        .expect("Initialize failed");

    // Open a document
    let file_uri = root_uri.join("main.rs").expect("Failed to create file URI");
    let file_path = file_uri.to_file_path().expect("Failed to convert URI to path");
    let text = std::fs::read_to_string(&file_path)
        .expect("Failed to read file");

    let mut version_tracker = DocumentVersionTracker::default();
    client
        .did_open(&file_uri, "rust", text, &mut version_tracker)
        .await
        .expect("didOpen failed");

    // Wait for diagnostics
    let diagnostics = wait_for_diagnostics(&handlers, Duration::from_secs(20)).await;
    assert!(!diagnostics.is_empty(), "Expected diagnostics from rust-analyzer");

    // Wait a bit more for rust-analyzer to fully analyze
    sleep(Duration::from_millis(5000)).await;

    // Test hover
    let hover_pos = Position::new(6, 4); // "add" function
    let hover: Option<Hover> = client.hover(&file_uri, hover_pos).await.expect("Hover failed");
    assert!(hover.is_some(), "Expected hover result");

    // Test completion
    let completion_pos = Position::new(2, 10);
    let completion: Option<CompletionResponse> = client.completion(&file_uri, completion_pos).await.expect("Completion failed");
    assert!(completion.is_some(), "Expected completion result");

    // Test goto definition (on "add" function)
    let def_pos = Position::new(6, 10); // "add"
    let definition: Option<GotoDefinitionResponse> = client.goto_definition(&file_uri, def_pos).await.expect("Goto definition failed");
    assert!(definition.is_some(), "Expected definition result");

    // Test document symbols
    let symbols: Option<DocumentSymbolResponse> = client.document_symbols(&file_uri).await.expect("Document symbols failed");
    assert!(symbols.is_some(), "Expected document symbols result");

    // Shutdown
    client.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_code_actions() {
    let (_temp_dir, root_uri) = setup_test_project();
    let (client, handlers) = create_client(root_uri.clone());

    let _init_result: InitializeResult = client
        .initialize(
            Some(root_uri.clone()),
            Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "test_project".to_string(),
            }]),
            None,
            None,
        )
        .await
        .expect("Initialize failed");

    let file_uri = root_uri.join("main.rs").expect("Failed to create file URI");
    let file_path = file_uri.to_file_path().expect("Failed to convert URI to path");
    let text = std::fs::read_to_string(&file_path)
        .expect("Failed to read file");

    let mut version_tracker = DocumentVersionTracker::default();
    client
        .did_open(&file_uri, "rust", text, &mut version_tracker)
        .await
        .expect("didOpen failed");

    // Wait for diagnostics
    let _diagnostics = wait_for_diagnostics(&handlers, Duration::from_secs(20)).await;

    // Wait a bit more for rust-analyzer to fully analyze
    sleep(Duration::from_millis(5000)).await;

    // Test code action with diagnostics
    let range = Range::new(Position::new(0, 0), Position::new(8, 0));
    let code_actions: Option<CodeActionResponse> = client
        .code_action_with_diagnostics(&file_uri, range, None, None)
        .await
        .expect("Code action failed");
    // Code actions may be None if no actions are available for the given range
    // This is valid behavior, so we just check that the request succeeds
    let _ = code_actions;

    client.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_workspace_symbols() {
    let (_temp_dir, root_uri) = setup_test_project();
    let (client, _handlers) = create_client(root_uri.clone());

    let _init_result: InitializeResult = client
        .initialize(
            Some(root_uri.clone()),
            Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "test_project".to_string(),
            }]),
            None,
            None,
        )
        .await
        .expect("Initialize failed");

    let file_uri = root_uri.join("main.rs").expect("Failed to create file URI");
    let file_path = file_uri.to_file_path().expect("Failed to convert URI to path");
    let text = std::fs::read_to_string(&file_path)
        .expect("Failed to read file");

    let mut version_tracker = DocumentVersionTracker::default();
    client
        .did_open(&file_uri, "rust", text, &mut version_tracker)
        .await
        .expect("didOpen failed");

    // Wait for diagnostics
    let _diagnostics = wait_for_diagnostics(&_handlers, Duration::from_secs(20)).await;

    // Wait a bit more for rust-analyzer to fully analyze
    sleep(Duration::from_millis(5000)).await;

    // Test workspace symbols
    let symbols: Option<Vec<SymbolInformation>> = client.workspace_symbols("add".to_string()).await.expect("Workspace symbols failed");
    assert!(symbols.is_some(), "Expected workspace symbols result");

    client.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_formatting() {
    let (_temp_dir, root_uri) = setup_test_project();
    let (client, _handlers) = create_client(root_uri.clone());

    let _init_result: InitializeResult = client
        .initialize(
            Some(root_uri.clone()),
            Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "test_project".to_string(),
            }]),
            None,
            None,
        )
        .await
        .expect("Initialize failed");

    let file_uri = root_uri.join("main.rs").expect("Failed to create file URI");
    let file_path = file_uri.to_file_path().expect("Failed to convert URI to path");
    let text = std::fs::read_to_string(&file_path)
        .expect("Failed to read file");

    let mut version_tracker = DocumentVersionTracker::default();
    client
        .did_open(&file_uri, "rust", text, &mut version_tracker)
        .await
        .expect("didOpen failed");

    // Wait for diagnostics
    let _diagnostics = wait_for_diagnostics(&_handlers, Duration::from_secs(20)).await;

    // Wait a bit more for rust-analyzer to fully analyze
    sleep(Duration::from_millis(5000)).await;

    // Test formatting
    let formatting_options = lsp_types::FormattingOptions {
        tab_size: 4,
        insert_spaces: true,
        ..Default::default()
    };
    let edits: Option<Vec<lsp_types::TextEdit>> = client.formatting(&file_uri, formatting_options).await.expect("Formatting failed");
    assert!(edits.is_some(), "Expected formatting edits");

    client.shutdown().await.expect("Shutdown failed");
}
#[tokio::test]
async fn test_position_encoding_and_sync_kind_are_negotiated() {
    let (_temp_dir, root_uri) = setup_test_project();
    let (client, _handlers) = create_client(root_uri.clone());

    let init: InitializeResult = client
        .initialize(
            Some(root_uri.clone()),
            Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "test_project".to_string(),
            }]),
            None,
            None,
        )
        .await
        .expect("Initialize failed");

    let expected_encoding = init
        .capabilities
        .position_encoding
        .as_ref()
        .and_then(|kind| kind.as_str().parse::<PositionEncoding>().ok())
        .unwrap_or(PositionEncoding::Utf16);
    assert_eq!(
        client.position_encoding().await,
        expected_encoding,
        "client must expose the encoding the server selected"
    );

    let expected_sync = match init.capabilities.text_document_sync {
        Some(lsp_types::TextDocumentSyncCapability::Kind(kind)) => kind,
        Some(lsp_types::TextDocumentSyncCapability::Options(options)) => {
            options.change.unwrap_or(lsp_types::TextDocumentSyncKind::NONE)
        }
        None => lsp_types::TextDocumentSyncKind::NONE,
    };
    assert_eq!(client.sync_kind().await, expected_sync);

    client.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_concurrent_requests_are_not_starved_by_the_writer() {
    let (_temp_dir, root_uri) = setup_test_project();
    let (client, _handlers) = create_client(root_uri.clone());

    client
        .initialize(
            Some(root_uri.clone()),
            Some(vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "test_project".to_string(),
            }]),
            None,
            None,
        )
        .await
        .expect("Initialize failed");

    let mut tasks = Vec::new();
    for _ in 0..64 {
        let client = client.clone();
        tasks.push(tokio::spawn(async move {
            client.workspace_symbols("add".to_string()).await
        }));
    }

    for task in tasks {
        let result = task.await.expect("request task panicked");
        assert!(result.is_ok(), "concurrent request failed: {result:?}");
    }

    client.shutdown().await.expect("Shutdown failed");
}
