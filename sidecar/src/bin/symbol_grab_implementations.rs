//! We are going to test out how the grab implementations part is working
//! over here as a E2E script

use std::{path::PathBuf, sync::Arc};

use llm_client::{
    broker::LLMBroker,
    clients::types::LLMType,
    config::LLMBrokerConfiguration,
    provider::{AnthropicAPIKey, GoogleAIStudioKey, LLMProvider, LLMProviderAPIKeys},
};
use sidecar::{
    agentic::{
        symbol::{
            events::{input::SymbolEventRequestId, message_event::SymbolEventMessageProperties},
            identifier::{LLMProperties, MechaCodeSymbolThinking, Snippet, SymbolIdentifier},
            tool_box::ToolBox,
            tool_properties::ToolProperties,
            types::Symbol,
        },
        tool::{
            broker::{ToolBroker, ToolBrokerConfiguration},
            code_edit::models::broker::CodeEditBroker,
        },
    },
    chunking::{
        editor_parsing::EditorParsing,
        languages::TSLanguageParsing,
        text_document::{Position, Range},
        types::OutlineNodeContent,
    },
    inline_completion::symbols_tracker::SymbolTrackerInline,
};

fn default_index_dir() -> PathBuf {
    match directories::ProjectDirs::from("ai", "codestory", "sidecar") {
        Some(dirs) => dirs.data_dir().to_owned(),
        None => "codestory_sidecar".into(),
    }
}

#[tokio::main]
async fn main() {
    let fs_file_path =
        "/Users/skcd/scratch/ide/src/vs/platform/configuration/common/configurationRegistry.ts"
            .to_owned();
    let placeholder_range = Range::new(Position::new(264, 0, 0), Position::new(686, 1, 0));
    let editor_url = "http://localhost:42450".to_owned();
    let editor_parsing = Arc::new(EditorParsing::default());
    let symbol_broker = Arc::new(SymbolTrackerInline::new(editor_parsing.clone()));
    let tool_broker = Arc::new(ToolBroker::new(
        Arc::new(
            LLMBroker::new(LLMBrokerConfiguration::new(default_index_dir()))
                .await
                .expect("to initialize properly"),
        ),
        Arc::new(CodeEditBroker::new()),
        symbol_broker.clone(),
        Arc::new(TSLanguageParsing::init()),
        ToolBrokerConfiguration::new(None, true),
        LLMProperties::new(
            LLMType::GeminiPro,
            LLMProvider::GoogleAIStudio,
            LLMProviderAPIKeys::GoogleAIStudio(GoogleAIStudioKey::new(
                "".to_owned(),
            )),
        ),
    ));

    let tool_box = Arc::new(ToolBox::new(tool_broker, symbol_broker, editor_parsing));

    let mecha_code_symbol_thinking = MechaCodeSymbolThinking::new(
        "ConfigurationRegistry".to_owned(),
        vec![],
        false,
        fs_file_path.to_owned(),
        Some(Snippet::new(
            "ConfigurationRegistry".to_owned(),
            placeholder_range.clone(),
            fs_file_path.to_owned(),
            "".to_owned(),
            OutlineNodeContent::new(
                "ConfigurationRegistry".to_owned(),
                placeholder_range.clone(),
                sidecar::chunking::types::OutlineNodeType::Class,
                "".to_owned(),
                fs_file_path.to_owned(),
                placeholder_range.clone(),
                placeholder_range.clone(),
                "rust".to_owned(),
                None,
            ),
        )),
        vec![],
        tool_box.clone(),
    );

    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();

    let (ui_sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    // fill this
    let access_token = String::from("");
    let event_properties = SymbolEventMessageProperties::new(
        SymbolEventRequestId::new("".to_owned(), "".to_owned()),
        ui_sender,
        editor_url.to_owned(),
        tokio_util::sync::CancellationToken::new(),
        access_token,
    );

    let symbol_identifier =
        SymbolIdentifier::with_file_path("ConfigurationRegistry", &fs_file_path);

    let symbol = Symbol::new(
        symbol_identifier.clone(),
        mecha_code_symbol_thinking,
        sender,
        tool_box.clone(),
        LLMProperties::new(
            LLMType::ClaudeOpus,
            LLMProvider::Anthropic,
            llm_client::provider::LLMProviderAPIKeys::Anthropic(AnthropicAPIKey::new(
                "".to_owned(),
            )),
        ),
        ToolProperties::new(),
        event_properties.clone(),
    )
    .await
    .expect("to work");

    let implementations = symbol
        .mecha_code_symbol()
        .grab_implementations(tool_box, symbol_identifier, event_properties.clone())
        .await;
    println!("implementations: {:?}", implementations);
    let mecha_code_symbol = symbol.mecha_code_symbol();
    dbg!(mecha_code_symbol.to_llm_request(event_properties).await);
}
