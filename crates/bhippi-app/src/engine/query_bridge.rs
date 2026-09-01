//! The model's read surface (ENG-115).
//!
//! `SceneQueries` (ADR-0027) has been a complete, deterministic, 13-query read API since
//! 2026-09-01 with nothing but IPC wired to it, so the model was left guessing at the scene
//! from a digest. This answers `<engine_query>` payloads out of that same API.
//!
//! Everything here is **retrieval**: each answer is small and scoped. Dumping the project
//! into the context window is the failure mode this exists to avoid.

use super::{game_dir_of, scene_rel_of, sessions_lock};
use crate::commands::AppError;
use bhippi_engine::api::{EntityQuery, SceneQueries};
use bhippi_engine::asset::AssetIndex;
use bhippi_engine::document::SceneDocument;
use bhippi_types::{AssetId, EntityId};
use serde_json::Value;
use std::fmt::Write as _;
use std::str::FromStr;

/// How many entities a single find/list answer may return before it is truncated. Past
/// this the answer stops being retrieval and starts being a dump.
const MAX_ROWS: usize = 40;

/// Answer one `<engine_query>` payload. Errors come back as text too — the model needs to
/// read the failure, not have the turn aborted under it.
pub async fn answer_query(workspace: &str, payload: &str) -> String {
    match answer_inner(workspace, payload) {
        Ok(text) => text,
        Err(error) => match error.hint {
            Some(hint) => format!("query failed: {}\nhint: {hint}", error.message),
            None => format!("query failed: {}", error.message),
        },
    }
}

fn answer_inner(workspace: &str, payload: &str) -> Result<String, AppError> {
    let query: Value = serde_json::from_str(payload).map_err(|error| AppError {
        message: format!("that engine query is not valid JSON: {error}"),
        hint: Some("Use {\"kind\":\"entity\",\"entity\":\"Player\"}.".to_owned()),
    })?;
    let kind = query
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("scene")
        .to_owned();

    // Two query kinds need no scene at all, so they answer before anything is opened —
    // asking for a component's schema must work in an empty project.
    match kind.as_str() {
        "console" => {
            return Ok(super::telemetry::console_answer(
                query.get("level").and_then(Value::as_str),
                query.get("channel").and_then(Value::as_str),
                query.get("search").and_then(Value::as_str),
                query.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize,
                query.get("limit").and_then(Value::as_u64).unwrap_or(40) as usize,
            ))
        }
        "play_stats" => return Ok(super::telemetry::play_stats_answer()),
        "project" => {
            let game_dir = game_dir_of(workspace)?;
            let manifest = bhippi_engine::manifest::load_manifest(&game_dir)
                .map_err(super::engine_error)?
                .ok_or_else(|| AppError::plain("This workspace has no Bhippi.game.toml."))?;
            return Ok(format!(
                "Project {} v{}\ndefault scene: {}\nlevels: {}\ntargets: {}\nengine track: {:?}",
                manifest.game.name,
                manifest.game.version,
                manifest.game.default_scene,
                manifest.game.levels.join(", "),
                manifest.enabled_targets().join(", "),
                manifest.game.engine_track,
            ));
        }
        "schema" => {
            return Ok(schema_answer(
                query.get("component").and_then(Value::as_str),
            ))
        }
        "templates" => {
            let mut out = String::from("Spawn templates:\n");
            for spec in bhippi_engine::scaffold::templates() {
                let _ = writeln!(out, "- {} ({})", spec.name, spec.label);
            }
            return Ok(out);
        }
        "weather" => {
            let mut out = String::from("Weather presets:\n");
            for preset in bhippi_engine::weather::presets() {
                let _ = writeln!(out, "- {} ({})", preset.id, preset.label);
            }
            return Ok(out);
        }
        _ => {}
    }

    let game_dir = game_dir_of(workspace)?;
    let scene_rel = query.get("scene").and_then(Value::as_str);
    let rel = scene_rel_of(&game_dir, scene_rel)?;
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    let state = store
        .state(&game_dir, &rel)
        .ok_or_else(|| AppError::plain("The scene could not be opened."))?;
    let doc = store
        .document(&game_dir, &rel)
        .ok_or_else(|| AppError::plain("The scene could not be opened."))?
        .clone();
    drop(store);

    // The asset index is only scanned for the queries that actually resolve assets —
    // walking the project folder on every scene question is waste.
    let index = if matches!(
        kind.as_str(),
        "assets"
            | "asset_users"
            | "asset_dependencies"
            | "material_graph"
            | "shader"
            | "animation_graph"
    ) {
        AssetIndex::scan(&game_dir).ok()
    } else {
        None
    };
    let queries = match index.as_ref() {
        Some(index) => SceneQueries::with_assets(&doc, index),
        None => SceneQueries::new(&doc),
    };

    let text = match kind.as_str() {
        "scene" => {
            let view = queries.compact().get();
            let mut out = format!(
                "Scene {rel}\nname: {}\nkind: {:?}\nentities: {}\nroots: {}\nweather: {}\n",
                view.name,
                view.kind,
                view.entity_count,
                view.root_count,
                doc.settings.weather.as_deref().unwrap_or("none"),
            );
            out.push_str("\nHierarchy:\n");
            out.push_str(&bhippi_engine::mindmap::digest_text(&doc, 0));
            out
        }
        "selection" => {
            if state.selection.is_empty() {
                "Nothing is selected in the editor.".to_owned()
            } else {
                let mut out = String::from("Selected:\n");
                for id in &state.selection {
                    out.push_str(&entity_summary(&queries, *id));
                }
                out
            }
        }
        "entity" => {
            let id = resolve(&doc, &query)?;
            entity_summary(&queries.deep(), id)
        }
        "components" => {
            let id = resolve(&doc, &query)?;
            let view = queries
                .deep()
                .get_components(id)
                .ok_or_else(|| AppError::plain("That entity is not in the scene."))?;
            let mut out = format!("Components on {id}:\n");
            for name in &view.names {
                let payload = view
                    .payloads
                    .as_ref()
                    .and_then(|payloads| payloads.get(name))
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let _ = writeln!(out, "- {name}: {payload}");
            }
            out
        }
        "children" => {
            let id = resolve(&doc, &query)?;
            let view = queries
                .get_children(id)
                .ok_or_else(|| AppError::plain("That entity is not in the scene."))?;
            let mut out = format!("Children of {id} ({}):\n", view.ids.len());
            for child in view.ids.iter().take(MAX_ROWS) {
                out.push_str(&entity_line(&queries, *child));
            }
            out
        }
        "parent" => {
            let id = resolve(&doc, &query)?;
            match queries.get_parent(id).and_then(|view| view.parent) {
                Some(parent) => format!("Parent of {id}: {} ({})\n", parent.name, parent.id),
                None => format!("{id} is a root entity.\n"),
            }
        }
        "find" => {
            let filter = EntityQuery {
                name: query.get("name").and_then(Value::as_str).map(str::to_owned),
                tag: query.get("tag").and_then(Value::as_str).map(str::to_owned),
                has_component: query
                    .get("has_component")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                parent: None,
                roots_only: query
                    .get("roots_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            let hits = queries.find_entities(&filter);
            let mut out = format!("{} match(es):\n", hits.len());
            for hit in hits.iter().take(MAX_ROWS) {
                let _ = writeln!(out, "- {} — {}", hit.name, hit.stable_path);
            }
            if hits.len() > MAX_ROWS {
                let _ = writeln!(out, "… {} more; narrow the query.", hits.len() - MAX_ROWS);
            }
            out
        }
        "physics" => {
            let id = resolve(&doc, &query)?;
            match queries.get_physics(id) {
                Some(view) => format!("{view:?}\n"),
                None => format!("{id} has no physics components.\n"),
            }
        }
        "scripts" => {
            let id = resolve(&doc, &query)?;
            format!("Scripts on {id}: {:?}\n", queries.deep().get_scripts(id))
        }
        "asset_users" => {
            let id = resolve_asset(&query)?;
            format!("Asset users for {id}: {:?}\n", queries.deep().get_asset_users(id))
        }
        "asset_dependencies" => {
            let id = resolve_asset(&query)?;
            format!(
                "Asset dependencies for {id}: {:?}\n",
                queries.deep().get_asset_dependencies(id)
            )
        }
        "material_graph" => {
            let id = resolve_asset(&query)?;
            format!(
                "Material graph for {id}: {:?}\n",
                queries.deep().get_material_graph(id)
            )
        }
        "shader" => {
            let id = resolve_asset(&query)?;
            format!("Shader users for {id}: {:?}\n", queries.deep().get_shader(id))
        }
        "animation_graph" => {
            let id = resolve(&doc, &query)?;
            format!(
                "Animation graph for {id}: {:?}\n",
                queries.deep().get_animation_graph(id)
            )
        }
        "assets" => {
            let Some(index) = index.as_ref() else {
                return Ok("The asset index could not be read.".to_owned());
            };
            let wanted = query.get("kind_filter").and_then(Value::as_str);
            let search = query
                .get("search")
                .and_then(Value::as_str)
                .map(str::to_lowercase);
            let offset = query.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let limit = (query.get("limit").and_then(Value::as_u64).unwrap_or(MAX_ROWS as u64)
                as usize)
                .min(MAX_ROWS);
            let mut rows = Vec::new();
            for (id, record) in &index.assets {
                let kind = record.kind.to_string();
                if wanted.is_some_and(|want| want != kind) {
                    continue;
                }
                if search
                    .as_ref()
                    .is_some_and(|needle| !record.path_rel.to_lowercase().contains(needle))
                {
                    continue;
                }
                rows.push(format!("- {} [{}] {}", record.path_rel, kind, id));
            }
            let mut out = format!("{} asset(s):\n", rows.len());
            for row in rows.iter().skip(offset).take(limit) {
                out.push_str(row);
                out.push('\n');
            }
            if rows.len() > offset.saturating_add(limit) {
                let _ = writeln!(
                    out,
                    "… {} more; continue with offset {}.",
                    rows.len() - offset.saturating_add(limit),
                    offset.saturating_add(limit)
                );
            }
            out
        }
        other => format!(
            "unknown query kind {other:?}. Valid kinds: scene, selection, entity, components, \
             children, parent, find, physics, scripts, assets, asset_users, asset_dependencies, \
             material_graph, shader, animation_graph, schema, templates, weather, project, console, play_stats."
        ),
    };
    Ok(text)
}

fn resolve_asset(query: &Value) -> Result<AssetId, AppError> {
    let token = query
        .get("asset")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError {
            message: "this query needs an \"asset\" id".to_owned(),
            hint: Some("Run an assets query first, then pass its ULID.".to_owned()),
        })?;
    AssetId::from_str(token).map_err(|_| AppError {
        message: format!("{token:?} is not a valid asset id"),
        hint: Some("Run an assets query first, then pass its ULID.".to_owned()),
    })
}

fn schema_answer(component: Option<&str>) -> String {
    match component {
        Some(name) => bhippi_engine::schema::excerpt(name).unwrap_or_else(|| {
            format!(
                "unknown component {name:?}. Registered components: {}",
                bhippi_engine::schema::component_names().join(", ")
            )
        }),
        None => format!(
            "Registered components: {}\nAsk for one by name to see its fields.",
            bhippi_engine::schema::component_names().join(", ")
        ),
    }
}

/// Resolve the `entity` field: a ULID, a name, or a `scene:/Path#ULID` reference.
fn resolve(doc: &SceneDocument, query: &Value) -> Result<EntityId, AppError> {
    let token = query
        .get("entity")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError {
            message: "this query needs an \"entity\"".to_owned(),
            hint: Some("Pass a ULID, a name, or a scene:/Path#ULID reference.".to_owned()),
        })?;
    bhippi_engine::query::find_by_name(doc, token)
        .into_iter()
        .next()
        .or_else(|| doc.resolve_ref(token))
        .ok_or_else(|| AppError {
            message: format!("no entity matches {token:?}"),
            hint: Some("Run a find query first.".to_owned()),
        })
}

fn entity_line(queries: &SceneQueries<'_>, id: EntityId) -> String {
    queries
        .get_entity(id)
        .map(|view| format!("- {} — {}\n", view.name, view.stable_path))
        .unwrap_or_else(|| format!("- {id} (missing)\n"))
}

fn entity_summary(queries: &SceneQueries<'_>, id: EntityId) -> String {
    let Some(view) = queries.get_entity(id) else {
        return format!("{id} is not in the scene.\n");
    };
    let mut out = format!("{}\n  path: {}\n", view.name, view.stable_path);
    if !view.tags.is_empty() {
        let _ = writeln!(out, "  tags: {}", view.tags.join(", "));
    }
    let _ = writeln!(out, "  components: {}", view.component_names.join(", "));
    if let Some(components) = &view.components {
        for (name, payload) in components {
            let _ = writeln!(out, "    {name} = {payload}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::answer_inner;

    #[test]
    fn schema_queries_answer_without_a_project() {
        let text = answer_inner("", "{\"kind\":\"schema\",\"component\":\"Light\"}")
            .expect("schema needs no game");
        assert!(text.contains("intensity"));
        assert!(text.contains("directional"));
    }

    #[test]
    fn an_unknown_component_lists_the_registry() {
        let text = answer_inner("", "{\"kind\":\"schema\",\"component\":\"GravityGun\"}")
            .expect("answers");
        assert!(text.contains("unknown component"));
        assert!(
            text.contains("MeshRenderer"),
            "the real registry is offered"
        );
    }

    #[test]
    fn templates_and_weather_answer_from_the_engine_registries() {
        let templates = answer_inner("", "{\"kind\":\"templates\"}").expect("answers");
        assert!(templates.contains("cube"));
        assert!(templates.contains("trigger"));
        let weather = answer_inner("", "{\"kind\":\"weather\"}").expect("answers");
        assert!(weather.contains("overcast"));
        assert_eq!(weather.lines().count(), 9, "eight presets plus the header");
    }

    #[test]
    fn malformed_query_json_is_reported_not_panicked() {
        let error = answer_inner("", "{not json").expect_err("must reject");
        assert!(error.hint.is_some());
    }
}
