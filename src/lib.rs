#![feature(trait_alias)]
mod path;

use path::PathBuilder;
pub use path::{Comparison, Direction, Format, InlineCount};

use hyper::{
    body::Buf,
    client::{connect::Connect, Client},
    http::uri::{Authority, InvalidUri, Scheme},
    Body, Response, Uri,
};
use log::debug;
use serde::{de::DeserializeOwned, Deserialize};
use std::{convert::TryFrom, io::Read};
use thiserror::Error;

/// Umbrella trait covering all the traits required of a [`Client`] for a [`DataSource`] to work.
pub trait Connector = Connect + Clone + Send + Sync + 'static;

/// Represents a target OData API.
pub struct DataSource<C> {
    client: Client<C>,
    authority: Authority,
    base_path: String,
    scheme: Scheme,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid URI")]
    Uri(#[from] InvalidUri),
    #[error("http error")]
    Http(#[from] hyper::http::Error),
    #[error("hyper error")]
    Hyper(#[from] hyper::Error),
    #[error("serde error")]
    Serde(serde_json::Error, String),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

/// Wraps lists of Resources returned by the API. Used for deserializing ListRequest responses.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub value: Vec<T>,
    #[serde(rename = "odata.count")]
    pub count: Option<String>,
    #[serde(rename = "odata.nextLink")]
    pub next_link: Option<String>,
    #[serde(rename = "odata.metadata")]
    pub metadata: Option<String>,
}

async fn deserialize_as<T: DeserializeOwned>(response: Response<Body>) -> Result<T, Error> {
    let body = hyper::body::aggregate(response).await?;

    let mut content = String::new();
    body.reader().read_to_string(&mut content)?;

    serde_json::from_str(&content).map_err(|e| Error::Serde(e, content))
}

impl<C> DataSource<C>
where
    C: Connector,
{
    /// Construct a new DataSource using a [`Client`], [`Authority`] and a base_domain.
    /// ```rust
    /// # use hyper::{Client, client::HttpConnector};
    /// # use hyper_openssl::{HttpsConnector};
    /// # use odata_simple_client::DataSource;
    /// # let client: Client<HttpsConnector<HttpConnector>> =
    /// #   Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());
    /// #
    /// let datasource = DataSource::new(
    ///     client,
    ///     "oda.ft.dk",
    ///     Some(String::from("/api"))
    /// ).unwrap();
    /// ```
    pub fn new<A>(
        client: Client<C>,
        domain: A,
        base_path: Option<String>,
    ) -> Result<DataSource<C>, Error>
    where
        Authority: TryFrom<A>,
        Error: From<<Authority as TryFrom<A>>::Error>,
    {
        Ok(DataSource {
            client,
            authority: Authority::try_from(domain)?,
            base_path: base_path.unwrap_or_default(),
            scheme: Scheme::HTTPS,
        })
    }

    async fn execute<R>(&self, request: R) -> Result<Response<Body>, Error>
    where
        R: Into<PathBuilder>,
    {
        let builder: PathBuilder = request.into().base_path(self.base_path.clone());

        let uri = Uri::builder()
            .scheme(self.scheme.as_ref())
            .authority(self.authority.as_ref())
            .path_and_query(builder.build()?)
            .build()?;

        debug!("fetching {}", uri);
        Ok(self.client.get(uri).await?)
    }

    /// Fetch a single resource using a [`GetRequest`]
    /// ```rust
    /// # use hyper::{Client, client::HttpConnector};
    /// # use hyper_openssl::{HttpsConnector};
    /// # use odata_simple_client::{DataSource, GetRequest};
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
    /// # tokio_test::block_on(async {
    /// let dokument: Dokument = datasource.fetch(
    ///         GetRequest::new("Dokument", 24)
    ///     ).await.unwrap();
    ///
    /// assert_eq!(dokument.titel, "Grund- og nærhedsnotat vedr. sanktioner på toldområdet");
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

    /// Fetch a [`Page`]d list of resources using a [`ListRequest`]
    /// ```rust
    /// # use hyper::{Client, client::HttpConnector};
    /// # use hyper_openssl::{HttpsConnector};
    /// # use odata_simple_client::{DataSource, ListRequest, Page, InlineCount};
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
    /// # tokio_test::block_on(async {
    /// let page: Page<Dokument> = datasource
    ///     .fetch_paged(ListRequest::new("Dokument")
    ///         .inline_count(InlineCount::AllPages)
    ///     ).await.unwrap();
    /// assert!(page.count.unwrap().parse::<u32>().unwrap() > 0)
    /// # });
    /// ```
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

/// Request a single resource by ID
pub struct GetRequest {
    builder: PathBuilder,
}

impl<'a> GetRequest {
    /// Constructs a GET request for `<DataSource Path>/resource_type(id)`
    ///
    /// Must be [`DataSource::fetch`]ed using a [`DataSource`] to retrieve data.
    pub fn new(resource_type: &str, id: usize) -> Self {
        GetRequest {
            builder: PathBuilder::new(resource_type.to_string()).id(id),
        }
    }

    /// Change format of the returned data.
    ///
    /// Can be either [`Format::Json`] or [`Format::Xml`]
    pub fn format(mut self, format: Format) -> Self {
        self.builder = self.builder.format(format);
        self
    }

    /// Expand specific relations of the returned object, if possible.
    ///
    /// For the [Folketinget API](https://oda.ft.dk) for example, you can expand the `DokumentAktør` field of a `Dokument`, to simultaneously retrieve information about the document authors, instead of having to do two separate lookups for the `DokumentAktør` relation and then the actual `Aktør`.
    pub fn expand<'f, F>(mut self, field: F) -> Self
    where
        F: IntoIterator<Item = &'f str>,
    {
        self.builder = self.builder.expand(field);
        self
    }
}

impl From<GetRequest> for PathBuilder {
    fn from(request: GetRequest) -> Self {
        request.builder
    }
}

/// Request a list of resources.
pub struct ListRequest {
    builder: PathBuilder,
}

impl<'a> ListRequest {
    pub fn new(resource_type: &str) -> Self {
        ListRequest {
            builder: PathBuilder::new(resource_type.to_string()),
        }
    }

    /// Change format of the returned data.
    ///
    /// Can be either [`Format::Json`] or [`Format::Xml`]
    pub fn format(mut self, format: Format) -> Self {
        self.builder = self.builder.format(format);
        self
    }

    /// Order the returned resources by `field`, optionally in specified `direction`. [`Direction::Ascending`] by default.
    pub fn order_by(mut self, field: &str, direction: Option<Direction>) -> Self {
        self.builder = self.builder.order_by(field, direction);
        self
    }

    /// Only retrieve the top `count` items.
    pub fn top(mut self, count: u32) -> Self {
        self.builder = self.builder.top(count);
        self
    }

    /// Skip the first `count` items.
    pub fn skip(mut self, count: u32) -> Self {
        self.builder = self.builder.skip(count);
        self
    }

    /// Include an inline count field in the odata page metadata.
    /// Useful for gauging how many results/pages are left. By default this is not specified, which implies [`InlineCount::None`]
    pub fn inline_count(mut self, value: InlineCount) -> Self {
        self.builder = self.builder.inline_count(value);
        self
    }

    /// Filter the returned results using an OData conditional expression.
    /// See [the OData 2.0 documentation (section 4.5)](https://www.odata.org/documentation/odata-version-2-0/uri-conventions/) for more information.
    /// Fetch a [`Page`]d list of resources using a [`ListRequest`]
    /// ```rust
    /// # use hyper::{Client, client::HttpConnector};
    /// # use hyper_openssl::{HttpsConnector};
    /// # use odata_simple_client::{DataSource, ListRequest, Page, Comparison};
    /// # use serde::Deserialize;
    /// #
    /// # let client: Client<HttpsConnector<HttpConnector>> =
    /// #   Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());
    /// #
    /// # let datasource = DataSource::new(client, "oda.ft.dk", Some(String::from("/api"))).unwrap();
    /// #
    /// #[derive(Deserialize, Debug)]
    /// struct Dokument {
    ///     titel: String,
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let page: Page<Dokument> = datasource
    ///     .fetch_paged(ListRequest::new("Dokument")
    ///         .filter("id", Comparison::Equal, "24")
    ///     ).await.unwrap();
    /// assert_eq!(page.value[0].titel, "Grund- og nærhedsnotat vedr. sanktioner på toldområdet")
    /// # });
    /// ```
    pub fn filter(mut self, field: &str, comparison: Comparison, value: &str) -> Self {
        self.builder = self.builder.filter(field, comparison, value);
        self
    }

    pub fn expand<'f, F>(mut self, field: F) -> Self
    where
        F: IntoIterator<Item = &'f str>,
    {
        self.builder = self.builder.expand(field);
        self
    }
}

impl From<ListRequest> for PathBuilder {
    fn from(request: ListRequest) -> Self {
        request.builder
    }
}

#[cfg(test)]
mod tests {
    use crate::{DataSource, Direction, GetRequest, ListRequest};
    use hyper::{client::HttpConnector, Client};
    use hyper_openssl::HttpsConnector;

    #[tokio::test]
    async fn test_get_request() {
        let client: Client<HttpsConnector<HttpConnector>> =
            Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());

        let datasource = DataSource::new(client, "oda.ft.dk", Some(String::from("/api"))).unwrap();

        let response = datasource
            .execute(GetRequest::new("Dokument", 1).expand(["DokumentAktør"]))
            .await
            .unwrap();

        let data = crate::deserialize_as::<serde_json::Value>(response).await;
        println!("{:#?}", data);
    }

    #[tokio::test]
    async fn test_list_request() {
        let client: Client<HttpsConnector<HttpConnector>> =
            Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());

        let datasource = DataSource::new(client, "oda.ft.dk", Some(String::from("/api"))).unwrap();

        let response = datasource
            .execute(
                ListRequest::new("Dokument")
                    .expand(["DokumentAktør"])
                    .order_by("id", Some(Direction::Descending))
                    .top(1),
            )
            .await
            .unwrap();

        let data = crate::deserialize_as::<serde_json::Value>(response).await;
        println!("{:#?}", data);
    }
}
