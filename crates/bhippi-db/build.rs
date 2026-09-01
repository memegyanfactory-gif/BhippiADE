use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=migrations");

    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("Cargo must provide OUT_DIR"));
    let schema_path = out_dir.join("query-schema.db");
    let migrations_path = Path::new("migrations");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| panic!("cannot create migration runtime: {error}"));

    runtime.block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(&schema_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap_or_else(|error| panic!("cannot create query schema: {error}"));
        let migrator = Migrator::new(migrations_path)
            .await
            .unwrap_or_else(|error| panic!("cannot load migrations: {error}"));
        migrator
            .run(&pool)
            .await
            .unwrap_or_else(|error| panic!("cannot migrate query schema: {error}"));
        pool.close().await;
    });

    let normalized = schema_path.to_string_lossy().replace('\\', "/");
    println!("cargo:rustc-env=DATABASE_URL=sqlite://{normalized}");
}
