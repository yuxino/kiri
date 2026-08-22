use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::{redirect::Policy, Client};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::core::ocr_provider::is_loopback_url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_PNG_BYTES: usize = 20 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const OCR_PROMPT: &str =
    "Extract all visible text from this image. Return only the recognized text, preserving its reading order and line breaks.";

/// A deliberately small remote OCR transport for OpenAI-compatible Chat Completions APIs.
///
/// The regular client honors the operating system proxy configuration. The three explicitly
/// supported local development hosts (`localhost`, `127.0.0.1`, and `::1`) use a separate client
/// with proxy discovery disabled.
#[derive(Clone)]
pub struct RemoteOcrClient {
    system_proxy_client: Client,
    loopback_client: Client,
}

impl RemoteOcrClient {
    pub fn new() -> Result<Self, RemoteOcrError> {
        let common = || {
            Client::builder()
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
        };

        let system_proxy_client = common()
            .build()
            .map_err(|_| RemoteOcrError::ClientInitialization)?;
        let loopback_client = common()
            .no_proxy()
            .build()
            .map_err(|_| RemoteOcrError::ClientInitialization)?;

        Ok(Self {
            system_proxy_client,
            loopback_client,
        })
    }

    /// Sends one OCR request. The endpoint must be the complete Chat Completions endpoint.
    ///
    /// There are intentionally no retries or provider fallbacks: every invocation performs at
    /// most one HTTP request, so the user's explicit disclosure decision remains predictable.
    pub async fn recognize(
        &self,
        endpoint: &Url,
        model: &str,
        api_key: &SecretString,
        png: &[u8],
    ) -> Result<String, RemoteOcrError> {
        let route = classify_endpoint(endpoint)?;
        validate_model(model)?;
        validate_api_key(api_key)?;
        validate_png(png)?;

        let body = build_request_body(model, png);
        let client = match route {
            EndpointRoute::SystemProxy => &self.system_proxy_client,
            EndpointRoute::LoopbackNoProxy => &self.loopback_client,
        };

        let mut response = client
            .post(endpoint.clone())
            .bearer_auth(api_key.expose_secret())
            .json(&body)
            .send()
            .await
            .map_err(|_| RemoteOcrError::RequestFailed)?;
        // `json` serializes into reqwest's owned request bytes before the
        // request is sent. Release the additional Base64-bearing DTO as soon
        // as the response arrives instead of retaining a second copy while
        // the response body is read and parsed.
        drop(body);

        let status = response.status();
        if !status.is_success() {
            return Err(RemoteOcrError::HttpStatus(status.as_u16()));
        }

        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(RemoteOcrError::ResponseTooLarge);
        }

        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| RemoteOcrError::ResponseReadFailed)?
        {
            append_response_chunk(&mut bytes, &chunk)?;
        }

        parse_response(&bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointRoute {
    SystemProxy,
    LoopbackNoProxy,
}

fn classify_endpoint(endpoint: &Url) -> Result<EndpointRoute, RemoteOcrError> {
    if endpoint.host().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(RemoteOcrError::InvalidEndpoint);
    }

    let loopback = is_loopback_url(endpoint);
    match endpoint.scheme() {
        "https" if loopback => Ok(EndpointRoute::LoopbackNoProxy),
        "https" => Ok(EndpointRoute::SystemProxy),
        "http" if loopback => Ok(EndpointRoute::LoopbackNoProxy),
        _ => Err(RemoteOcrError::InvalidEndpoint),
    }
}

fn validate_model(model: &str) -> Result<(), RemoteOcrError> {
    if model.is_empty()
        || model.trim() != model
        || model.len() > 256
        || model.chars().any(char::is_control)
    {
        return Err(RemoteOcrError::InvalidModel);
    }
    Ok(())
}

fn validate_api_key(api_key: &SecretString) -> Result<(), RemoteOcrError> {
    let api_key = api_key.expose_secret();
    if api_key.is_empty() || api_key.len() > 16 * 1024 || api_key.chars().any(char::is_control) {
        return Err(RemoteOcrError::InvalidApiKey);
    }
    Ok(())
}

fn validate_png(png: &[u8]) -> Result<(), RemoteOcrError> {
    if png.len() > MAX_PNG_BYTES {
        return Err(RemoteOcrError::ImageTooLarge);
    }
    if !png.starts_with(PNG_SIGNATURE) {
        return Err(RemoteOcrError::InvalidImage);
    }
    Ok(())
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: [ChatMessage; 1],
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: [ChatContent; 2],
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatContent {
    Text { text: &'static str },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize)]
struct ImageUrl {
    url: String,
}

fn build_request_body<'a>(model: &'a str, png: &[u8]) -> ChatCompletionRequest<'a> {
    let encoded = BASE64_STANDARD.encode(png);
    ChatCompletionRequest {
        model,
        messages: [ChatMessage {
            role: "user",
            content: [
                ChatContent::Text { text: OCR_PROMPT },
                ChatContent::ImageUrl {
                    image_url: ImageUrl {
                        url: format!("data:image/png;base64,{encoded}"),
                    },
                },
            ],
        }],
    }
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

fn parse_response(bytes: &[u8]) -> Result<String, RemoteOcrError> {
    let response: ChatCompletionResponse =
        serde_json::from_slice(bytes).map_err(|_| RemoteOcrError::InvalidResponse)?;
    let text = response
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or(RemoteOcrError::InvalidResponse)?;

    if text.trim().is_empty() {
        return Err(RemoteOcrError::EmptyResponse);
    }
    Ok(text)
}

fn append_response_chunk(bytes: &mut Vec<u8>, chunk: &[u8]) -> Result<(), RemoteOcrError> {
    let next_len = bytes
        .len()
        .checked_add(chunk.len())
        .ok_or(RemoteOcrError::ResponseTooLarge)?;
    if next_len > MAX_RESPONSE_BYTES {
        return Err(RemoteOcrError::ResponseTooLarge);
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

/// Errors deliberately retain no reqwest/serde sources because those can contain request URLs,
/// response bodies, or provider-returned OCR text when formatted with `Debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RemoteOcrError {
    #[error("remote OCR client could not be initialized")]
    ClientInitialization,
    #[error("remote OCR endpoint is not allowed")]
    InvalidEndpoint,
    #[error("remote OCR model is invalid")]
    InvalidModel,
    #[error("remote OCR credential is invalid")]
    InvalidApiKey,
    #[error("OCR image is not a PNG")]
    InvalidImage,
    #[error("OCR image exceeds the size limit")]
    ImageTooLarge,
    #[error("remote OCR request failed")]
    RequestFailed,
    #[error("remote OCR service returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("remote OCR response exceeds the size limit")]
    ResponseTooLarge,
    #[error("remote OCR response could not be read")]
    ResponseReadFailed,
    #[error("remote OCR response has an invalid format")]
    InvalidResponse,
    #[error("remote OCR response contained no text")]
    EmptyResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_fixture() -> Vec<u8> {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(b"fixture");
        png
    }

    #[test]
    fn request_body_uses_chat_completions_multimodal_shape() {
        let png = png_fixture();
        let value = serde_json::to_value(build_request_body("vision-model", &png)).unwrap();

        assert_eq!(value["model"], "vision-model");
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"][0]["type"], "text");
        assert_eq!(value["messages"][0]["content"][1]["type"], "image_url");
        assert_eq!(
            value["messages"][0]["content"][1]["image_url"]["url"],
            format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png))
        );
    }

    #[test]
    fn parses_first_choice_without_mutating_text() {
        let bytes = br#"{
            "choices": [{"message": {"content": "first line\nsecond line"}}]
        }"#;

        assert_eq!(parse_response(bytes).unwrap(), "first line\nsecond line");
    }

    #[test]
    fn rejects_invalid_or_empty_responses_without_retaining_body() {
        let private_body = br#"{"private":"recognized secret text"}"#;
        let invalid_error = parse_response(private_body).unwrap_err();
        let empty_error = parse_response(br#"{"choices":[]}"#).unwrap_err();

        assert_eq!(invalid_error, RemoteOcrError::InvalidResponse);
        assert_eq!(empty_error, RemoteOcrError::InvalidResponse);
        assert!(!format!("{invalid_error:?}").contains("recognized secret text"));
        assert!(!invalid_error.to_string().contains("recognized secret text"));
    }

    #[test]
    fn enforces_png_signature_and_size_limit() {
        assert_eq!(
            validate_png(b"not a png"),
            Err(RemoteOcrError::InvalidImage)
        );

        let mut oversized = vec![0; MAX_PNG_BYTES + 1];
        oversized[..PNG_SIGNATURE.len()].copy_from_slice(PNG_SIGNATURE);
        assert_eq!(validate_png(&oversized), Err(RemoteOcrError::ImageTooLarge));
    }

    #[test]
    fn enforces_response_size_limit_while_streaming() {
        let mut response = vec![0; MAX_RESPONSE_BYTES];
        assert_eq!(
            append_response_chunk(&mut response, &[1]),
            Err(RemoteOcrError::ResponseTooLarge)
        );
        assert_eq!(response.len(), MAX_RESPONSE_BYTES);
    }

    #[test]
    fn only_explicit_local_hosts_may_use_plain_http_and_bypass_proxy() {
        assert_eq!(
            classify_endpoint(&Url::parse("http://127.0.0.1:8080/v1/chat/completions").unwrap()),
            Ok(EndpointRoute::LoopbackNoProxy)
        );
        assert_eq!(
            classify_endpoint(&Url::parse("https://[::1]/v1/chat/completions").unwrap()),
            Ok(EndpointRoute::LoopbackNoProxy)
        );
        for endpoint in [
            "http://127.0.0.2:8080/v1/chat/completions",
            "http://foo.localhost:8080/v1/chat/completions",
            "http://[::ffff:127.0.0.1]:8080/v1/chat/completions",
            "https://example.com/v1/chat/completions?forward=elsewhere",
        ] {
            assert_eq!(
                classify_endpoint(&Url::parse(endpoint).unwrap()),
                Err(RemoteOcrError::InvalidEndpoint),
                "unexpectedly accepted {endpoint}"
            );
        }
        assert_eq!(
            classify_endpoint(&Url::parse("https://api.example.test/v1/chat/completions").unwrap()),
            Ok(EndpointRoute::SystemProxy)
        );
        assert_eq!(
            classify_endpoint(&Url::parse("http://api.example.test/v1/chat/completions").unwrap()),
            Err(RemoteOcrError::InvalidEndpoint)
        );
    }

    #[test]
    fn endpoint_and_status_errors_never_echo_sensitive_inputs() {
        let endpoint = Url::parse(
            "https://user:private-password@example.test/v1/chat/completions#private-fragment",
        )
        .unwrap();
        let error = classify_endpoint(&endpoint).unwrap_err();
        let rendered = format!("{error:?} {error}");

        assert!(!rendered.contains("private-password"));
        assert!(!rendered.contains("private-fragment"));

        let status = RemoteOcrError::HttpStatus(401);
        let rendered = format!("{status:?} {status}");
        assert_eq!(
            rendered,
            "HttpStatus(401) remote OCR service returned HTTP status 401"
        );
    }
}
