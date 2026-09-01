//! Asking a local LLM server to drop the model it is holding in memory.
//!
//! A loaded 7B model is roughly 5 GB of RAM, and a 70B is most of a workstation. When the
//! user moves to a cloud backend, that memory is doing nothing but keeping their machine
//! slow — so the moment they switch away, the local server is asked to let it go.
//!
//! Only two of these servers can actually be told. That is stated plainly rather than
//! papered over, because "we tried" and "it worked" are different facts and the user is
//! entitled to know which one they got:
//!
//! * **Ollama** — documented and reliable. `keep_alive: 0` on a generate call unloads the
//!   model immediately.
//! * **LM Studio** — its newer REST surface accepts an unload; older builds do not, and
//!   there is no way to tell them apart except by asking.
//! * **llama.cpp, vLLM, Jan, text-generation-webui, Bionic** — one model, loaded at
//!   startup by design, with no unload in the OpenAI-compatible surface they expose.
//!   Nothing here can free that; the honest answer is to say so and name what would.
//!
//! Nothing in this module ever *starts* anything, and every failure is soft: ejection is
//! a courtesy to the user's RAM, never a precondition for the provider switch that
//! triggered it.

use std::time::Duration;

/// Ejection is best-effort and must never delay the provider switch behind it.
const EJECT_TIMEOUT: Duration = Duration::from_secs(4);

/// What an ejection attempt achieved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ejected {
    /// The server confirmed it released the model.
    Freed { provider: String, model: String },
    /// The server is reachable but has no way to unload on request.
    NotSupported { provider: String, why: String },
    /// Nothing was holding memory in the first place.
    NothingLoaded,
    /// The attempt failed. Not an error the user needs to act on.
    Failed { provider: String, reason: String },
}

impl Ejected {
    /// One line for the log and, when it matters, for the user.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Freed { provider, model } => {
                format!("{provider} released {model} from memory.")
            }
            Self::NotSupported { provider, why } => {
                format!("{provider} kept its model loaded — {why}")
            }
            Self::NothingLoaded => "No local model was loaded.".to_owned(),
            Self::Failed { provider, reason } => {
                format!("Could not ask {provider} to unload: {reason}")
            }
        }
    }

    /// True only when memory was actually released.
    #[must_use]
    pub const fn freed(&self) -> bool {
        matches!(self, Self::Freed { .. })
    }
}

/// Asks the local server behind `provider_id` to release `model`.
///
/// `port` is the port detection actually got an answer on — never a guess, because
/// posting an unload to whatever happens to be listening on 1234 is not something to do
/// speculatively.
pub async fn eject(provider_id: &str, label: &str, port: u16, model: Option<&str>) -> Ejected {
    let Some(model) = model.filter(|name| !name.trim().is_empty()) else {
        return Ejected::NothingLoaded;
    };
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    match provider_id {
        "ollama" => ollama_unload(&client, &base, label, model).await,
        "lmstudio" => lmstudio_unload(&client, &base, label, model).await,
        other => Ejected::NotSupported {
            provider: label.to_owned(),
            why: unsupported_reason(other).to_owned(),
        },
    }
}

/// Ollama's documented unload: a generate call with a zero keep-alive.
///
/// Deliberately `/api/generate` with an empty prompt rather than a chat turn — it loads
/// nothing, generates nothing, and exists precisely to change residency.
async fn ollama_unload(client: &reqwest::Client, base: &str, label: &str, model: &str) -> Ejected {
    let body = serde_json::json!({
        "model": model,
        "prompt": "",
        "keep_alive": 0,
    });
    let request = client
        .post(format!("{base}/api/generate"))
        .json(&body)
        .timeout(EJECT_TIMEOUT)
        .send();

    match tokio::time::timeout(EJECT_TIMEOUT, request).await {
        Ok(Ok(response)) if response.status().is_success() => Ejected::Freed {
            provider: label.to_owned(),
            model: model.to_owned(),
        },
        Ok(Ok(response)) => Ejected::Failed {
            provider: label.to_owned(),
            reason: format!("answered HTTP {}", response.status().as_u16()),
        },
        Ok(Err(error)) => Ejected::Failed {
            provider: label.to_owned(),
            reason: error.to_string(),
        },
        Err(_) => Ejected::Failed {
            provider: label.to_owned(),
            reason: format!("no answer within {}s", EJECT_TIMEOUT.as_secs()),
        },
    }
}

/// LM Studio's REST unload, which only newer builds expose.
///
/// Both known paths are tried because the surface moved between releases and there is no
/// version handshake that would let us choose correctly in advance. A 404 from both is
/// not a failure — it is an older build, which is a different thing and is said as such.
async fn lmstudio_unload(
    client: &reqwest::Client,
    base: &str,
    label: &str,
    model: &str,
) -> Ejected {
    let body = serde_json::json!({ "model": model });
    let mut last = String::new();

    for path in ["/api/v0/models/unload", "/api/v1/models/unload"] {
        let request = client
            .post(format!("{base}{path}"))
            .json(&body)
            .timeout(EJECT_TIMEOUT)
            .send();
        match tokio::time::timeout(EJECT_TIMEOUT, request).await {
            Ok(Ok(response)) if response.status().is_success() => {
                return Ejected::Freed {
                    provider: label.to_owned(),
                    model: model.to_owned(),
                }
            }
            Ok(Ok(response)) => last = format!("HTTP {}", response.status().as_u16()),
            Ok(Err(error)) => last = error.to_string(),
            Err(_) => last = "timed out".to_owned(),
        }
    }

    Ejected::NotSupported {
        provider: label.to_owned(),
        why: format!(
            "this build exposes no unload endpoint ({last}). Eject it from LM Studio's \
             own model tray, or run `lms unload --all`."
        ),
    }
}

/// Why a given server cannot be told to unload, and what would work instead.
fn unsupported_reason(provider_id: &str) -> &'static str {
    match provider_id {
        "llamacpp" => {
            "llama.cpp's server loads one model at startup and exposes no unload. Stop \
             the server process to free the memory."
        }
        "vllm" => {
            "vLLM holds its model for the lifetime of the process. Stop the server to \
             free the memory."
        }
        "jan" => "Jan manages residency itself — unload from its own model panel.",
        "tgui" => {
            "text-generation-webui unloads from its Model tab, not over the API it \
             exposes here."
        }
        "bionic" => {
            "Bionic exposes no unload over its local API. Close it, or unload from its \
             own model panel, to free the memory."
        }
        _ => "this server exposes no unload endpoint.",
    }
}

#[cfg(test)]
mod tests {
    use super::{eject, unsupported_reason, Ejected};

    /// Nothing loaded is not a failure, and must not produce a message that reads like one.
    #[tokio::test]
    async fn no_model_means_nothing_to_eject() {
        assert_eq!(
            eject("ollama", "Ollama", 11434, None).await,
            Ejected::NothingLoaded
        );
        assert_eq!(
            eject("ollama", "Ollama", 11434, Some("   ")).await,
            Ejected::NothingLoaded
        );
        assert!(!Ejected::NothingLoaded.freed());
    }

    /// A server that genuinely cannot unload must say so and name what would work —
    /// never silently claim success it did not achieve.
    #[tokio::test]
    async fn a_server_without_an_unload_endpoint_says_so_and_names_the_fix() {
        let outcome = eject("llamacpp", "llama.cpp server", 8080, Some("qwen3")).await;
        let Ejected::NotSupported { why, .. } = &outcome else {
            panic!("llama.cpp cannot unload and must report that: {outcome:?}");
        };
        assert!(why.contains("Stop the server"), "{why}");
        assert!(!outcome.freed(), "nothing was actually freed");
        assert!(outcome.describe().contains("kept its model loaded"));
    }

    /// Every server this app can detect must have an honest answer, so a switch never
    /// produces an unexplained silence.
    #[test]
    fn every_local_server_has_a_stated_reason() {
        for id in ["llamacpp", "vllm", "jan", "tgui", "bionic", "something-new"] {
            let reason = unsupported_reason(id);
            assert!(!reason.is_empty(), "{id} has no explanation");
            assert!(reason.ends_with('.'), "{id}: {reason}");
        }
    }

    /// An unreachable server is a soft failure — the provider switch behind it must not
    /// be blocked, and the wording must not read as an error the user has to act on.
    #[tokio::test]
    async fn an_unreachable_server_fails_softly() {
        // Port 1 is reserved and never listening, so this exercises the failure path
        // without depending on what happens to be running on the machine.
        let outcome = eject("ollama", "Ollama", 1, Some("llama3")).await;
        let Ejected::Failed { reason, .. } = &outcome else {
            panic!("an unreachable server must report a failure: {outcome:?}");
        };
        assert!(!reason.is_empty());
        assert!(!outcome.freed());
        assert!(outcome.describe().starts_with("Could not ask"));
    }
}
