//! Client which can help us talk to openai

use async_openai::{
    config::{AzureConfig, OpenAIConfig},
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs, FunctionCall, Role,
    },
    Client,
};
use async_trait::async_trait;
use futures::StreamExt;

use crate::provider::LLMProviderAPIKeys;

use super::types::{
    LLMClient, LLMClientCompletionRequest, LLMClientCompletionResponse, LLMClientError,
    LLMClientMessage, LLMClientRole, LLMType,
};

enum OpenAIClientType {
    AzureClient(Client<AzureConfig>),
    OpenAIClient(Client<OpenAIConfig>),
}

pub struct OpenAIClient {}

impl OpenAIClient {
    pub fn new() -> Self {
        Self {}
    }

    pub fn model(&self, model: &LLMType) -> Option<String> {
        match model {
            LLMType::GPT3_5_16k => Some("gpt-3.5-turbo-16k-0613".to_owned()),
            LLMType::Gpt4 => Some("gpt-4-0613".to_owned()),
            LLMType::Gpt4Turbo => Some("gpt-4-1106-preview".to_owned()),
            LLMType::Gpt4_32k => Some("gpt-4-32k-0613".to_owned()),
            LLMType::Gpt4O => Some("gpt-4o".to_owned()),
            LLMType::Gpt4OMini => Some("gpt-4o-mini".to_owned()),
            LLMType::DeepSeekCoder33BInstruct => Some("deepseek-coder-33b".to_owned()),
            LLMType::Llama3_1_8bInstruct => Some("llama-3.1-8b-instant".to_owned()),
            LLMType::O1Preview => Some("o1-preview".to_owned()),
            _ => None,
        }
    }

    pub fn o1_preview_messages(
        &self,
        messages: &[LLMClientMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, LLMClientError> {
        let mut formatted_messages = Vec::new();
        let mut accumulated_content = String::new();
        let mut last_role: Option<LLMClientRole> = None;

        for message in messages {
            let current_role = match message.role() {
                LLMClientRole::System | LLMClientRole::User => LLMClientRole::User, // Treat System as User
                other => other.clone(),
            };

            match current_role {
                LLMClientRole::User => {
                    if last_role == Some(LLMClientRole::User) {
                        // Accumulate content for consecutive user messages
                        accumulated_content.push_str("\n");
                        accumulated_content.push_str(message.content());
                    } else {
                        // Flush previous messages if role changes
                        if let Some(prev_role) = last_role {
                            if prev_role != LLMClientRole::User && !accumulated_content.is_empty() {
                                let user_message = ChatCompletionRequestUserMessageArgs::default()
                                    .role(Role::User)
                                    .content(accumulated_content.clone())
                                    .build()
                                    .map(ChatCompletionRequestMessage::User)
                                    .map_err(LLMClientError::OpenAPIError)?;
                                formatted_messages.push(user_message);
                                accumulated_content.clear();
                            }
                        }
                        accumulated_content = message.content().to_owned();
                    }
                    last_role = Some(LLMClientRole::User);
                }
                LLMClientRole::Assistant => {
                    // Flush accumulated user messages before handling assistant message
                    if last_role == Some(LLMClientRole::User) && !accumulated_content.is_empty() {
                        let user_message = ChatCompletionRequestUserMessageArgs::default()
                            .role(Role::User)
                            .content(accumulated_content.clone())
                            .build()
                            .map(ChatCompletionRequestMessage::User)
                            .map_err(LLMClientError::OpenAPIError)?;
                        formatted_messages.push(user_message);
                        accumulated_content.clear();
                    }
                    // Handle assistant message
                    let assistant_message = match message.get_function_call() {
                        Some(function_call) => ChatCompletionRequestAssistantMessageArgs::default()
                            .role(Role::Function)
                            .function_call(FunctionCall {
                                name: function_call.name().to_owned(),
                                arguments: function_call.arguments().to_owned(),
                            })
                            .build()
                            .map(ChatCompletionRequestMessage::Assistant)
                            .map_err(LLMClientError::OpenAPIError)?,
                        None => ChatCompletionRequestAssistantMessageArgs::default()
                            .role(Role::Assistant)
                            .content(message.content().to_owned())
                            .build()
                            .map(ChatCompletionRequestMessage::Assistant)
                            .map_err(LLMClientError::OpenAPIError)?,
                    };
                    formatted_messages.push(assistant_message);
                    last_role = Some(LLMClientRole::Assistant);
                }
                LLMClientRole::Function => {
                    // Flush accumulated user messages before handling function message
                    if last_role == Some(LLMClientRole::User) && !accumulated_content.is_empty() {
                        let user_message = ChatCompletionRequestUserMessageArgs::default()
                            .role(Role::User)
                            .content(accumulated_content.clone())
                            .build()
                            .map(ChatCompletionRequestMessage::User)
                            .map_err(LLMClientError::OpenAPIError)?;
                        formatted_messages.push(user_message);
                        accumulated_content.clear();
                    }
                    // Handle function message
                    let function_message = match message.get_function_call() {
                        Some(function_call) => ChatCompletionRequestAssistantMessageArgs::default()
                            .role(Role::Function)
                            .content(message.content().to_owned())
                            .function_call(FunctionCall {
                                name: function_call.name().to_owned(),
                                arguments: function_call.arguments().to_owned(),
                            })
                            .build()
                            .map(ChatCompletionRequestMessage::Assistant)
                            .map_err(LLMClientError::OpenAPIError)?,
                        None => return Err(LLMClientError::FunctionCallNotPresent),
                    };
                    formatted_messages.push(function_message);
                    last_role = Some(LLMClientRole::Function);
                }
                // if its a system prompt don't do anything
                LLMClientRole::System => {}
            }
        }

        // Flush any remaining accumulated user messages
        if last_role == Some(LLMClientRole::User) && !accumulated_content.is_empty() {
            let user_message = ChatCompletionRequestUserMessageArgs::default()
                .role(Role::User)
                .content(accumulated_content)
                .build()
                .map(ChatCompletionRequestMessage::User)
                .map_err(LLMClientError::OpenAPIError)?;
            formatted_messages.push(user_message);
        }

        Ok(formatted_messages)
    }

    pub fn messages(
        &self,
        messages: &[LLMClientMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, LLMClientError> {
        let formatted_messages = messages
            .into_iter()
            .map(|message| {
                let role = message.role();
                match role {
                    LLMClientRole::User => ChatCompletionRequestUserMessageArgs::default()
                        .role(Role::User)
                        .content(message.content().to_owned())
                        .build()
                        .map(|message| ChatCompletionRequestMessage::User(message))
                        .map_err(|e| LLMClientError::OpenAPIError(e)),
                    LLMClientRole::System => ChatCompletionRequestSystemMessageArgs::default()
                        .role(Role::System)
                        .content(message.content().to_owned())
                        .build()
                        .map(|message| ChatCompletionRequestMessage::System(message))
                        .map_err(|e| LLMClientError::OpenAPIError(e)),
                    // TODO(skcd): This might be wrong, but for now its okay as we
                    // do not use these branches at all
                    LLMClientRole::Assistant => match message.get_function_call() {
                        Some(function_call) => ChatCompletionRequestAssistantMessageArgs::default()
                            .role(Role::Function)
                            .function_call(FunctionCall {
                                name: function_call.name().to_owned(),
                                arguments: function_call.arguments().to_owned(),
                            })
                            .build()
                            .map(|message| ChatCompletionRequestMessage::Assistant(message))
                            .map_err(|e| LLMClientError::OpenAPIError(e)),
                        None => ChatCompletionRequestAssistantMessageArgs::default()
                            .role(Role::Assistant)
                            .content(message.content().to_owned())
                            .build()
                            .map(|message| ChatCompletionRequestMessage::Assistant(message))
                            .map_err(|e| LLMClientError::OpenAPIError(e)),
                    },
                    LLMClientRole::Function => match message.get_function_call() {
                        Some(function_call) => ChatCompletionRequestAssistantMessageArgs::default()
                            .role(Role::Function)
                            .content(message.content().to_owned())
                            .function_call(FunctionCall {
                                name: function_call.name().to_owned(),
                                arguments: function_call.arguments().to_owned(),
                            })
                            .build()
                            .map(|message| ChatCompletionRequestMessage::Assistant(message))
                            .map_err(|e| LLMClientError::OpenAPIError(e)),
                        None => Err(LLMClientError::FunctionCallNotPresent),
                    },
                }
            })
            .collect::<Vec<_>>();
        formatted_messages
            .into_iter()
            .collect::<Result<Vec<ChatCompletionRequestMessage>, LLMClientError>>()
    }

    fn generate_openai_client(
        &self,
        api_key: LLMProviderAPIKeys,
        llm_model: &LLMType,
    ) -> Result<OpenAIClientType, LLMClientError> {
        // special escape hatch for deepseek-coder-33b
        if matches!(llm_model, LLMType::DeepSeekCoder33BInstruct) {
            // if we have deepseek coder 33b right now, then we should return an openai
            // client right here, this is a hack to get things working and the provider
            // needs to be updated to support this
            return match api_key {
                LLMProviderAPIKeys::OpenAIAzureConfig(api_key) => {
                    let config = OpenAIConfig::new()
                        .with_api_key(api_key.api_key)
                        .with_api_base(api_key.api_base);
                    Ok(OpenAIClientType::OpenAIClient(Client::with_config(config)))
                }
                _ => Err(LLMClientError::WrongAPIKeyType),
            };
        }
        match api_key {
            LLMProviderAPIKeys::OpenAI(api_key) => {
                let config = OpenAIConfig::new().with_api_key(api_key.api_key);
                Ok(OpenAIClientType::OpenAIClient(Client::with_config(config)))
            }
            LLMProviderAPIKeys::OpenAIAzureConfig(azure_config) => {
                let config = AzureConfig::new()
                    .with_api_base(azure_config.api_base)
                    .with_api_key(azure_config.api_key)
                    .with_deployment_id(azure_config.deployment_id)
                    .with_api_version(azure_config.api_version);
                Ok(OpenAIClientType::AzureClient(Client::with_config(config)))
            }
            _ => Err(LLMClientError::WrongAPIKeyType),
        }
    }
}

#[async_trait]
impl LLMClient for OpenAIClient {
    fn client(&self) -> &crate::provider::LLMProvider {
        &crate::provider::LLMProvider::OpenAI
    }

    async fn stream_completion(
        &self,
        api_key: LLMProviderAPIKeys,
        request: LLMClientCompletionRequest,
        sender: tokio::sync::mpsc::UnboundedSender<LLMClientCompletionResponse>,
    ) -> Result<String, LLMClientError> {
        let llm_model = request.model();
        let model = self.model(llm_model);
        if model.is_none() {
            return Err(LLMClientError::UnSupportedModel);
        }
        let model = model.unwrap();
        let messages = if llm_model == &LLMType::O1Preview {
            self.o1_preview_messages(request.messages())?
        } else {
            self.messages(request.messages())?
        };
        let mut request_builder_args = CreateChatCompletionRequestArgs::default();
        let mut request_builder = request_builder_args
            .model(model.to_owned())
            .messages(messages);
        // o1-preview has no streaming
        if !llm_model.is_o1_preview() {
            request_builder = request_builder
                .temperature(request.temperature())
                .stream(true);
        } else {
            request_builder = request_builder.temperature(1.0);
        }
        if let Some(frequency_penalty) = request.frequency_penalty() {
            request_builder = request_builder.frequency_penalty(frequency_penalty);
        }
        let request = request_builder.build()?;
        let mut buffer = String::new();
        let client = self.generate_openai_client(api_key, llm_model)?;

        // TODO(skcd): Bad code :| we are repeating too many things but this
        // just works and we need it right now
        match client {
            OpenAIClientType::AzureClient(client) => {
                let stream_maybe = client.chat().create_stream(request).await;
                if stream_maybe.is_err() {
                    return Err(LLMClientError::OpenAPIError(stream_maybe.err().unwrap()));
                } else {
                    dbg!("no error here");
                }
                let mut stream = stream_maybe.unwrap();
                while let Some(response) = stream.next().await {
                    match response {
                        Ok(response) => {
                            let delta = response
                                .choices
                                .get(0)
                                .map(|choice| choice.delta.content.to_owned())
                                .flatten()
                                .unwrap_or("".to_owned());
                            let _value = response
                                .choices
                                .get(0)
                                .map(|choice| choice.delta.content.as_ref())
                                .flatten();
                            buffer.push_str(&delta);
                            let _ = sender.send(LLMClientCompletionResponse::new(
                                buffer.to_owned(),
                                Some(delta),
                                model.to_owned(),
                            ));
                        }
                        Err(err) => {
                            dbg!(err);
                            break;
                        }
                    }
                }
            }
            OpenAIClientType::OpenAIClient(client) => {
                if llm_model == &LLMType::O1Preview {
                    let completion = client.chat().create(request).await?;
                    let response = completion
                        .choices
                        .get(0)
                        .ok_or(LLMClientError::FailedToGetResponse)?;
                    let content = response
                        .message
                        .content
                        .as_ref()
                        .ok_or(LLMClientError::FailedToGetResponse)?;
                    let _ = sender.send(LLMClientCompletionResponse::new(
                        content.to_owned(),
                        Some(content.to_owned()),
                        model.to_owned(),
                    ));
                    buffer = content.to_owned();
                } else {
                    let mut stream = client.chat().create_stream(request).await?;
                    while let Some(response) = stream.next().await {
                        match response {
                            Ok(response) => {
                                let response = response
                                    .choices
                                    .get(0)
                                    .ok_or(LLMClientError::FailedToGetResponse)?;
                                let text = response.delta.content.to_owned();
                                if let Some(text) = text {
                                    buffer.push_str(&text);
                                    let _ = sender.send(LLMClientCompletionResponse::new(
                                        buffer.to_owned(),
                                        Some(text),
                                        model.to_owned(),
                                    ));
                                }
                            }
                            Err(err) => {
                                dbg!(err);
                                break;
                            }
                        }
                    }
                }
            }
        }
        Ok(buffer)
    }

    async fn completion(
        &self,
        api_key: LLMProviderAPIKeys,
        request: LLMClientCompletionRequest,
    ) -> Result<String, LLMClientError> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let result = self.stream_completion(api_key, request, sender).await?;
        Ok(result)
    }

    async fn stream_prompt_completion(
        &self,
        _api_key: LLMProviderAPIKeys,
        _request: super::types::LLMClientCompletionStringRequest,
        _sender: tokio::sync::mpsc::UnboundedSender<LLMClientCompletionResponse>,
    ) -> Result<String, LLMClientError> {
        Err(LLMClientError::OpenAIDoesNotSupportCompletion)
    }
}
