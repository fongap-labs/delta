//! Built-in model and provider catalog used only for compatibility and product discovery.
//!
//! ModelAuthority owns persisted routing/configuration state. This module owns
//! the static compatibility catalog so the authority does not encode concrete
//! model/provider knowledge in its state-management implementation.

pub(crate) const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";
pub(crate) const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
pub(crate) const OPENAI_PROTOCOL: &str = "openai";
pub(crate) const ANTHROPIC_PROTOCOL: &str = "anthropic";
pub(crate) const MODEL_MATRIX: &[(&str, &str, Option<u64>)] = &[
    ("gpt-5.6-sol", "GPT-5.6 Sol · OpenAI", Some(400_000)),
    ("gpt-5.6-terra", "GPT-5.6 Terra · OpenAI", Some(400_000)),
    ("gpt-5.6-luna", "GPT-5.6 Luna · OpenAI", Some(400_000)),
    ("gpt-5.5", "GPT-5.5 · OpenAI", Some(400_000)),
    (
        "anthropic:claude-fable-5",
        "Claude Fable 5 · Anthropic",
        Some(1_000_000),
    ),
    (
        "anthropic:claude-opus-4-8",
        "Claude Opus 4.8 · Anthropic",
        Some(200_000),
    ),
    (
        "anthropic:claude-sonnet-4-6",
        "Claude Sonnet 4.6 · Anthropic",
        Some(200_000),
    ),
    (
        "anthropic:claude-haiku-4-5",
        "Claude Haiku 4.5 · Anthropic",
        Some(200_000),
    ),
    ("meta:muse-spark-1.1", "Muse Spark 1.1 · Meta", None),
    ("zai:glm-5.2", "GLM-5.2 · Z AI", Some(128_000)),
    (
        "deepseek:deepseek-v4-flash",
        "DeepSeek V4 Flash · DeepSeek",
        Some(128_000),
    ),
    (
        "deepseek:deepseek-v4-pro",
        "DeepSeek V4 Pro · DeepSeek",
        Some(128_000),
    ),
    ("kimi:kimi-k2.6", "Kimi K2.6 · Moonshot", Some(256_000)),
    ("minimax:MiniMax-M2.5", "MiniMax M2.5 · MiniMax", None),
    ("qwen:qwen3-max", "Qwen3 Max · Alibaba", Some(256_000)),
    ("xai:grok-4.3", "Grok 4.3 · xAI", Some(256_000)),
    (
        "mistral:mistral-large-latest",
        "Mistral Large · Mistral",
        Some(128_000),
    ),
    (
        "together:thinkingmachines/Inkling",
        "Inkling · via Together",
        None,
    ),
    (
        "together:zai-org/GLM-5.2",
        "GLM-5.2 · via Together",
        Some(128_000),
    ),
    (
        "together:moonshotai/Kimi-K3",
        "Kimi K3 · via Together",
        Some(1_000_000),
    ),
    (
        "together:moonshotai/Kimi-K2.7-Code",
        "Kimi K2.7 Code · via Together",
        Some(256_000),
    ),
    (
        "together:moonshotai/Kimi-K2.6",
        "Kimi K2.6 · via Together",
        Some(256_000),
    ),
    (
        "together:deepseek-ai/DeepSeek-V4-Pro",
        "DeepSeek V4 Pro · via Together",
        Some(128_000),
    ),
    (
        "together:meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
        "Llama 4 Maverick · via Together",
        Some(1_000_000),
    ),
    (
        "fireworks:accounts/fireworks/models/glm-5p2",
        "GLM-5.2 · via Fireworks",
        Some(128_000),
    ),
    (
        "fireworks:accounts/fireworks/models/kimi-k2p6",
        "Kimi K2.6 · via Fireworks",
        Some(256_000),
    ),
    (
        "fireworks:accounts/fireworks/models/deepseek-v4-pro",
        "DeepSeek V4 Pro · via Fireworks",
        Some(128_000),
    ),
    (
        "fireworks:accounts/fireworks/models/llama4-maverick-instruct-basic",
        "Llama 4 Maverick · via Fireworks",
        Some(1_000_000),
    ),
    (
        "openrouter:z-ai/glm-5.2",
        "GLM-5.2 · via OpenRouter",
        Some(128_000),
    ),
    (
        "openrouter:moonshotai/kimi-k2.6",
        "Kimi K2.6 · via OpenRouter",
        Some(256_000),
    ),
    (
        "openrouter:deepseek/deepseek-v4-pro",
        "DeepSeek V4 Pro · via OpenRouter",
        Some(128_000),
    ),
    (
        "openrouter:meta-llama/llama-4-maverick",
        "Llama 4 Maverick · via OpenRouter",
        Some(1_000_000),
    ),
];

#[derive(Clone, Copy)]
pub(crate) struct ProviderSpec {
    pub(crate) name: &'static str,
    pub(crate) title: &'static str,
    pub(crate) protocol: &'static str,
    pub(crate) base_url: &'static str,
    pub(crate) recommended_model: &'static str,
    pub(crate) env_key: &'static str,
    pub(crate) blurb: &'static str,
}

pub(crate) const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec { name: "openai", title: "OpenAI", protocol: OPENAI_PROTOCOL, base_url: DEFAULT_OPENAI_URL, recommended_model: "gpt-5.6-sol", env_key: "OPENAI_API_KEY", blurb: "OpenAI Responses API by default; Chat Completions is available for compatible endpoints." },
    ProviderSpec { name: "anthropic", title: "Claude (Anthropic)", protocol: ANTHROPIC_PROTOCOL, base_url: DEFAULT_ANTHROPIC_URL, recommended_model: "claude-fable-5", env_key: "ANTHROPIC_API_KEY", blurb: "Anthropic-compatible Messages API." },
    ProviderSpec { name: "zai", title: "Z AI (GLM)", protocol: OPENAI_PROTOCOL, base_url: "https://api.z.ai/api/paas/v4", recommended_model: "glm-5.2", env_key: "ZAI_API_KEY", blurb: "Uses Z AI's OpenAI-compatible API." },
    ProviderSpec { name: "deepseek", title: "DeepSeek", protocol: OPENAI_PROTOCOL, base_url: "https://api.deepseek.com", recommended_model: "deepseek-v4-flash", env_key: "DEEPSEEK_API_KEY", blurb: "Uses DeepSeek's OpenAI-compatible API." },
    ProviderSpec { name: "kimi", title: "Kimi (Moonshot AI)", protocol: OPENAI_PROTOCOL, base_url: "https://api.moonshot.ai/v1", recommended_model: "kimi-k2.6", env_key: "MOONSHOT_API_KEY", blurb: "Uses Moonshot's OpenAI-compatible API." },
    ProviderSpec { name: "minimax", title: "MiniMax", protocol: OPENAI_PROTOCOL, base_url: "https://api.minimax.io/v1", recommended_model: "MiniMax-M2.5", env_key: "MINIMAX_API_KEY", blurb: "Uses MiniMax's OpenAI-compatible API." },
    ProviderSpec { name: "qwen", title: "Qwen (Alibaba)", protocol: OPENAI_PROTOCOL, base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", recommended_model: "qwen3-max", env_key: "DASHSCOPE_API_KEY", blurb: "Uses Alibaba Model Studio's OpenAI-compatible API." },
    ProviderSpec { name: "xai", title: "xAI (Grok)", protocol: OPENAI_PROTOCOL, base_url: "https://api.x.ai/v1", recommended_model: "grok-4.3", env_key: "XAI_API_KEY", blurb: "Uses xAI's OpenAI-compatible API." },
    ProviderSpec { name: "mistral", title: "Mistral", protocol: OPENAI_PROTOCOL, base_url: "https://api.mistral.ai/v1", recommended_model: "mistral-large-latest", env_key: "MISTRAL_API_KEY", blurb: "Uses Mistral's OpenAI-compatible API." },
    ProviderSpec { name: "meta", title: "Meta (Muse Spark)", protocol: OPENAI_PROTOCOL, base_url: "https://api.meta.ai/v1", recommended_model: "muse-spark-1.1", env_key: "META_API_KEY", blurb: "Uses Meta's OpenAI-compatible Model API." },
    ProviderSpec { name: "together", title: "Together AI", protocol: OPENAI_PROTOCOL, base_url: "https://api.together.xyz/v1", recommended_model: "zai-org/GLM-5.2", env_key: "TOGETHER_API_KEY", blurb: "Uses Together AI's OpenAI-compatible API." },
    ProviderSpec { name: "fireworks", title: "Fireworks AI", protocol: OPENAI_PROTOCOL, base_url: "https://api.fireworks.ai/inference/v1", recommended_model: "accounts/fireworks/models/glm-5p2", env_key: "FIREWORKS_API_KEY", blurb: "Uses Fireworks AI's OpenAI-compatible API." },
    ProviderSpec { name: "openrouter", title: "OpenRouter", protocol: OPENAI_PROTOCOL, base_url: "https://openrouter.ai/api/v1", recommended_model: "z-ai/glm-5.2", env_key: "OPENROUTER_API_KEY", blurb: "Uses OpenRouter's OpenAI-compatible API." },
];

#[derive(Clone)]
pub(crate) struct ProviderDescriptor {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) protocol: String,
    pub(crate) base_url: String,
    pub(crate) recommended_model: String,
    pub(crate) env_key: String,
    pub(crate) blurb: String,
}

impl From<ProviderSpec> for ProviderDescriptor {
    fn from(value: ProviderSpec) -> Self {
        Self {
            name: value.name.to_string(),
            title: value.title.to_string(),
            protocol: value.protocol.to_string(),
            base_url: value.base_url.to_string(),
            recommended_model: value.recommended_model.to_string(),
            env_key: value.env_key.to_string(),
            blurb: value.blurb.to_string(),
        }
    }
}

pub(crate) fn suggested_models(provider: &str) -> Vec<String> {
    let mut suggestions: Vec<String> = MODEL_MATRIX
        .iter()
        .filter_map(|(model, _, _)| {
            if provider == "openai" && !model.contains(':') {
                Some((*model).to_string())
            } else {
                model
                    .strip_prefix(&format!("{provider}:"))
                    .map(str::to_string)
            }
        })
        .collect();
    let extras: &[&str] = match provider {
        "zai" => &["glm-4.6"],
        "deepseek" => &["deepseek-v4-pro"],
        "kimi" => &["kimi-k2.5"],
        "minimax" => &["MiniMax-M2.5-highspeed", "MiniMax-M3"],
        "qwen" => &["qwen3-coder-plus", "qwen-plus"],
        "xai" => &["grok-4"],
        "mistral" => &["mistral-small-latest"],
        _ => &[],
    };
    for model in extras {
        if !suggestions.iter().any(|existing| existing == model) {
            suggestions.push((*model).to_string());
        }
    }
    suggestions
}
