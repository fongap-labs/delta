//! Vision bridge: lets a text-only chat model work with pictures through a configured vision model.
//!
//! When the chat model cannot take image input, the vision model describes each attached picture
//! first, and the description goes into the chat in its place. The description keeps what a text
//! model needs to answer: every visible word, the layout, and anything unclear. This module builds
//! the request and reads its answer; the runtime sends the request through its egress gate and
//! proxy, the same way it sends every model request.

use serde_json::{json, Map, Value};

use crate::provider::ProviderRequest;

/// Largest picture the bridge sends, in MiB. A larger picture is refused, not cut down.
const MAX_IMAGE_MB_CAP: i64 = 8;
const MAX_IMAGE_MB_FLOOR: i64 = 1;
const MAX_TOKENS_CAP: i64 = 4096;
const DEFAULT_MAX_TOKENS: i64 = 1024;
const DEFAULT_TIMEOUT_SECS: f64 = 60.0;
const MIN_TIMEOUT_SECS: f64 = 5.0;
const MAX_TIMEOUT_SECS: f64 = 300.0;
/// Longest custom instruction accepted, in characters.
pub const MAX_PROMPT_CHARS: usize = 4000;

/// The instruction the vision model gets unless the settings replace it.
pub const DEFAULT_PROMPT: &str = "Describe this image for an assistant that cannot see it. Transcribe every visible word exactly, in reading order. Describe the layout, the objects and how they relate, and any colours, charts or numbers that matter. Mark anything you cannot read or are unsure of as [unclear]. Do not add anything that is not in the image.";

/// Protocols a bridge can speak. The picture's transfer format follows the protocol.
pub const PROTOCOLS: [&str; 3] = ["anthropic", "openai_chat", "openai_responses"];

/// How a turn treats pictures.
#[derive(Clone, Default)]
pub enum VisionMode {
    /// Pictures go to the chat model as they are.
    #[default]
    Native,
    /// The chat model cannot read pictures, so the vision model describes each one first.
    Bridge(VisionBridge),
    /// The chat model cannot read pictures and the bridge cannot be used. The reason is shown in
    /// place of each picture, so the user and the model both see why the picture was not read.
    Unavailable(String),
}

/// A resolved vision model together with the request limits it was configured with.
#[derive(Clone)]
pub struct VisionBridge {
    /// Provider-facing model id.
    pub model: String,
    pub protocol: String,
    pub api_key: String,
    pub base_url: String,
    pub max_image_bytes: usize,
    pub max_tokens: i64,
    pub timeout_secs: f64,
    pub prompt: String,
}

/// The bridge settings as saved in the preferences, before the vision model is resolved.
pub struct BridgeSettings {
    pub model: String,
    /// Empty means the vision model's own protocol.
    pub protocol: String,
    pub max_image_mb: i64,
    pub max_tokens: i64,
    pub timeout_secs: f64,
    pub prompt: String,
}

/// Read the bridge settings. `None` means no vision model is configured.
pub fn bridge_settings(prefs: &Map<String, Value>) -> Option<BridgeSettings> {
    let model = prefs
        .get("vision_model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())?
        .to_string();
    Some(BridgeSettings {
        model,
        protocol: prefs
            .get("vision_protocol")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        max_image_mb: prefs
            .get("vision_max_image_mb")
            .and_then(Value::as_i64)
            .unwrap_or(MAX_IMAGE_MB_CAP)
            .clamp(MAX_IMAGE_MB_FLOOR, MAX_IMAGE_MB_CAP),
        max_tokens: prefs
            .get("vision_max_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(DEFAULT_MAX_TOKENS)
            .clamp(1, MAX_TOKENS_CAP),
        timeout_secs: prefs
            .get("vision_timeout_secs")
            .and_then(Value::as_f64)
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS),
        prompt: prefs
            .get("vision_prompt")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .unwrap_or(DEFAULT_PROMPT)
            .to_string(),
    })
}

/// Whether a model reads pictures itself, and how that is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSupport {
    Yes,
    No,
    Unknown,
}

/// Where the answer about a model came from, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportSource {
    /// The user marked the model.
    Manual,
    /// A request showed it: pictures were refused, or accepted.
    Learned,
    /// The provider's own model list says which inputs the model takes.
    Declared,
    /// The model's name belongs to a family known to take pictures, or known not to.
    Name,
}

impl SupportSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Learned => "learned",
            Self::Declared => "declared",
            Self::Name => "name",
        }
    }
}

fn flag(prefs: &Map<String, Value>, key: &str, model_id: &str) -> Option<bool> {
    prefs
        .get(key)
        .and_then(Value::as_object)
        .and_then(|map| map.get(model_id))
        .and_then(Value::as_bool)
}

/// Whether this model takes pictures: the user's mark wins, then what a request showed, then what
/// the provider declares, then the model's name. A model none of them speaks for is `Unknown`.
pub fn image_support(
    prefs: &Map<String, Value>,
    model_id: &str,
) -> (ImageSupport, Option<SupportSource>) {
    let known = |value: bool, source| {
        (
            if value {
                ImageSupport::Yes
            } else {
                ImageSupport::No
            },
            Some(source),
        )
    };
    if let Some(value) = flag(prefs, "vision_support", model_id) {
        return known(value, SupportSource::Manual);
    }
    if let Some(value) = flag(prefs, "vision_learned", model_id) {
        return known(value, SupportSource::Learned);
    }
    if let Some(value) = flag(prefs, "vision_declared", model_id) {
        return known(value, SupportSource::Declared);
    }
    match name_hint(model_id) {
        Some(value) => known(value, SupportSource::Name),
        None => (ImageSupport::Unknown, None),
    }
}

/// Whether the preferences say this model takes pictures itself. Only a model something speaks for
/// as taking them does; an unknown one is treated as not taking them.
pub fn model_supports_images(prefs: &Map<String, Value>, model_id: &str) -> bool {
    image_support(prefs, model_id).0 == ImageSupport::Yes
}

/// The part of a routed id (`alias:model`, `fireworks:accounts/x/models/qwen-vl`) that names the model.
fn model_part(model_id: &str) -> String {
    model_id
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(model_id)
        .to_ascii_lowercase()
}

/// A guess from the model's name alone, for families that are clear. Anything else is `None`: a name
/// like "Max" says nothing, and a wrong guess is worse than none.
pub fn name_hint(model_id: &str) -> Option<bool> {
    let name = model_part(model_id);
    const TAKES_PICTURES: [&str; 14] = [
        "claude-",
        "gemini",
        "gpt-4o",
        "gpt-4.1",
        "gpt-5",
        "-vl",
        "vl-",
        "vision",
        "llava",
        "pixtral",
        "qvq",
        "internvl",
        "glm-4v",
        "minicpm-v",
    ];
    if TAKES_PICTURES.iter().any(|family| name.contains(family)) {
        return Some(true);
    }
    const TEXT_ONLY: [&str; 4] = ["deepseek-", "gpt-3.5", "o1-mini", "o1-preview"];
    if TEXT_ONLY.iter().any(|family| name.contains(family)) {
        return Some(false);
    }
    None
}

/// What one entry of a provider's model list says about picture input, when it says anything.
/// Providers describe this in different places: Anthropic under `capabilities.image_input`,
/// OpenRouter under `architecture.input_modalities`, and gateways as `input_modalities`,
/// `modalities`, `supports_vision` or `capabilities.vision`.
pub fn declared_image_input(entry: &Value) -> Option<bool> {
    if let Some(value) = entry
        .pointer("/capabilities/image_input/supported")
        .and_then(Value::as_bool)
    {
        return Some(value);
    }
    let mentions_image = |list: &Value| {
        list.as_array().map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| kind.eq_ignore_ascii_case("image"))
        })
    };
    for pointer in [
        "/architecture/input_modalities",
        "/input_modalities",
        "/supported_input_modalities",
        "/modalities/input",
        "/modalities",
    ] {
        if let Some(found) = entry.pointer(pointer).and_then(mentions_image) {
            return Some(found);
        }
    }
    for pointer in ["/capabilities/vision", "/supports_vision", "/vision"] {
        if let Some(value) = entry.pointer(pointer).and_then(Value::as_bool) {
            return Some(value);
        }
    }
    None
}

/// Why a model that cannot read pictures gets none, shown in place of each picture.
pub const NO_BRIDGE_REASON: &str =
    "this model cannot read pictures and no vision model is set (Settings > Models > Pictures)";

/// The token limit field each protocol takes. The Responses API renamed it.
pub fn token_key(protocol: &str) -> &'static str {
    if protocol == "openai_responses" {
        "max_output_tokens"
    } else {
        "max_tokens"
    }
}

/// The request that asks the vision model about one picture. The caller sets the proxy.
pub fn describe_request(bridge: &VisionBridge, picture: &Value) -> Result<ProviderRequest, String> {
    let data_url = picture
        .get("data_url")
        .and_then(Value::as_str)
        .ok_or_else(|| "the picture has no data".to_string())?;
    // Base64 carries three bytes in every four characters.
    let encoded = data_url.split_once(',').map_or(0, |(_, data)| data.len());
    if encoded / 4 * 3 > bridge.max_image_bytes {
        return Err(format!(
            "the picture is larger than {} MiB",
            bridge.max_image_bytes / (1024 * 1024)
        ));
    }
    let content = crate::runtime::provider_user_content(
        &bridge.protocol,
        &bridge.prompt,
        std::slice::from_ref(picture),
    );
    Ok(ProviderRequest {
        protocol: bridge.protocol.clone(),
        model: bridge.model.clone(),
        messages: json!([{"role": "user", "content": content}]),
        tools: None,
        settings: Some(json!({ token_key(&bridge.protocol): bridge.max_tokens })),
        api_key: bridge.api_key.clone(),
        base_url: bridge.base_url.clone(),
        timeout_secs: Some(bridge.timeout_secs),
        reduced: false,
        pinned_addrs: Vec::new(),
        proxy: None,
    })
}

/// The description the vision model wrote, from a provider response.
pub fn response_text(response: &Value) -> Result<String, String> {
    response
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "the vision model returned no text".to_string())
}

/// The attachments a chat model receives for a stored message. A picture with a description
/// is replaced by it; every other attachment is passed through unchanged.
///
/// `notes` is the message's `vision_notes`: a list of `{index, text}` where `index` is the
/// position of the picture in the message's attachments.
pub fn outbound_attachments(attachments: &[Value], notes: Option<&Value>) -> Vec<Value> {
    let notes = notes.and_then(Value::as_array);
    attachments
        .iter()
        .enumerate()
        .map(|(index, attachment)| {
            let described = notes.and_then(|list| {
                list.iter()
                    .find(|note| note.get("index").and_then(Value::as_u64) == Some(index as u64))
            });
            match described.and_then(|note| note.get("text").and_then(Value::as_str)) {
                Some(text) => json!({
                    "kind": "text",
                    "name": attachment.get("name").cloned().unwrap_or_else(|| json!("image")),
                    "text": text,
                }),
                None => attachment.clone(),
            }
        })
        .collect()
}

/// The attachments with every picture that has no description replaced by a note saying it was not
/// sent. A message written while the chat model took pictures keeps them, so a later request to a
/// model that does not must not send them.
pub fn withhold_pictures(attachments: Vec<Value>, reason: &str) -> Vec<Value> {
    attachments
        .into_iter()
        .map(|attachment| {
            if !is_picture(&attachment) {
                return attachment;
            }
            json!({
                "kind": "text",
                "name": attachment.get("name").cloned().unwrap_or_else(|| json!("image")),
                "text": format!(
                    "[image not sent: {reason}]"
                ),
            })
        })
        .collect()
}

/// Whether a stored attachment is a picture the bridge can describe.
pub fn is_picture(attachment: &Value) -> bool {
    attachment.get("kind").and_then(Value::as_str) == Some("image")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge(protocol: &str, max_image_bytes: usize) -> VisionBridge {
        VisionBridge {
            model: "vision-model".to_string(),
            protocol: protocol.to_string(),
            api_key: "key".to_string(),
            base_url: "https://vision.example/v1".to_string(),
            max_image_bytes,
            max_tokens: 512,
            timeout_secs: 30.0,
            prompt: DEFAULT_PROMPT.to_string(),
        }
    }

    fn picture(encoded: &str) -> Value {
        json!({
            "kind": "image",
            "name": "receipt.png",
            "mime": "image/png",
            "data_url": format!("data:image/png;base64,{encoded}"),
        })
    }

    #[test]
    fn token_limit_field_follows_the_protocol() {
        assert_eq!(token_key("openai_responses"), "max_output_tokens");
        assert_eq!(token_key("openai_chat"), "max_tokens");
        assert_eq!(token_key("anthropic"), "max_tokens");
    }

    #[test]
    fn the_picture_is_sent_in_each_protocols_own_format() {
        let image = picture("iVBORw0KGgo=");
        let chat = describe_request(&bridge("openai_chat", 1 << 20), &image).unwrap();
        let text = chat.messages.to_string();
        assert!(text.contains("\"image_url\""), "{text}");
        assert!(text.contains(DEFAULT_PROMPT), "{text}");

        let responses = describe_request(&bridge("openai_responses", 1 << 20), &image).unwrap();
        assert!(responses.messages.to_string().contains("\"input_image\""));
        assert_eq!(responses.settings, Some(json!({"max_output_tokens": 512})));

        let anthropic = describe_request(&bridge("anthropic", 1 << 20), &image).unwrap();
        assert!(anthropic.messages.to_string().contains("\"base64\""));
        assert_eq!(anthropic.settings, Some(json!({"max_tokens": 512})));
    }

    #[test]
    fn a_picture_over_the_limit_is_refused_before_any_request() {
        // 4 base64 characters carry 3 bytes, so 8 characters carry 6 bytes.
        let image = picture("AAAAAAAA");
        let error = describe_request(&bridge("openai_chat", 5), &image).unwrap_err();
        assert!(error.contains("larger than"), "{error}");
    }

    #[test]
    fn settings_are_clamped_to_their_ranges() {
        let prefs = json!({
            "vision_model": " vision-model ",
            "vision_max_image_mb": 99,
            "vision_max_tokens": 0,
            "vision_timeout_secs": 1.0,
            "vision_prompt": "   ",
        });
        let settings = bridge_settings(prefs.as_object().unwrap()).unwrap();
        assert_eq!(settings.model, "vision-model");
        assert_eq!(settings.max_image_mb, MAX_IMAGE_MB_CAP);
        assert_eq!(settings.max_tokens, 1);
        assert_eq!(settings.timeout_secs, MIN_TIMEOUT_SECS);
        assert_eq!(settings.prompt, DEFAULT_PROMPT);
        assert!(bridge_settings(&Map::new()).is_none());
    }

    #[test]
    fn a_models_picture_support_comes_from_the_strongest_source_that_knows() {
        let prefs = json!({
            "vision_support": {"marked-yes": true, "marked-no": false, "both": false},
            "vision_learned": {"learned-no": false, "both": true, "marked-no": true},
            "vision_declared": {"declared-yes": true, "learned-no": true},
        });
        let prefs = prefs.as_object().unwrap();
        let of = |model| image_support(prefs, model);
        assert_eq!(
            of("marked-yes"),
            (ImageSupport::Yes, Some(SupportSource::Manual))
        );
        assert_eq!(
            of("marked-no"),
            (ImageSupport::No, Some(SupportSource::Manual))
        );
        // The user's mark beats what a request showed; what a request showed beats the provider's list.
        assert_eq!(of("both"), (ImageSupport::No, Some(SupportSource::Manual)));
        assert_eq!(
            of("learned-no"),
            (ImageSupport::No, Some(SupportSource::Learned))
        );
        assert_eq!(
            of("declared-yes"),
            (ImageSupport::Yes, Some(SupportSource::Declared))
        );
        assert_eq!(
            of("claude-sonnet-4-6"),
            (ImageSupport::Yes, Some(SupportSource::Name))
        );
        assert_eq!(of("Max"), (ImageSupport::Unknown, None));
        assert!(model_supports_images(prefs, "marked-yes"));
        assert!(!model_supports_images(prefs, "Max"));
    }

    #[test]
    fn a_name_speaks_only_for_families_that_are_clear() {
        for yes in [
            "anthropic:claude-opus-4-7",
            "gpt-4o-mini",
            "fireworks:accounts/fireworks/models/qwen2p5-vl-72b-instruct",
            "gemini-2.5-pro",
            "glm-4v-plus",
        ] {
            assert_eq!(name_hint(yes), Some(true), "{yes}");
        }
        for no in ["deepseek-chat", "deepseek-reasoner", "gpt-3.5-turbo"] {
            assert_eq!(name_hint(no), Some(false), "{no}");
        }
        for unclear in ["Max", "qwen-max", "kimi-k2", "glm-4.6", "my-gateway-model"] {
            assert_eq!(name_hint(unclear), None, "{unclear}");
        }
        // A vision family inside an otherwise text-only provider's name still counts as pictures.
        assert_eq!(name_hint("deepseek-vl2"), Some(true));
    }

    #[test]
    fn a_providers_model_list_declares_picture_input_in_the_forms_in_use() {
        let declared = |entry: Value| declared_image_input(&entry);
        assert_eq!(
            declared(json!({"capabilities": {"image_input": {"supported": true}}})),
            Some(true)
        );
        assert_eq!(
            declared(json!({"architecture": {"input_modalities": ["text", "image"]}})),
            Some(true)
        );
        assert_eq!(
            declared(json!({"architecture": {"input_modalities": ["text"]}})),
            Some(false)
        );
        assert_eq!(
            declared(json!({"input_modalities": ["Text", "IMAGE"]})),
            Some(true)
        );
        assert_eq!(
            declared(json!({"modalities": {"input": ["text"], "output": ["text"]}})),
            Some(false)
        );
        assert_eq!(
            declared(json!({"modalities": ["text", "image"]})),
            Some(true)
        );
        assert_eq!(declared(json!({"supports_vision": false})), Some(false));
        assert_eq!(
            declared(json!({"capabilities": {"vision": true}})),
            Some(true)
        );
        assert_eq!(declared(json!({"id": "plain"})), None);
    }

    #[test]
    fn a_described_picture_is_replaced_and_the_rest_are_kept() {
        let attachments = vec![
            json!({"kind": "text", "name": "notes.txt", "text": "hi"}),
            picture("iVBORw0KGgo="),
        ];
        let notes = json!([{"index": 1, "text": "[image: receipt.png]\nTotal 42"}]);
        let out = outbound_attachments(&attachments, Some(&notes));
        assert_eq!(out[0], attachments[0]);
        assert_eq!(out[1]["kind"], "text");
        assert_eq!(out[1]["name"], "receipt.png");
        assert!(out[1]["text"].as_str().unwrap().contains("Total 42"));

        let untouched = outbound_attachments(&attachments, None);
        assert_eq!(untouched, attachments);
    }

    #[test]
    fn response_text_requires_a_non_empty_answer() {
        assert_eq!(
            response_text(&json!({"text": " Total 42 "})).unwrap(),
            "Total 42"
        );
        assert!(response_text(&json!({"text": "  "})).is_err());
        assert!(response_text(&json!({})).is_err());
    }
}
