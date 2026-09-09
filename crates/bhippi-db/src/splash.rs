//! Favourited splash screens (GAD-161).
//!
//! A splash is built into one game, but a splash its author likes is a house style they will
//! want again. These rows are per user and across projects, so deleting a game never takes
//! the studio's saved looks with it.
//!
//! Facts only. The spec travels as the JSON `bhippi-engine::godot::splash` produced; this
//! crate never parses it, because the shape belongs to the engine and nothing here queries
//! by any field inside it.

use crate::db_error;
use crate::repositories::RepoDb;
use bhippi_types::{Result, Timestamp};

/// One saved splash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplashFavouriteRecord {
    /// ULID, so a list sorted by id is a list sorted by when it was saved.
    pub id: String,
    pub name: String,
    /// The `SplashSpec` as the engine serialises it.
    pub spec_json: String,
    /// The project it was saved from, forward-slashed. Empty when unknown.
    pub origin: String,
    pub created_at: String,
}

/// A favourite on its way in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSplashFavourite {
    pub id: String,
    pub name: String,
    pub spec_json: String,
    pub origin: String,
}

#[derive(Clone)]
pub struct SplashRepo {
    db: RepoDb,
}

impl SplashRepo {
    pub(crate) const fn new(db: RepoDb) -> Self {
        Self { db }
    }

    /// Save a splash, or rename and refresh one already saved under the same id.
    ///
    /// Re-saving is an update rather than a second row: favouriting the same splash twice is
    /// something a person does by accident, and two identical entries in the list is the
    /// result nobody wants.
    pub async fn save(&self, entry: &NewSplashFavourite, now: &Timestamp) -> Result<()> {
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO splash_favourites (id, name, spec_json, origin, created_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                 name      = excluded.name,
                 spec_json = excluded.spec_json,
                 origin    = excluded.origin"#,
            entry.id,
            entry.name,
            entry.spec_json,
            entry.origin,
            now,
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "save splash favourite"))?;
        Ok(())
    }

    /// Saved splashes, newest first.
    pub async fn list(&self, limit: i64) -> Result<Vec<SplashFavouriteRecord>> {
        let rows = sqlx::query!(
            r#"SELECT id, name, spec_json, origin, created_at
               FROM splash_favourites
               ORDER BY created_at DESC, id DESC
               LIMIT ?"#,
            limit
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list splash favourites"))?;
        Ok(rows
            .into_iter()
            .map(|row| SplashFavouriteRecord {
                id: row.id,
                name: row.name,
                spec_json: row.spec_json,
                origin: row.origin,
                created_at: row.created_at,
            })
            .collect())
    }

    /// Forget one favourite. Returns whether a row was actually removed.
    pub async fn remove(&self, id: &str) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM splash_favourites WHERE id = ?", id)
            .execute(&self.db.writer)
            .await
            .map_err(|error| db_error(error, "remove splash favourite"))?;
        Ok(result.rows_affected() == 1)
    }
}
