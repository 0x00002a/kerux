#[cfg(feature = "storage-postgres")]
extern crate tokio_postgres as pg;

use actix_web::{
    web::{self, Data, JsonConfig},
    App,
};
use error::Error;
use fs_err::tokio::read_to_string;
use serde::Deserialize;
use state::StateResolver;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::EnvFilter;

mod client_api;
mod error;
mod events;
mod state;
mod storage;
mod util;
mod validate;

use storage::StorageManager;
use util::{domain::Domain, StorageExt};

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    #[serde(rename = "sled")]
    Sled,
    #[serde(rename = "mem")]
    InMemory,
}
#[derive(Deserialize)]
pub struct Config {
    domain: Domain,
    bind_address: SocketAddr,
    storage: DatabaseType,
}

pub struct ServerState {
    pub config: Config,
    pub db_pool: Box<dyn StorageManager>,
    pub state_resolver: StateResolver,
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    run().await.map_err(|e| {
        eprintln!("Error starting the server: {}", e);
        std::io::Error::from(std::io::ErrorKind::Other)
    })
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config: Config = toml::from_str(&read_to_string("config.toml").await?)?;
    let db_pool = match config.storage {
        DatabaseType::InMemory => {
            Box::new(storage::mem::MemStorageManager::new()) as Box<dyn StorageManager>
        }
        DatabaseType::Sled => Box::new(storage::sled::SledStorage::new("sled")?) as _,
    };
    let state_resolver = StateResolver::new(db_pool.get_handle().await?);
    let server_state = Arc::new(ServerState {
        config,
        db_pool,
        state_resolver,
    });

    let server_state2 = Arc::clone(&server_state);
    actix_web::HttpServer::new(move || {
        App::new()
            .app_data(Data::new(Arc::clone(&server_state)))
            .app_data(Data::new(
                JsonConfig::default().error_handler(|e, _req| Error::from(e).into()),
            ))
            .service(web::scope("/_matrix/client").configure(client_api::configure_endpoints))
            .service(util::print_the_world)
            .wrap(
                actix_cors::Cors::default()
                    .send_wildcard()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
                    .allowed_headers(vec![
                        "Origin",
                        "X-Requested-With",
                        "Content-Type",
                        "Accept",
                        "Authorization",
                    ]),
            )
    })
    .bind(server_state2.config.bind_address)?
    .run()
    .await?;
    Ok(())
}
