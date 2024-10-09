use deadpool::managed::Pool;
use diesel::prelude::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

use crate::constants::{
    DISPATCH_EVENT_FEED_LATEST_BLOCK_UPDATE, DISPATCH_EVENT_LATEST_BLOCK,
    DISPATCH_EVENT_NB_FEEDS_UPDATED,
};
use crate::schema::pragma_devnet_dispatch_event::dsl as dispatch_dsl;
use crate::{config::get_config, error::MonitoringError, models::PragmaDevnetDispatchEvent};

/// Read the database of the indexed Dispatch events and populate the metrics:
/// * dispatch_event_latest_block,
/// * dispatch_event_feed_latest_block_update,
/// * dispatch_event_nb_feeds_updated.
pub async fn process_dispatch_events(
    pool: Pool<AsyncDieselConnectionManager<AsyncPgConnection>>,
) -> Result<(), MonitoringError> {
    let config = get_config(None).await;
    let network = config.network_str();

    let mut conn = pool.get().await.map_err(MonitoringError::Connection)?;

    // Query the latest dispatch event
    let latest_event = dispatch_dsl::pragma_devnet_dispatch_event
        .filter(dispatch_dsl::network.eq(network))
        .order(dispatch_dsl::block_number.desc())
        .first::<PragmaDevnetDispatchEvent>(&mut conn)
        .await
        .optional()?;

    if let Some(event) = latest_event {
        DISPATCH_EVENT_LATEST_BLOCK
            .with_label_values(&[network])
            .set(event.block_number as f64);

        if let Some(feeds_updated) = event.feeds_updated {
            let nb_feeds_updated = feeds_updated.len() as f64;
            DISPATCH_EVENT_NB_FEEDS_UPDATED
                .with_label_values(&[network, &event.block_number.to_string()])
                .set(nb_feeds_updated);

            for feed in feeds_updated {
                DISPATCH_EVENT_FEED_LATEST_BLOCK_UPDATE
                    .with_label_values(&[network, &feed])
                    .set(event.block_number as f64);
            }
        }
    }

    Ok(())
}
