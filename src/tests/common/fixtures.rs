use deadpool::managed::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use rstest::fixture;
use starknet::core::types::FieldElement;

use crate::config::{Config, NetworkName};

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
pub async fn test_config() -> Config {
    Config::new(
        NetworkName::Katana,
        FieldElement::from_hex_be(
            "0x06df335982dddce41008e4c03f2546fa27276567b5274c7d0c1262f3c2b5d167",
        )
        .unwrap(),
        vec!["ETH/USD".to_string(), "BTC/USD".to_string()],
    )
    .await
}
