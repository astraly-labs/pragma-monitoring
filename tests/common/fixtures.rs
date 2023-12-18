use deadpool::managed::{Object, Pool};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use rstest::fixture;
use starknet::providers::{jsonrpc::HttpTransport, JsonRpcClient};
use url::Url;

#[fixture]
pub async fn setup_database(
) -> Object<AsyncDieselConnectionManager<diesel_async::AsyncPgConnection>> {
    // Setup database connection
    let database_url = "postgres://postgres:postgres@localhost:5432/postgres";
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    let pool: Pool<AsyncDieselConnectionManager<diesel_async::AsyncPgConnection>> =
        Pool::builder(config).build().unwrap();

    let conn = pool.get().await.unwrap();

    conn
}

#[fixture]
pub async fn starknet_provider() -> JsonRpcClient<HttpTransport> {
    let url = Url::parse("http://localhost:5050").expect("Invalid JSON RPC URL");
    JsonRpcClient::new(HttpTransport::new(url)) // Katana URL
}
