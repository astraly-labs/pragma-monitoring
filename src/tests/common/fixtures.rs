use std::sync::Arc;

use arc_swap::Guard;
use deadpool::managed::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use rstest::fixture;
use starknet::core::types::FieldElement;

use crate::config::{config_force_init, get_config, Config, ConfigInput, NetworkName};

#[fixture]
pub fn database() -> Pool<AsyncDieselConnectionManager<diesel_async::AsyncPgConnection>> {
    // Setup database connection
    let database_url = "postgres://postgres:postgres@localhost:5432/postgres";
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    let pool: Pool<AsyncDieselConnectionManager<diesel_async::AsyncPgConnection>> =
        Pool::builder(config).build().unwrap();

    pool
}

#[fixture]
pub async fn test_config() -> Guard<Arc<Config>> {
    config_force_init(ConfigInput {
        network: NetworkName::Katana,
        oracle_address: FieldElement::from_hex_be(
            "0x06df335982dddce41008e4c03f2546fa27276567b5274c7d0c1262f3c2b5d167",
        )
        .unwrap(),
        pairs: vec!["ETH/USD".to_string(), "BTC/USD".to_string()],
    })
    .await;
    get_config(None).await
}
