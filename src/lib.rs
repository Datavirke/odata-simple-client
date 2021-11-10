#![feature(trait_alias)]
mod path;

use path::PathBuilder;
pub use path::{Comparison, Format, Order};

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

pub trait Connector = Connect + Clone + Send + Sync + 'static;

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

async fn extract_as<T: DeserializeOwned>(response: Response<Body>) -> Result<T, Error> {
    let body = hyper::body::aggregate(response).await?;

    let mut content = String::new();
    body.reader().read_to_string(&mut content)?;

    serde_json::from_str(&content).map_err(|e| Error::Serde(e, content))
}

impl<C> DataSource<C>
where
    C: Connector,
{
    pub fn new<A>(client: Client<C>, domain: A, base_path: String) -> Result<DataSource<C>, Error>
    where
        Authority: TryFrom<A>,
        Error: From<<Authority as TryFrom<A>>::Error>,
    {
        Ok(DataSource {
            client,
            authority: Authority::try_from(domain)?,
            base_path,
            scheme: Scheme::HTTPS,
        })
    }

    async fn execute<R>(&self, request: R) -> Result<Response<Body>, Error>
    where
        R: Into<PathBuilder>,
    {
        let mut builder: PathBuilder = request.into();
        builder.base_path = self.base_path.clone();

        let uri = Uri::builder()
            .scheme(self.scheme.as_ref())
            .authority(self.authority.as_ref())
            .path_and_query(builder.build()?)
            .build()?;

        debug!("fetching {}", uri);
        Ok(self.client.get(uri).await?)
    }

    pub async fn fetch<T>(&self, request: GetRequest) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let response = self.execute(request).await?;
        extract_as::<T>(response).await
    }

    pub async fn fetch_paged<T>(&self, request: ListRequest) -> Result<Page<T>, Error>
    where
        T: DeserializeOwned,
    {
        let response = self.execute(request).await?;
        extract_as::<Page<T>>(response).await
    }
}

pub struct GetRequest {
    builder: PathBuilder,
}

impl<'a> GetRequest {
    pub fn new(resource_type: &str, id: usize) -> Self {
        GetRequest {
            builder: PathBuilder::new(resource_type.to_string()).id(id),
        }
    }

    pub fn format(mut self, format: Format) -> Self {
        self.builder = self.builder.format(format);
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

impl From<GetRequest> for PathBuilder {
    fn from(request: GetRequest) -> Self {
        request.builder
    }
}

pub struct ListRequest {
    builder: PathBuilder,
}

impl<'a> ListRequest {
    pub fn new(resource_type: &str) -> Self {
        ListRequest {
            builder: PathBuilder::new(resource_type.to_string()),
        }
    }

    pub fn format(mut self, format: Format) -> Self {
        self.builder = self.builder.format(format);
        self
    }

    pub fn order_by(mut self, field: &str, order: Option<Order>) -> Self {
        self.builder = self.builder.order_by(field, order);
        self
    }

    pub fn top(mut self, count: u32) -> Self {
        self.builder = self.builder.top(count);
        self
    }

    pub fn skip(mut self, count: u32) -> Self {
        self.builder = self.builder.skip(count);
        self
    }

    pub fn inline_count(mut self, value: String) -> Self {
        self.builder = self.builder.inline_count(value);
        self
    }

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
    use crate::{DataSource, GetRequest, ListRequest, Order};
    use hyper::{client::HttpConnector, Client};
    use hyper_openssl::HttpsConnector;

    #[tokio::test]
    async fn test_get_request() {
        let client: Client<HttpsConnector<HttpConnector>> =
            Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());

        let datasource = DataSource::new(client, "oda.ft.dk", String::from("/api")).unwrap();

        let response = datasource
            .execute(GetRequest::new("Dokument", 1).expand(["DokumentAktør"]))
            .await
            .unwrap();

        let data = crate::extract_as::<serde_json::Value>(response).await;
        println!("{:#?}", data);
    }

    #[tokio::test]
    async fn test_list_request() {
        let client: Client<HttpsConnector<HttpConnector>> =
            Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());

        let datasource = DataSource::new(client, "oda.ft.dk", String::from("/api")).unwrap();

        let response = datasource
            .execute(
                ListRequest::new("Dokument")
                    .expand(["DokumentAktør"])
                    .order_by("id", Some(Order::Descending))
                    .top(1),
            )
            .await
            .unwrap();

        let data = crate::extract_as::<serde_json::Value>(response).await;
        println!("{:#?}", data);
    }
}
