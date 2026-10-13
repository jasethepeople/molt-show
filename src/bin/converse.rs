//! `cargo run --bin converse -- --rounds 6 --population experiments/emergence-001/population.json`
//!
//! Boots the agents named in a population file, gives each one nothing but
//! its own identity and a one-line persona seed, and lets them take turns.
//! No objective is loaded, no organization is prescribed, no format is
//! imposed on what they say. Every turn becomes a signed MESSAGE_SENT event
//! in the store — the resulting JSONL is the raw transcript, nothing
//! post-processed.
//!
//! Select a backend with `--backend anthropic|ollama|mock`. Ollama uses the
//! shell-to-model map in `--models` (defaulting to the show-001 map), while
//! `--model` overrides that map for every agent. Anthropic reads its key from
//! `ANTHROPIC_API_KEY`; Ollama reads `OLLAMA_BASE_URL` or defaults to the local
//! server at `http://127.0.0.1:11434`.
//! A fresh run may also use `--question TEXT` to append a signed
//! `VIEWER_QUESTION_POSTED` data event before the first turn. The question is
//! length-limited and shown to agents inside quoted data delimiters.
//!
//! ROLE SLOT. The persona seed for every agent is generated from
//! population.json alone, identically every run. On top of that, this build
//! reads an optional role file (`--roles`, default
//! experiments/emergence-001/roles.json) keyed by agent_id, whose `role`,
//! `role_goals`, and `role_behavior` strings are appended verbatim to that
//! agent's seed. That is the single knob for deciding what role a given agent
//! plays — agent A01 (agent 1, the "Prophet" slot) ships with placeholders
//! you replace. Role text is seed text only: it never enters genesis,
//! identity, or any event, and an absent role leaves the baseline seed
//! byte-for-byte unchanged.

use molt_kernel::events::create_signed_event;
use molt_kernel::genesis::create_genesis;
use molt_kernel::identity::Identity;
use molt_kernel::llm_agent::{AnthropicClient, LlmAgentShell, MockClient, OllamaClient};
use molt_kernel::memory::FileMemoryStore;
use molt_kernel::runtime::LocalWorld;
use molt_kernel::store::{EventStore, FileEventStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;

#[derive(Deserialize)]
struct PopulationFile {
    agents: Vec<PopulationAgent>,
}

#[derive(Deserialize)]
struct PopulationAgent {
    agent_id: String,
    shell: String,
    capabilities: Vec<String>,
}

/// One editable role slot, keyed by agent_id in the role file. Every field is
/// optional and additive: leaving a field out — or omitting the agent, or the
/// whole file — means that agent keeps the exact baseline persona seed.
#[derive(Deserialize, Clone, Default)]
struct RoleSlot {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    temperament: Option<String>,
    #[serde(default)]
    role_goals: Option<Vec<String>>,
    #[serde(default)]
    role_behavior: Option<String>,
}

#[derive(Deserialize, Clone)]
struct ModelSlot {
    model: String,
}

#[derive(Deserialize, Default)]
struct ModelsFile {
    #[serde(default)]
    shells: BTreeMap<String, ModelSlot>,
}

/// What gets persisted to `--identities` so a population can be resumed
/// with the same cryptographic identities across process restarts, instead
/// of a fresh genesis — and fresh, unrelated agents — every run.
#[derive(Serialize, Deserialize)]
struct SavedIdentities(BTreeMap<String, String>);

fn option(args: &[String], name: &str, default: &str) -> String {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .unwrap_or_else(|| default.into())
}

/// Loads identities from disk if `identities_path` exists, otherwise
/// generates fresh ones and writes them out. Returns the identities in
/// `agent_ids` order plus whether they were freshly generated this call —
/// the caller uses that to decide whether a genesis event is still needed.
fn load_or_create_identities(
    identities_path: &str,
    agent_ids: &[String],
) -> Result<(Vec<Identity>, bool), String> {
    if let Ok(raw) = fs::read_to_string(identities_path) {
        let saved: SavedIdentities = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let mut identities = Vec::new();
        for id in agent_ids {
            let hex_key = saved
                .0
                .get(id)
                .ok_or_else(|| format!("{identities_path} has no saved key for agent {id}"))?;
            let bytes = hex::decode(hex_key).map_err(|e| e.to_string())?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| format!("saved key for {id} is not 32 bytes"))?;
            identities.push(Identity::from_signing_key_bytes(id.clone(), &arr));
        }
        return Ok((identities, false));
    }

    let identities: Vec<Identity> = agent_ids.iter().map(Identity::generate).collect();
    let saved = SavedIdentities(
        agent_ids
            .iter()
            .zip(&identities)
            .map(|(id, identity)| (id.clone(), hex::encode(identity.signing_key_bytes())))
            .collect(),
    );
    fs::write(identities_path, serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok((identities, true))
}

/// Builds an agent's one-line persona seed. With `slot` = None this is the
/// baseline seed, unchanged; a role slot only appends role text to it.
fn persona_seed(
    agent_def: &PopulationAgent,
    population_size: usize,
    slot: Option<&RoleSlot>,
) -> String {
    let mut persona = format!(
        "You are agent {} ({} shell). You have these declared capabilities: {}. \
         You are one of {} agents in a shared environment with no assigned objective \
         and no assigned organization. What you do with that is up to you.",
        agent_def.agent_id,
        agent_def.shell,
        agent_def.capabilities.join(", "),
        population_size,
    );
    if let Some(slot) = slot {
        if let Some(role) = slot.role.as_deref() {
            persona.push_str(&format!(" Your role is: {role}."));
        }
        if let Some(temperament) = slot.temperament.as_deref() {
            persona.push_str(&format!(" Your temperament is: {temperament}"));
        }
        if let Some(goals) = slot.role_goals.as_ref().filter(|g| !g.is_empty()) {
            persona.push_str(&format!(" Your role's goals are: {}.", goals.join("; ")));
        }
        if let Some(behavior) = slot.role_behavior.as_deref() {
            persona.push_str(&format!(" Your role's behavior is: {behavior}."));
        }
    }
    persona
}

fn client_for(
    backend: &str,
    model_override: &str,
    shell: &str,
    models: &ModelsFile,
) -> Result<Box<dyn molt_kernel::llm_agent::LlmClient>, String> {
    match backend {
        "ollama" => {
            let model = if model_override.is_empty() {
                models
                    .shells
                    .get(shell)
                    .map(|slot| slot.model.clone())
                    .ok_or_else(|| format!("no Ollama model configured for shell {shell}"))?
            } else {
                model_override.to_string()
            };
            println!("  {shell} -> Ollama/{model}");
            Ok(Box::new(OllamaClient::from_env(model)))
        }
        "anthropic" => {
            let model = if model_override.is_empty() {
                "claude-sonnet-4-6".to_string()
            } else {
                model_override.to_string()
            };
            AnthropicClient::from_env(model)
                .map(|client| Box::new(client) as Box<dyn molt_kernel::llm_agent::LlmClient>)
                .ok_or_else(|| "ANTHROPIC_API_KEY is not set".to_string())
        }
        "mock" => Ok(Box::new(MockClient {
            fixed_reply: "[mock reply]".to_string(),
        })),
        other => Err(format!(
            "unknown backend {other}; expected ollama, anthropic, or mock"
        )),
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let population_path = option(&args, "--population", "experiments/emergence-001/population.json");
    let roles_path = option(&args, "--roles", "experiments/emergence-001/roles.json");
    let store_path = option(&args, "--store", "events.jsonl");
    let memory_path = option(&args, "--memory", "memory.jsonl");
    let identities_path = option(&args, "--identities", "identities.json");
    let backend = option(&args, "--backend", "anthropic");
    let models_path = option(&args, "--models", "experiments/show-001/models.json");
    let question = option(&args, "--question", "");
    let rounds: u32 = option(&args, "--rounds", "6")
        .parse()
        .map_err(|_| "--rounds must be an integer".to_string())?;
    let model = option(&args, "--model", "");

    let raw = fs::read_to_string(&population_path).map_err(|e| e.to_string())?;
    let population: PopulationFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let agent_ids: Vec<String> = population.agents.iter().map(|a| a.agent_id.clone()).collect();

    // The role file is optional. A missing file means baseline: every agent
    // keeps the unmodified seed generated from population.json.
    let roles: BTreeMap<String, RoleSlot> = match fs::read_to_string(&roles_path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("{roles_path}: {e}"))?,
        Err(_) => BTreeMap::new(),
    };

    let models: ModelsFile = match fs::read_to_string(&models_path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("{models_path}: {e}"))?,
        Err(error) if backend == "ollama" => {
            return Err(format!("cannot load Ollama model map {models_path}: {error}"));
        }
        Err(_) => ModelsFile::default(),
    };

    println!("backend: {backend}");

    let mut store = FileEventStore::open(&store_path).map_err(|e| e.to_string())?;
    let memory = FileMemoryStore::open(&memory_path).map_err(|e| e.to_string())?;
    let store_has_history = !store.all().map_err(|e| e.to_string())?.is_empty();
    if !question.trim().is_empty() {
        if store_has_history {
            return Err(format!(
                "cannot seed --question into existing history at {store_path}; use a fresh store"
            ));
        }
        let clean_question = question.trim();
        if clean_question.chars().count() > 1000 {
            return Err("--question must be at most 1000 characters".to_string());
        }
        let question_identity = Identity::generate("viewer-intake");
        let question_event = create_signed_event(
            &question_identity,
            "VIEWER_QUESTION_POSTED",
            "MOLT/0.1",
            0,
            0,
            &molt_kernel::clock::now_rfc3339(),
            vec![],
            serde_json::json!({
                "question_id": "seed-001",
                "source": "viewer",
                "question": clean_question
            }),
            vec![],
            vec![],
        );
        store
            .append(question_event.clone())
            .map_err(|e| e.to_string())?;
        println!("seeded viewer question: {}", question_event.event_id);
    }
    let mut world = LocalWorld::new("emergence-001", store, memory);

    let (identities, freshly_generated) = load_or_create_identities(&identities_path, &agent_ids)?;

    // A fresh set of identities against a store that already has events
    // means identities.json was lost or pointed somewhere else — resuming
    // would silently start signing a history that belongs to different
    // agents. Refuse rather than guess.
    if freshly_generated && store_has_history {
        return Err(format!(
            "{store_path} already has events but {identities_path} was just generated — \
             this looks like a mismatched resume. Point --identities at the file that was \
             used to create {store_path}, or start both fresh with new paths."
        ));
    }

    if !store_has_history {
        let attester = Identity::generate("genesis-attester");
        let genesis_event = create_genesis(
            &attester,
            "MOLT/0.1",
            &identities,
            &molt_kernel::clock::now_rfc3339(),
        );
        println!("genesis: {}", genesis_event.event_id);
    } else {
        println!("resuming existing history at {store_path}");
    }

    if roles.is_empty() {
        println!("no role slots at {roles_path} — every agent runs on its baseline seed");
    } else {
        let mut named: Vec<String> = roles.keys().cloned().collect();
        named.sort();
        println!("role slots loaded from {roles_path} for: {}", named.join(", "));
    }

    for (identity, agent_def) in identities.iter().zip(population.agents.iter()) {
        let peers: Vec<String> = agent_ids
            .iter()
            .filter(|other| *other != &agent_def.agent_id)
            .cloned()
            .collect();
        // The seed is identity + declared capabilities, plus — only when this
        // agent has a role slot — that role's text. No task, no tone, no
        // instructions on how to use the other agents. It is regenerated
        // identically every run (including resumed ones) from population.json
        // plus the role file, so neither can drift out of sync with the
        // store. Role text is seed text; it is never persisted as identity.
        let persona = persona_seed(agent_def, agent_ids.len(), roles.get(&agent_def.agent_id));
        let client = client_for(&backend, &model, &agent_def.shell, &models)?;
        let shell = LlmAgentShell::new(agent_def.agent_id.clone(), persona, peers, client);
        world
            .register_agent(identity.clone(), Box::new(shell))
            .map_err(|e| e.to_string())?;
    }

    for round in 0..rounds {
        for id in &agent_ids {
            let event = world.decide_and_commit(id).map_err(|e| e.to_string())?;
            println!("round {round} — {id} — event {}", event.event_id);
            for other in &agent_ids {
                if other != id {
                    world.receive_next(other).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    println!("done ({rounds} round(s) this run). transcript at {store_path}");
    println!("re-run the same command to continue where this left off.");
    Ok(())
}
