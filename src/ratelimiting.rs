use crate::{
    deserialize_as, path::Format, Connector, DataSource, Error, GetRequest, ListRequest, Page,
    PathBuilder,
};
use hyper::{Body, Response};
use serde::de::DeserializeOwned;
use std::{num::NonZeroU32, sync::Arc};

pub type RateLimiter = governor::RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::QuantaClock,
>;
pub use governor::Quota;

/// Rate-limited wrapper around a DataSource. Requires the 'rate-limiter' feature to be enabled.
/// Cloning the RateLimitedDataSource shares the rate-limiting mechanism between the two copies,
/// preserving the rate-limiting guarantees across all of them.
#[derive(Clone)]
pub struct RateLimitedDataSource<C>
where
    C: Connector,
{
    datasource: DataSource<C>,
    rate_limiter: Arc<RateLimiter>,
}

impl<C> RateLimitedDataSource<C>
where
    C: Connector,
{
    /// Construct a RateLimitedDataSource from an existing [`DataSource`], and a [`Quota`]
    pub fn new(datasource: DataSource<C>, quota: Quota) -> Self {
        Self {
            datasource,
            rate_limiter: Arc::new(RateLimiter::direct(quota)),
        }
    }

    /// Construct a RateLimitedResource from an existing [`DataSource`], 
    /// and a non-zero integer indicating the maximum number of requests
    /// the DataSource should serve per second.
    pub fn per_second(datasource: DataSource<C>, per_second: NonZeroU32) -> Self {
        Self::new(datasource, Quota::per_second(per_second))
    }

    async fn execute<R>(&self, request: R) -> Result<Response<Body>, Error>
    where
        R: Into<PathBuilder>,
    {
        self.rate_limiter.until_ready().await;
        self.datasource.execute(request).await
    }

    /// Fetch two resources on a datasource rate-limited to one per second,
    /// and assert more than a second passed in total.
    /// ```rust
    /// # use hyper::{Client, client::HttpConnector};
    /// # use hyper_openssl::{HttpsConnector};
    /// # use odata_simple_client::{RateLimitedDataSource, DataSource, GetRequest};
    /// # use serde::Deserialize;
    /// #
    /// # let client: Client<HttpsConnector<HttpConnector>> =
    /// #   Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());
    /// #
    /// # let datasource = DataSource::new(client, "oda.ft.dk", Some(String::from("/api"))).unwrap();
    /// #
    /// #[derive(Deserialize)]
    /// struct Dokument {
    ///     titel: String,
    /// }
    ///
    /// let datasource = RateLimitedDataSource::per_second(
    ///     datasource,
    ///     std::num::NonZeroU32::new(1u32).unwrap()
    /// );
    ///
    /// let start = std::time::Instant::now();
    ///
    /// # tokio_test::block_on(async {
    /// let first: Dokument = datasource.fetch(
    ///         GetRequest::new("Dokument", 24)
    ///     ).await.unwrap();
    ///
    /// let second: Dokument = datasource.fetch(
    ///         GetRequest::new("Dokument", 26)
    ///     ).await.unwrap();
    ///
    /// assert!(start.elapsed().as_millis() >= 1000);
    ///
    /// # assert_eq!(first.titel, "Grund- og nærhedsnotat vedr. sanktioner på toldområdet");
    /// # assert_eq!(second.titel, "Revideret grund- og nærhedsnotat om sanktioner på toldområdet\n");
    /// # });
    /// ```
    pub async fn fetch<T>(&self, request: GetRequest) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let response = self
            .execute(Into::<PathBuilder>::into(request).format(Format::Json))
            .await?;
        deserialize_as::<T>(response).await
    }

    pub async fn fetch_paged<T>(&self, request: ListRequest) -> Result<Page<T>, Error>
    where
        T: DeserializeOwned,
    {
        let response = self
            .execute(Into::<PathBuilder>::into(request).format(Format::Json))
            .await?;
        deserialize_as::<Page<T>>(response).await
    }
}
