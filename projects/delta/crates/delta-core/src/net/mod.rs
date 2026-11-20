//! Outbound network policy. One decision, every exit.
//!
//! Delta leaves the machine over four different doors: the model provider calls
//! ([`crate::provider`]), the web tools ([`crate::web::fetch`] / [`crate::web::search`]),
//! the local voice-model download (`delta-stt`), and the controlled browser
//! (`delta-computer`). Before this module each door had its own idea of what "the
//! network" meant, and the mismatch was user-visible: the browser honoured the
//! system proxy while `web_fetch` did not, so on a machine that needs a proxy the
//! search failed and the model went looking for another way out — usually the
//! browser. That asymmetry, not a badly written prompt, is what produced
//! "asking what day it is opens a browser".
//!
//! The policy lives here; the mechanics stay at each door. That split is forced by
//! a version split we cannot paper over: `delta-core` pins ureq 2 (`AgentBuilder`)
//! while `delta-stt` pins ureq 3 (`config::Config`). A shared builder type would be
//! a lie, so what is shared is the *decision* — which mode, which URL, which host
//! bypasses it — and each call site applies that decision with its own API.

pub mod proxy;

pub use proxy::ProxySettings;
