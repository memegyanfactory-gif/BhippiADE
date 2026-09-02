//! The provider catalogue: every backend Bhippi knows, with how to find, install,
//! and talk to it. Data-driven so vendor commands change in exactly one place (R11).

use crate::model::ProviderKind;
use crate::transcript::Transcript;

/// How to install (and thereby update) one CLI provider. Explicit argv, never a shell
/// string (INV-003).
#[derive(Clone, Copy, Debug)]
pub struct InstallSpec {
    pub program: &'static str,
    pub args: &'static [&'static str],
}

/// One known backend.
#[derive(Clone, Copy, Debug)]
pub struct ProviderSpec {
    /// Stable id used in config prefs, logs, and the UI (`providers.enabled`).
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ProviderKind,
    /// CLI executable name (without platform suffix).
    pub binary: Option<&'static str>,
    /// Credential environment variable — presence only, never read into storage (INV-002).
    pub env_key: Option<&'static str>,
    /// Loopback port probed for local servers.
    pub port: Option<u16>,
    /// HTTP path probed for local servers.
    pub probe_path: Option<&'static str>,
    /// Install/update recipe; `None` for servers and cloud rows.
    pub install: Option<InstallSpec>,
    /// Prompt argv template following spec §8.1a; `{prompt}` is substituted verbatim.
    pub prompt_args: Option<&'static [&'static str]>,
    /// Argv fragment that pins a model, with `{model}` substituted. `None` means this
    /// backend takes no model flag, so the picker leaves it on the vendor's default.
    pub model_args: Option<&'static [&'static str]>,
    /// Argv that makes the CLI print the models it accepts, read once per detection so
    /// the picker offers the vendor's own live catalogue rather than a list we maintain
    /// by hand and get wrong the week the vendor ships a new model.
    pub list_models_args: Option<&'static [&'static str]>,
    /// How this backend's stdout is read back into an answer.
    pub transcript: Transcript,
    /// How many tokens this backend can read in one turn.
    ///
    /// Used for the pre-send budget guard, so a prompt that cannot possibly fit is
    /// refused before it is paid for rather than after the vendor rejects it. These are
    /// the vendor's published figures for the default model; a smaller model pinned in
    /// the picker only makes the real ceiling lower, and the guard is deliberately a
    /// floor check rather than an exact accounting.
    pub context_window: u32,
    /// Whether this backend can read images (ADR-0015 gates computer use on it).
    pub vision: bool,
    /// Model names this backend is *known* to accept, offered in the composer's picker.
    ///
    /// This is the fallback, not the primary source: a backend with `list_models_args`
    /// overrides it with whatever the vendor prints. Empty is the honest default for a
    /// backend that can be neither asked nor documented — local servers fill their list
    /// by probing at detection time, and for everything else the picker offers a
    /// free-text field. A guessed model id would fail at the vendor and read like our
    /// bug, so we do not guess.
    pub models: &'static [&'static str],
}

const NPM: &str = "npm";

/// The full catalogue, ordered for the Settings page: agents, locals, clouds, demo.
pub const CATALOG: &[ProviderSpec] = &[
    ProviderSpec {
        id: "claude",
        label: "Claude Code",
        kind: ProviderKind::Cli,
        binary: Some("claude"),
        env_key: None,
        port: None,
        probe_path: None,
        install: Some(InstallSpec {
            program: NPM,
            args: &["install", "-g", "@anthropic-ai/claude-code"],
        }),
        // `stream-json` over `json`, and the difference is the whole complaint that
        // "Claude takes too much time": under `json` the CLI buffers the entire turn and
        // prints it on exit, so a ninety-second answer is ninety seconds of blank screen
        // followed by a wall of text. `stream-json` prints events as they happen, and
        // `--include-partial-messages` makes those events token-level, so the first
        // words land in about a second. `--verbose` is not optional — Claude Code
        // requires it for stream-json under `--print`.
        //
        // `--strict-mcp-config` with no `--mcp-config` alongside it means "load no MCP
        // servers at all". A chat turn needs none, and booting a project's servers is
        // dead time on every single turn before the model has even been asked anything.
        prompt_args: Some(&[
            "-p",
            "{prompt}",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--strict-mcp-config",
        ]),
        model_args: Some(&["--model", "{model}"]),
        // Claude Code prints no catalogue; these are the aliases its own `--help`
        // documents, and the free-text field still takes a full `claude-*` id.
        list_models_args: None,
        transcript: Transcript::JsonLines,
        context_window: 200_000,
        vision: true,
        models: &["fable", "opus", "sonnet", "haiku"],
    },
    ProviderSpec {
        id: "codex",
        label: "Codex CLI",
        kind: ProviderKind::Cli,
        binary: Some("codex"),
        env_key: None,
        port: None,
        probe_path: None,
        install: Some(InstallSpec {
            program: NPM,
            args: &["install", "-g", "@openai/codex"],
        }),
        // `--skip-git-repo-check` is load-bearing, not tidy-up: Codex refuses to run
        // outside a directory it already trusts, and the chat workspace is never one.
        // Without it every Codex turn dies on "Not inside a trusted directory", which
        // reads like our bug. `--json` is what keeps the answer clean — Codex's prose
        // mode prefixes a workdir/model/session banner to every reply.
        prompt_args: Some(&[
            "exec",
            "--skip-git-repo-check",
            "--json",
            "--color",
            "never",
            "{prompt}",
        ]),
        model_args: Some(&["-m", "{model}"]),
        list_models_args: Some(&["debug", "models"]),
        transcript: Transcript::JsonLines,
        context_window: 272_000,
        vision: true,
        models: &[],
    },
    ProviderSpec {
        id: "opencode",
        label: "OpenCode",
        kind: ProviderKind::Cli,
        binary: Some("opencode"),
        env_key: None,
        port: None,
        probe_path: None,
        install: Some(InstallSpec {
            program: NPM,
            args: &["install", "-g", "opencode-ai"],
        }),
        // `--format json` drops the `> agent · model` banner prose mode prints, and
        // hands back real token counts for the usage ledger.
        // `--auto` auto-approves permissions so headless execution does not halt on closed stdin.
        // `--pure` disables external unmanaged plugins that can fail or hang.
        // `--thinking` gives the reasoning drawer events to stream.
        prompt_args: Some(&[
            "run",
            "--format",
            "json",
            "--auto",
            "--pure",
            "--thinking",
            "{prompt}",
        ]),
        model_args: Some(&["-m", "{model}"]),
        list_models_args: Some(&["models"]),
        transcript: Transcript::JsonLines,
        context_window: 200_000,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "grok",
        label: "Grok CLI",
        kind: ProviderKind::Cli,
        binary: Some("grok"),
        env_key: None,
        port: None,
        probe_path: None,
        install: Some(InstallSpec {
            program: NPM,
            args: &["install", "-g", "@xai-official/grok"],
        }),
        // Headless, not the TUI. `-p` still means `--single`, but without
        // `--output-format streaming-json` Grok prints nothing until MCP servers
        // (npx remotion, watchfiwn, project `.mcp.json`) finish starting — often
        // past our idle timeout, which the UI reads as "unable to connect".
        // `--no-leader` keeps this turn off the user's already-running Grok TUI.
        // `--always-approve` matches `[ui] permission_mode = always-approve` so a
        // desktop spawn with stdin closed does not sit forever on a permission prompt.
        prompt_args: Some(&[
            "-p",
            "{prompt}",
            "--output-format",
            "streaming-json",
            "--permission-mode",
            "dontAsk",
            "--always-approve",
            "--no-leader",
            "--verbatim",
            "--disallowed-tools",
            "Agent",
        ]),
        model_args: Some(&["--model", "{model}"]),
        list_models_args: Some(&["models"]),
        transcript: Transcript::JsonLines,
        context_window: 256_000,
        vision: true,
        models: &[],
    },
    ProviderSpec {
        id: "kimi",
        label: "Kimi CLI",
        kind: ProviderKind::Cli,
        binary: Some("kimi"),
        env_key: None,
        port: None,
        probe_path: None,
        install: Some(InstallSpec {
            program: NPM,
            args: &["install", "-g", "@moonshot-ai/kimi-code"],
        }),
        prompt_args: Some(&["-p", "{prompt}"]),
        model_args: Some(&["--model", "{model}"]),
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 256_000,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "bionic",
        label: "Bionic",
        kind: ProviderKind::LocalServer,
        binary: Some("bionic"),
        env_key: None,
        port: Some(7432),
        probe_path: Some("/v1/models"),
        install: Some(InstallSpec {
            program: NPM,
            args: &["install", "-g", "@bionic-ai/cli"],
        }),
        prompt_args: Some(&["run", "--format", "json", "{prompt}"]),
        model_args: Some(&["-m", "{model}"]),
        list_models_args: Some(&["models"]),
        transcript: Transcript::JsonLines,
        context_window: 128_000,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "ollama",
        label: "Ollama",
        kind: ProviderKind::LocalServer,
        binary: Some("ollama"),
        env_key: None,
        port: Some(11434),
        probe_path: Some("/api/tags"),
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 32_768,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "lmstudio",
        label: "LM Studio",
        kind: ProviderKind::LocalServer,
        binary: None,
        env_key: None,
        port: Some(1234),
        probe_path: Some("/v1/models"),
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 32_768,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "llamacpp",
        label: "llama.cpp server",
        kind: ProviderKind::LocalServer,
        binary: None,
        env_key: None,
        port: Some(8080),
        probe_path: Some("/v1/models"),
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 32_768,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "vllm",
        label: "vLLM",
        kind: ProviderKind::LocalServer,
        binary: None,
        env_key: None,
        port: Some(8000),
        probe_path: Some("/v1/models"),
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 32_768,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "jan",
        label: "Jan",
        kind: ProviderKind::LocalServer,
        binary: None,
        env_key: None,
        port: Some(1337),
        probe_path: Some("/v1/models"),
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 32_768,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "tgui",
        label: "text-generation-webui",
        kind: ProviderKind::LocalServer,
        binary: None,
        env_key: None,
        port: Some(5000),
        probe_path: Some("/v1/models"),
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 32_768,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "anthropic",
        label: "Anthropic API",
        kind: ProviderKind::CloudApi,
        binary: None,
        env_key: Some("ANTHROPIC_API_KEY"),
        port: None,
        probe_path: None,
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 200_000,
        vision: true,
        models: &[],
    },
    ProviderSpec {
        id: "openai",
        label: "OpenAI API",
        kind: ProviderKind::CloudApi,
        binary: None,
        env_key: Some("OPENAI_API_KEY"),
        port: None,
        probe_path: None,
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 272_000,
        vision: true,
        models: &[],
    },
    ProviderSpec {
        id: "xai",
        label: "xAI API",
        kind: ProviderKind::CloudApi,
        binary: None,
        env_key: Some("XAI_API_KEY"),
        port: None,
        probe_path: None,
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 256_000,
        vision: true,
        models: &[],
    },
    ProviderSpec {
        id: "moonshot",
        label: "Moonshot API",
        kind: ProviderKind::CloudApi,
        binary: None,
        env_key: Some("MOONSHOT_API_KEY"),
        port: None,
        probe_path: None,
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 256_000,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "groq",
        label: "Groq API",
        kind: ProviderKind::CloudApi,
        binary: None,
        env_key: Some("GROQ_API_KEY"),
        port: None,
        probe_path: None,
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 131_072,
        vision: false,
        models: &[],
    },
    ProviderSpec {
        id: "openrouter",
        label: "OpenRouter API",
        kind: ProviderKind::CloudApi,
        binary: None,
        env_key: Some("OPENROUTER_API_KEY"),
        port: None,
        probe_path: None,
        install: None,
        prompt_args: None,
        model_args: None,
        list_models_args: None,
        transcript: Transcript::Plain,
        context_window: 128_000,
        vision: true,
        models: &[],
    },
];

/// Looks up one spec by id.
#[must_use]
pub fn spec(id: &str) -> Option<&'static ProviderSpec> {
    CATALOG.iter().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    use super::{spec, CATALOG};
    use crate::transcript::Transcript;

    #[test]
    fn ids_are_unique_and_known_providers_present() {
        let mut ids: Vec<_> = CATALOG.iter().map(|entry| entry.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate catalog ids");

        for wanted in [
            "claude", "codex", "opencode", "grok", "kimi", "bionic", "ollama", "lmstudio",
        ] {
            assert!(spec(wanted).is_some(), "{wanted} missing from catalog");
        }
        assert!(spec("demo").is_none(), "demo is built, not installed");
    }

    #[test]
    fn cli_agents_carry_install_and_prompt_recipes() {
        for id in ["claude", "codex", "opencode", "grok", "kimi", "bionic"] {
            let Some(entry) = spec(id) else {
                panic!("{id} missing from catalog");
            };
            let Some(install) = entry.install else {
                panic!("{id} lacks an install recipe");
            };
            assert!(!install.args.is_empty());
            let Some(prompt_args) = entry.prompt_args else {
                panic!("{id} lacks a prompt template");
            };
            assert!(
                prompt_args.contains(&"{prompt}"),
                "{id} prompt template misses the placeholder"
            );
        }
    }

    /// Regression pin. Codex refuses to run outside a directory it already trusts, and
    /// the chat workspace is never one. Dropping this flag makes every Codex turn fail
    /// with "Not inside a trusted directory", which reads like our bug.
    #[test]
    fn codex_runs_outside_a_trusted_directory() {
        let Some(codex) = spec("codex") else {
            panic!("codex missing from catalog");
        };
        let Some(args) = codex.prompt_args else {
            panic!("codex lacks a prompt template");
        };
        assert!(args.contains(&"--skip-git-repo-check"));
    }

    /// A backend whose stdout is JSON Lines must both ask its CLI for JSON and say it
    /// reads JSON. Editing one field without the other returns a wall of raw events to
    /// the user, or a JSON parser fed prose.
    #[test]
    fn json_line_backends_ask_their_cli_for_json() {
        for entry in CATALOG {
            let Some(args) = entry.prompt_args else {
                continue;
            };
            let asks_for_json = args.iter().any(|arg| arg.contains("json"));
            assert_eq!(
                asks_for_json,
                entry.transcript == Transcript::JsonLines,
                "{} asks for json={asks_for_json} but reads {:?}",
                entry.id,
                entry.transcript
            );
        }
    }

    /// A model list is only worth offering for a backend that can pin what it lists.
    #[test]
    fn every_listable_backend_can_pin_what_it_lists() {
        for entry in CATALOG {
            if entry.list_models_args.is_some() || !entry.models.is_empty() {
                assert!(
                    entry.model_args.is_some(),
                    "{} offers models it cannot pin",
                    entry.id
                );
            }
        }
    }
}
